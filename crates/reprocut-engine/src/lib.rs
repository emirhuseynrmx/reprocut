//! End-to-end reduction orchestration for ReproCut.

mod scheduler;
mod pipeline;
mod python_isolation;

pub use python_isolation::{PythonIsolationRequest, PythonPreparationError};
pub use scheduler::{CandidatePlan, FrontierOutcome, FrontierScheduler, SchedulerError};

use pipeline::{
    manifest_candidates, syntax_candidates, PipelineError, StructuredCandidate, SyntaxPhase,
};
use python_isolation::{FrozenPythonPreparation, PreparedPythonCandidate};

use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

use reprocut_adapters::{Ecosystem, NpmManifest, PreparationPlan};
use reprocut_core::{
    reduce_hierarchical_frontiers, AggregateDecision, AggregateEvidence, CandidateRank,
    CandidateVerdict, ContentDigest, DiagnosticChannel, EvaluationPolicy, ExecutionObservation,
    FailureFingerprint, FailureOracle, FrontierClass, OracleError, OracleMode, OracleSpec,
    ReductionResult, ReductionUnit,
};
use reprocut_runner::{CommandSpec, ProcessRunner, RunnerError};
use reprocut_state::{
    AttemptEventRecord, AttemptRecord, SessionContract, StateError, StateStore, TransitionRecord,
    WriterHandle,
};
use reprocut_workspace::{
    CandidateWorkspace, DirectoryHierarchy, InventoryPolicy, ProjectInventory, ProjectSnapshot,
    WorkspaceError,
};
use thiserror::Error;

/// Selects whether a run is ephemeral, new, resumed, or an explicit restart.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionMode {
    /// Keep only memory-local cache for compatibility and tests.
    Ephemeral,
    /// Create state and refuse to hide an existing resumable session.
    Create(PathBuf),
    /// Resume only an exactly compatible existing session.
    Resume(PathBuf),
    /// Start a new session while retaining earlier database history.
    Restart(PathBuf),
}

/// Authority granted to candidate dependency preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationMode {
    /// Never run an ecosystem preparation command.
    None,
    /// Permit built-in network-disabled commands with lifecycle scripts disabled.
    Offline,
    /// Permit network-disabled npm lifecycle scripts explicitly.
    LifecycleScripts,
    /// Trust the caller-provided Python command as isolated for dependency edits.
    IsolatedPython,
}

/// A complete reduction request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionRequest {
    source_root: PathBuf,
    program: PathBuf,
    arguments: Vec<OsString>,
    timeout: Duration,
    max_output_bytes: usize,
    diagnostic_channel: DiagnosticChannel,
    oracle_spec: OracleSpec,
    evaluation_policy: EvaluationPolicy,
    jobs: usize,
    session_mode: SessionMode,
    inventory_policy: InventoryPolicy,
    ecosystem: Ecosystem,
    preparation_mode: PreparationMode,
    python_isolation: Option<PythonIsolationRequest>,
}

impl ReductionRequest {
    /// Creates a request for one failing command.
    pub fn new(
        source_root: PathBuf,
        program: PathBuf,
        arguments: Vec<OsString>,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Self {
        Self {
            source_root,
            program,
            arguments,
            timeout,
            max_output_bytes,
            diagnostic_channel: DiagnosticChannel::Auto,
            oracle_spec: OracleSpec::automatic(DiagnosticChannel::Auto),
            evaluation_policy: EvaluationPolicy::strict(),
            jobs: 1,
            session_mode: SessionMode::Ephemeral,
            inventory_policy: InventoryPolicy::source_only(),
            ecosystem: Ecosystem::None,
            preparation_mode: PreparationMode::None,
            python_isolation: None,
        }
    }

    /// Returns a request using an explicit failure channel and aggregate policy.
    pub fn with_evaluation(
        mut self,
        diagnostic_channel: DiagnosticChannel,
        evaluation_policy: EvaluationPolicy,
    ) -> Self {
        self.diagnostic_channel = diagnostic_channel;
        self.oracle_spec = OracleSpec::automatic(diagnostic_channel);
        self.evaluation_policy = evaluation_policy;
        self
    }

    /// Returns a request using one prevalidated oracle contract.
    pub fn with_oracle(mut self, oracle_spec: OracleSpec) -> Self {
        self.diagnostic_channel = oracle_spec.channel();
        self.oracle_spec = oracle_spec;
        self
    }

    /// Returns a request with bounded parallelism and an explicit state policy.
    pub fn with_runtime(mut self, jobs: usize, session_mode: SessionMode) -> Self {
        self.jobs = jobs;
        self.session_mode = session_mode;
        self
    }

    /// Returns a request using exact nested-directory inventory exclusions.
    pub fn with_inventory_policy(mut self, inventory_policy: InventoryPolicy) -> Self {
        self.inventory_policy = inventory_policy;
        self
    }

    /// Enables ecosystem-aware preparation and structured reducer selection.
    pub fn with_ecosystem(
        mut self,
        ecosystem: Ecosystem,
        preparation_mode: PreparationMode,
    ) -> Self {
        self.ecosystem = ecosystem;
        self.preparation_mode = preparation_mode;
        self
    }

    /// Enables frozen-wheelhouse Python isolation for every execution phase.
    pub fn with_python_isolation(mut self, isolation: PythonIsolationRequest) -> Self {
        self.ecosystem = Ecosystem::Python;
        self.preparation_mode = PreparationMode::IsolatedPython;
        self.python_isolation = Some(isolation);
        self
    }

    /// Returns the original project root.
    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    /// Returns the child program.
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Returns the child argument vector.
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the per-run timeout.
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the per-stream capture budget.
    pub const fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }

    /// Returns the process-stream selection policy.
    pub const fn diagnostic_channel(&self) -> DiagnosticChannel {
        self.diagnostic_channel
    }

    /// Returns the complete validated failure-recognition contract.
    pub const fn oracle_spec(&self) -> &OracleSpec {
        &self.oracle_spec
    }

    /// Returns the repeated-execution policy.
    pub const fn evaluation_policy(&self) -> EvaluationPolicy {
        self.evaluation_policy
    }

    /// Returns requested worker count; zero means detected hardware parallelism.
    pub const fn jobs(&self) -> usize {
        self.jobs
    }

    /// Returns the durable-session policy.
    pub const fn session_mode(&self) -> &SessionMode {
        &self.session_mode
    }

    /// Returns exclusions applied before source inventory allocation.
    pub const fn inventory_policy(&self) -> &InventoryPolicy {
        &self.inventory_policy
    }

    /// Returns the explicitly selected project family.
    pub const fn ecosystem(&self) -> Ecosystem {
        self.ecosystem
    }

    /// Returns candidate preparation authority.
    pub const fn preparation_mode(&self) -> PreparationMode {
        self.preparation_mode
    }

    /// Returns the explicit Python isolation contract, when configured.
    pub const fn python_isolation(&self) -> Option<&PythonIsolationRequest> {
        self.python_isolation.as_ref()
    }
}

/// A completed, repeatedly verified reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionOutcome {
    source_snapshot_digest: ContentDigest,
    preparation_digest: ContentDigest,
    original_files: usize,
    original_bytes: u64,
    original_lines: u64,
    reduction: ReductionResult,
    fingerprint: FailureFingerprint,
    baseline_runs: u16,
    final_verifications: u16,
    inconclusive_attempts: u64,
    cache_hits: u64,
    state_path: Option<PathBuf>,
    resumed: bool,
    snapshot: ProjectSnapshot,
    structured_attempts: u64,
    accepted_structured_edits: Vec<String>,
    elapsed: Duration,
    attempt_events: Vec<AttemptEventRecord>,
}

impl ReductionOutcome {
    /// Returns the immutable source tree identity used by every candidate.
    pub const fn source_snapshot_digest(&self) -> ContentDigest {
        self.source_snapshot_digest
    }

    /// Returns the complete built-in or isolated preparation contract identity.
    pub const fn preparation_digest(&self) -> ContentDigest {
        self.preparation_digest
    }

    /// Returns the original regular-file count.
    pub const fn original_files(&self) -> usize {
        self.original_files
    }

    /// Returns source bytes hashed during the immutable session contract scan.
    pub const fn original_bytes(&self) -> u64 {
        self.original_bytes
    }

    /// Returns newline-delimited source records measured during that scan.
    pub const fn original_lines(&self) -> u64 {
        self.original_lines
    }

    /// Returns the deterministic reduction result.
    pub const fn reduction(&self) -> &ReductionResult {
        &self.reduction
    }

    /// Returns the preserved failure identity.
    pub const fn fingerprint(&self) -> &FailureFingerprint {
        &self.fingerprint
    }

    /// Returns the number of baseline observations.
    pub const fn baseline_runs(&self) -> u16 {
        self.baseline_runs
    }

    /// Returns the number of final verification executions.
    pub const fn final_verifications(&self) -> u16 {
        self.final_verifications
    }

    /// Returns candidates not accepted because evidence was incomplete.
    pub const fn inconclusive_attempts(&self) -> u64 {
        self.inconclusive_attempts
    }

    /// Returns candidate executions avoided by the content cache.
    pub const fn cache_hits(&self) -> u64 {
        self.cache_hits
    }

    /// Returns the durable journal path when state was enabled.
    pub fn state_path(&self) -> Option<&Path> {
        self.state_path.as_deref()
    }

    /// Reports whether the run reused an existing compatible session.
    pub const fn resumed(&self) -> bool {
        self.resumed
    }

    /// Returns the exact immutable project that passed final verification.
    pub const fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    /// Returns structured manifest/syntax candidates that reached a terminal verdict.
    pub const fn structured_attempts(&self) -> u64 {
        self.structured_attempts
    }

    /// Returns accepted manifest/syntax edit keys in fixpoint order.
    pub fn accepted_structured_edits(&self) -> &[String] {
        &self.accepted_structured_edits
    }

    /// Returns end-to-end wall time including baseline and final verification.
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Returns durable append-only attempt evidence, including resumed history.
    ///
    /// Ephemeral library requests have no persistent event ledger and return an
    /// empty slice; command-line runs always use durable state.
    pub fn attempt_events(&self) -> &[AttemptEventRecord] {
        &self.attempt_events
    }
}

/// A reduction orchestration failure.
#[derive(Debug, Error)]
pub enum EngineError {
    /// Project inventory or candidate materialization failed.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    /// A child command could not be executed reliably.
    #[error(transparent)]
    Runner(#[from] RunnerError),
    /// Baseline observations could not form a stable oracle.
    #[error(transparent)]
    Oracle(#[from] OracleError),
    /// There are no regular files to reduce.
    #[error("project contains no regular files")]
    EmptyProject,
    /// The supplied command completed successfully.
    #[error("baseline command succeeded; ReproCut requires a failure")]
    BaselineSucceeded,
    /// A required offline baseline preparation command did not succeed.
    #[error("baseline preparation failed; the failure oracle was not created")]
    BaselinePreparationFailed,
    /// The final candidate did not repeatedly preserve the failure.
    #[error("final verification did not preserve the configured failure")]
    FinalVerificationFailed,
    /// Durable state could not be safely created, validated, or updated.
    #[error(transparent)]
    State(#[from] StateError),
    /// A parallel frontier violated its total-order contract.
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    /// A manifest, syntax, or snapshot candidate could not be constructed safely.
    #[error("structured reduction failed: {0}")]
    Pipeline(String),
    /// Cached structured evidence could not be materialized under the current environment.
    #[error("a preserved structured candidate could not be prepared for publication")]
    StructuredRealizationFailed,
    /// Isolated Python was selected without a complete frozen-input contract.
    #[error("isolated Python preparation requires an explicit isolation request")]
    MissingPythonIsolation,
    /// Python preparation or command resolution failed closed.
    #[error(transparent)]
    PythonPreparation(#[from] PythonPreparationError),
    /// A generated candidate referenced an invalid inventory index.
    #[error("candidate referenced an invalid inventory unit")]
    InvalidCandidate,
}

impl From<PipelineError> for EngineError {
    fn from(error: PipelineError) -> Self {
        Self::Pipeline(error.to_string())
    }
}

/// Stateless entry point for deterministic project reduction.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReductionEngine;

impl ReductionEngine {
    /// Stabilizes, minimizes, and re-verifies one failing command.
    #[allow(clippy::too_many_lines)]
    pub fn run(request: &ReductionRequest) -> Result<ReductionOutcome, EngineError> {
        let started = Instant::now();
        let inventory =
            ProjectInventory::scan_with_policy(request.source_root(), request.inventory_policy())?;
        if inventory.units().is_empty() {
            return Err(EngineError::EmptyProject);
        }
        let source_snapshot = ProjectSnapshot::capture(&inventory, request.inventory_policy())?;
        let source_digest = source_snapshot.digest();
        let original_measurements = source_snapshot.measurements();
        let python_preparation = request
            .python_isolation()
            .map(|isolation| {
                FrozenPythonPreparation::capture(
                    isolation,
                    request.timeout(),
                    request.max_output_bytes(),
                )
            })
            .transpose()?;
        if let Some(preparation) = &python_preparation {
            preparation.validate_original_program(request.program())?;
        }
        if request.preparation_mode() == PreparationMode::IsolatedPython
            && python_preparation.is_none()
        {
            return Err(EngineError::MissingPythonIsolation);
        }
        let preparation_digest = python_preparation
            .as_ref()
            .map(FrozenPythonPreparation::digest)
            .unwrap_or_else(|| builtin_preparation_digest(request));
        let contract = session_contract(request, source_digest, preparation_digest);
        let (state, resumed) = open_state(request.session_mode(), contract)?;
        let state_path = state.as_ref().map(|store| store.path().to_path_buf());
        let writer = state.as_ref().map(StateStore::writer);
        let all_units = inventory.units().iter().collect::<Vec<_>>();
        let policy = request.evaluation_policy();
        let mut baselines = Vec::with_capacity(usize::from(policy.runs()));

        for _ in 0..policy.runs() {
            let observation = match run_candidate(
                request,
                &source_snapshot,
                &all_units,
                python_preparation.as_ref(),
            )? {
                CandidateExecution::Observed(observation) => observation,
                CandidateExecution::PreparationRejected => {
                    return Err(EngineError::BaselinePreparationFailed);
                }
            };
            if request.oracle_spec().mode() != OracleMode::ExitZero
                && policy == EvaluationPolicy::strict()
                && observation.exit_code() == Some(0)
                && observation.signal().is_none()
            {
                return Err(EngineError::BaselineSucceeded);
            }
            baselines.push(observation);
        }
        let oracle = stabilize_oracle(request.oracle_spec(), policy, &baselines)?;
        let first_error = Mutex::new(None::<EngineError>);
        let attempts_by_digest = Mutex::new(HashMap::<ContentDigest, AttemptRecord>::new());
        let memory_cache = Mutex::new(HashMap::<ContentDigest, AttemptRecord>::new());
        let inconclusive_attempts = AtomicU64::new(0);
        let cache_hits = AtomicU64::new(0);
        let mut from_digest = source_digest;
        let mut transition_ordinal = 0_u64;
        let mut frontier_phase = 0_u16;

        let hierarchy = DirectoryHierarchy::from_units(inventory.units());
        let directory_groups = hierarchy.directory_unit_ids();
        let reduction =
            reduce_hierarchical_frontiers(inventory.units(), &directory_groups, |frontier| {
                if has_error(&first_error) {
                    return vec![None; frontier.len()];
                }
                let phase = frontier_phase;
                frontier_phase = frontier_phase.saturating_add(1);
                let mut unique_by_digest = HashMap::<ContentDigest, usize>::new();
                let mut unique_plans = Vec::with_capacity(frontier.len());
                let mut slot_to_unique = Vec::with_capacity(frontier.len());
                let mut slot_cache_digests = Vec::with_capacity(frontier.len());
                let mut slot_material_digests = Vec::with_capacity(frontier.len());
                let mut slot_material_bytes = Vec::with_capacity(frontier.len());

                for (slot, candidate) in frontier.iter().enumerate() {
                    let unit_ids = candidate.iter().map(|unit| unit.id()).collect::<Vec<_>>();
                    let Ok(candidate_snapshot) = source_snapshot.subset(candidate.iter().copied())
                    else {
                        set_error(&first_error, EngineError::InvalidCandidate);
                        return vec![None; frontier.len()];
                    };
                    let material_digest = candidate_snapshot.digest();
                    let cache_digest = candidate_cache_digest(
                        material_digest,
                        request.oracle_spec().digest(),
                        preparation_digest,
                    );
                    slot_cache_digests.push(cache_digest);
                    slot_material_digests.push(material_digest);
                    slot_material_bytes.push(candidate_snapshot.total_bytes());
                    if let Some(&unique) = unique_by_digest.get(&cache_digest) {
                        slot_to_unique.push(unique);
                        continue;
                    }
                    let Ok(start) = u32::try_from(slot) else {
                        set_error(&first_error, EngineError::InvalidCandidate);
                        return vec![None; frontier.len()];
                    };
                    let unique = unique_plans.len();
                    unique_by_digest.insert(cache_digest, unique);
                    slot_to_unique.push(unique);
                    unique_plans.push(CandidatePlan::new(
                        CandidateRank::new(
                            phase,
                            u32::try_from(frontier.len()).unwrap_or(u32::MAX),
                            FrontierClass::Structured,
                            start,
                            cache_digest,
                        ),
                        CandidatePayload {
                            unit_ids,
                            digest: cache_digest,
                        },
                    ));
                }

                let evaluation = FrontierEvaluationContext {
                    request,
                    inventory: &inventory,
                    source_snapshot: &source_snapshot,
                    python_preparation: python_preparation.as_ref(),
                    oracle: &oracle,
                    policy,
                    writer: writer.as_ref(),
                    memory_cache: &memory_cache,
                    attempts_by_digest: &attempts_by_digest,
                    first_error: &first_error,
                    inconclusive_attempts: &inconclusive_attempts,
                    cache_hits: &cache_hits,
                };
                let scheduled =
                    FrontierScheduler::evaluate(unique_plans, request.jobs(), |payload| {
                        evaluation.evaluate(payload)
                    });
                let outcome = match scheduled {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        set_error(&first_error, EngineError::Scheduler(error));
                        return vec![None; frontier.len()];
                    }
                };
                let mut verdicts = slot_to_unique
                    .iter()
                    .map(|&unique| outcome.verdict(unique))
                    .collect::<Vec<_>>();

                let winner = loop {
                    let Some(winner) = earliest_terminal_preserved(&verdicts) else {
                        break None;
                    };
                    if slot_material_digests[winner] == from_digest {
                        verdicts[winner] = Some(CandidateVerdict::Rejected);
                        continue;
                    }
                    break Some(winner);
                };
                if let Some(winner) = winner {
                    if let Some(writer) = &writer {
                        let attempt = lock(&attempts_by_digest)
                            .get(&slot_cache_digests[winner])
                            .cloned();
                        let Some(attempt) = attempt else {
                            set_error(&first_error, EngineError::InvalidCandidate);
                            return vec![None; frontier.len()];
                        };
                        let transition = TransitionRecord::new(
                            transition_ordinal,
                            from_digest,
                            slot_material_digests[winner],
                            slot_cache_digests[winner],
                            slot_material_bytes[winner],
                        );
                        if let Err(error) = writer.accept_transition(attempt, transition) {
                            set_error(&first_error, EngineError::State(error));
                            verdicts.fill(None);
                            return verdicts;
                        }
                    }
                    from_digest = slot_material_digests[winner];
                    transition_ordinal = transition_ordinal.saturating_add(1);
                }
                verdicts
            });
        if let Some(error) = take_error(&first_error) {
            return Err(error);
        }

        let snapshot = source_snapshot.subset(reduction.kept())?;
        from_digest = snapshot.digest();
        let structured = StructuredReductionContext {
            request,
            python_preparation: python_preparation.as_ref(),
            preparation_digest,
            oracle: &oracle,
            policy,
            writer: writer.as_ref(),
            memory_cache: &memory_cache,
            attempts_by_digest: &attempts_by_digest,
            first_error: &first_error,
            inconclusive_attempts: &inconclusive_attempts,
            cache_hits: &cache_hits,
        };
        let structured_outcome = structured.reduce(
            snapshot,
            &mut frontier_phase,
            &mut transition_ordinal,
            &mut from_digest,
        )?;
        let snapshot = structured_outcome.snapshot;
        let mut final_error = None;
        let final_evidence = policy.aggregate(std::iter::from_fn(|| {
            if final_error.is_some() {
                return None;
            }
            Some(
                match run_snapshot_candidate(request, &snapshot, python_preparation.as_ref()) {
                    Ok(CandidateExecution::Observed(observation)) => oracle.classify(&observation),
                    Ok(CandidateExecution::PreparationRejected) => CandidateVerdict::Rejected,
                    Err(error) => {
                        final_error = Some(error);
                        CandidateVerdict::Inconclusive
                    }
                },
            )
        }));
        if let Some(error) = final_error {
            return Err(error);
        }
        if final_evidence.decision() != AggregateDecision::Preserved {
            return Err(EngineError::FinalVerificationFailed);
        }
        let attempt_events = writer
            .as_ref()
            .map(WriterHandle::attempt_events)
            .transpose()?
            .unwrap_or_default();

        Ok(ReductionOutcome {
            source_snapshot_digest: source_digest,
            preparation_digest,
            original_files: inventory.units().len(),
            original_bytes: original_measurements.bytes(),
            original_lines: original_measurements.lines(),
            reduction,
            fingerprint: oracle.fingerprint().clone(),
            baseline_runs: u16::try_from(baselines.len())
                .expect("evaluation policy run count is represented by u16"),
            final_verifications: final_evidence.observed_runs(),
            inconclusive_attempts: inconclusive_attempts.load(Ordering::Relaxed),
            cache_hits: cache_hits.load(Ordering::Relaxed),
            state_path,
            resumed,
            snapshot,
            structured_attempts: structured_outcome.attempts,
            accepted_structured_edits: structured_outcome.accepted,
            elapsed: started.elapsed(),
            attempt_events,
        })
    }
}

fn stabilize_oracle(
    spec: &OracleSpec,
    policy: EvaluationPolicy,
    baselines: &[ExecutionObservation],
) -> Result<FailureOracle, OracleError> {
    if policy == EvaluationPolicy::strict() {
        return FailureOracle::from_spec_and_baselines(spec.clone(), baselines);
    }

    let mut best = None::<(u16, usize, FailureOracle)>;
    for left in 0..baselines.len() {
        for right in (left + 1)..baselines.len() {
            if is_success(&baselines[left]) || is_success(&baselines[right]) {
                continue;
            }
            let pair = [&baselines[left], &baselines[right]];
            let pair = [pair[0].clone(), pair[1].clone()];
            let Ok(candidate) = FailureOracle::from_spec_and_baselines(spec.clone(), &pair) else {
                continue;
            };
            let evidence = policy.aggregate(
                baselines
                    .iter()
                    .map(|observation| candidate.classify(observation)),
            );
            if evidence.decision() != AggregateDecision::Preserved {
                continue;
            }
            let score_count = baselines
                .iter()
                .filter(|observation| {
                    candidate.classify(observation) == CandidateVerdict::Preserved
                })
                .count();
            let score = u16::try_from(score_count)
                .expect("evaluation policy run count is represented by u16");
            let replace = best
                .as_ref()
                .map(|(best_score, best_index, _)| {
                    score > *best_score || (score == *best_score && left < *best_index)
                })
                .unwrap_or(true);
            if replace {
                best = Some((score, left, candidate));
            }
        }
    }
    best.map(|(_, _, oracle)| oracle)
        .ok_or(OracleError::UnstableDiagnostic)
}

fn is_success(observation: &ExecutionObservation) -> bool {
    observation.exit_code() == Some(0) && observation.signal().is_none()
}

const fn aggregate_verdict(decision: AggregateDecision) -> CandidateVerdict {
    match decision {
        AggregateDecision::Preserved => CandidateVerdict::Preserved,
        AggregateDecision::Rejected => CandidateVerdict::Rejected,
        AggregateDecision::Inconclusive => CandidateVerdict::Inconclusive,
    }
}

fn run_candidate(
    request: &ReductionRequest,
    source_snapshot: &ProjectSnapshot,
    kept: &[&ReductionUnit],
    python_preparation: Option<&FrozenPythonPreparation>,
) -> Result<CandidateExecution, EngineError> {
    let snapshot = source_snapshot.subset(kept.iter().copied())?;
    run_snapshot_candidate(request, &snapshot, python_preparation)
}

fn run_snapshot_candidate(
    request: &ReductionRequest,
    snapshot: &ProjectSnapshot,
    python_preparation: Option<&FrozenPythonPreparation>,
) -> Result<CandidateExecution, EngineError> {
    let candidate = CandidateWorkspace::materialize_snapshot(snapshot)?;
    if !prepare_candidate(request, candidate.root())? {
        return Ok(CandidateExecution::PreparationRejected);
    }
    let command = if let Some(preparation) = python_preparation {
        let Some(prepared) = preparation.prepare(
            candidate.root(),
            request.timeout(),
            request.max_output_bytes(),
        )?
        else {
            return Ok(CandidateExecution::PreparationRejected);
        };
        prepared.command_for(
            request.program(),
            request.arguments(),
            request.timeout(),
            request.max_output_bytes(),
        )?
    } else {
        CommandSpec::new(
            request.program.clone(),
            request.arguments.clone(),
            candidate.root().to_path_buf(),
            request.timeout,
            request.max_output_bytes,
        )
    };
    Ok(CandidateExecution::Observed(ProcessRunner::run(&command)?))
}

enum CandidateExecution {
    Observed(ExecutionObservation),
    PreparationRejected,
}

fn prepare_candidate(request: &ReductionRequest, root: &Path) -> Result<bool, EngineError> {
    let Some(plan) = global_preparation(request) else {
        return Ok(true);
    };
    run_preparation(request, root, &plan)
}

fn global_preparation(request: &ReductionRequest) -> Option<PreparationPlan> {
    if request.ecosystem() != Ecosystem::Npm || request.preparation_mode() == PreparationMode::None
    {
        return None;
    }
    Some(NpmManifest::preparation(
        request.preparation_mode() == PreparationMode::LifecycleScripts,
    ))
}

fn run_preparation(
    request: &ReductionRequest,
    root: &Path,
    plan: &PreparationPlan,
) -> Result<bool, EngineError> {
    for preparation in plan.commands() {
        let command = CommandSpec::new(
            PathBuf::from(preparation.program()),
            preparation.arguments().to_vec(),
            root.to_path_buf(),
            request.timeout,
            request.max_output_bytes,
        );
        let observation = ProcessRunner::run(&command)?;
        if !is_success(&observation) {
            return Ok(false);
        }
    }
    Ok(true)
}

struct StructuredReductionOutcome {
    snapshot: ProjectSnapshot,
    attempts: u64,
    accepted: Vec<String>,
}

struct StructuredFrontierOutcome {
    accepted: Option<(String, ProjectSnapshot)>,
    attempts: u64,
}

struct StructuredReductionContext<'a> {
    request: &'a ReductionRequest,
    python_preparation: Option<&'a FrozenPythonPreparation>,
    preparation_digest: ContentDigest,
    oracle: &'a FailureOracle,
    policy: EvaluationPolicy,
    writer: Option<&'a WriterHandle>,
    memory_cache: &'a Mutex<HashMap<ContentDigest, AttemptRecord>>,
    attempts_by_digest: &'a Mutex<HashMap<ContentDigest, AttemptRecord>>,
    first_error: &'a Mutex<Option<EngineError>>,
    inconclusive_attempts: &'a AtomicU64,
    cache_hits: &'a AtomicU64,
}

impl StructuredReductionContext<'_> {
    fn reduce(
        &self,
        mut snapshot: ProjectSnapshot,
        frontier_phase: &mut u16,
        transition_ordinal: &mut u64,
        from_digest: &mut ContentDigest,
    ) -> Result<StructuredReductionOutcome, EngineError> {
        let mut attempts = 0_u64;
        let mut accepted = Vec::new();
        'fixpoint: loop {
            let manifests = manifest_candidates(
                &snapshot,
                self.request.ecosystem(),
                self.request.preparation_mode(),
            )?;
            let manifest_outcome =
                self.evaluate_frontier(manifests, frontier_phase, transition_ordinal, from_digest)?;
            attempts = attempts.saturating_add(manifest_outcome.attempts);
            if let Some((key, next)) = manifest_outcome.accepted {
                accepted.push(key);
                snapshot = next;
                continue 'fixpoint;
            }

            for syntax_phase in [SyntaxPhase::Delete, SyntaxPhase::Hoist] {
                let syntax = syntax_candidates(&snapshot, syntax_phase)?;
                let syntax_outcome = self.evaluate_frontier(
                    syntax,
                    frontier_phase,
                    transition_ordinal,
                    from_digest,
                )?;
                attempts = attempts.saturating_add(syntax_outcome.attempts);
                if let Some((key, next)) = syntax_outcome.accepted {
                    accepted.push(key);
                    snapshot = next;
                    continue 'fixpoint;
                }
            }
            break;
        }
        Ok(StructuredReductionOutcome {
            snapshot,
            attempts,
            accepted,
        })
    }

    fn evaluate_frontier(
        &self,
        candidates: Vec<StructuredCandidate>,
        frontier_phase: &mut u16,
        transition_ordinal: &mut u64,
        from_digest: &mut ContentDigest,
    ) -> Result<StructuredFrontierOutcome, EngineError> {
        if candidates.is_empty() {
            return Ok(StructuredFrontierOutcome {
                accepted: None,
                attempts: 0,
            });
        }
        let phase = *frontier_phase;
        *frontier_phase = frontier_phase.saturating_add(1);
        let granularity = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        let plans = candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                let digest = structured_candidate_digest(
                    &candidate,
                    self.request.oracle_spec().digest(),
                    self.preparation_digest,
                );
                CandidatePlan::new(
                    CandidateRank::new(
                        phase,
                        granularity,
                        FrontierClass::Structured,
                        u32::try_from(index).unwrap_or(u32::MAX),
                        digest,
                    ),
                    StructuredPayload { candidate, digest },
                )
            })
            .collect::<Vec<_>>();
        let realized = Mutex::new(HashMap::<ContentDigest, ProjectSnapshot>::new());
        let evaluator = StructuredEvaluationContext {
            shared: self,
            realized: &realized,
            current_digest: *from_digest,
        };
        let outcome = FrontierScheduler::evaluate(plans, self.request.jobs(), |payload| {
            evaluator.evaluate(payload)
        })?;
        let observed = u64::try_from(
            outcome
                .verdicts()
                .iter()
                .filter(|verdict| verdict.is_some())
                .count(),
        )
        .unwrap_or(u64::MAX);
        if let Some(error) = take_error(self.first_error) {
            return Err(error);
        }
        let Some(winner) = outcome.winner() else {
            return Ok(StructuredFrontierOutcome {
                accepted: None,
                attempts: observed,
            });
        };
        let payload = winner.payload();
        let realized = lock(&realized).get(&payload.digest).cloned();
        let realized = match realized {
            Some(realized) => realized,
            None => self
                .realize(&payload.candidate)?
                .map(|prepared| prepared.snapshot)
                .ok_or(EngineError::StructuredRealizationFailed)?,
        };
        if let Some(writer) = self.writer {
            let attempt = lock(self.attempts_by_digest)
                .get(&payload.digest)
                .cloned()
                .ok_or(EngineError::InvalidCandidate)?;
            writer.accept_transition(
                attempt,
                TransitionRecord::new(
                    *transition_ordinal,
                    *from_digest,
                    realized.digest(),
                    payload.digest,
                    realized.total_bytes(),
                ),
            )?;
        }
        *from_digest = realized.digest();
        *transition_ordinal = transition_ordinal.saturating_add(1);
        Ok(StructuredFrontierOutcome {
            accepted: Some((payload.candidate.key().to_owned(), realized)),
            attempts: observed,
        })
    }

    fn realize(
        &self,
        candidate: &StructuredCandidate,
    ) -> Result<Option<PreparedStructuredCandidate>, EngineError> {
        let workspace = CandidateWorkspace::materialize_snapshot(candidate.snapshot())?;
        if !prepare_candidate(self.request, workspace.root())? {
            return Ok(None);
        }
        if let Some(preparation) = candidate.preparation() {
            if !run_preparation(self.request, workspace.root(), preparation)? {
                return Ok(None);
            }
        }
        let snapshot = candidate
            .snapshot()
            .capture_prepared(workspace.root(), candidate.capture_paths())?;
        let python = if let Some(preparation) = self.python_preparation {
            let Some(prepared) = preparation.prepare(
                workspace.root(),
                self.request.timeout(),
                self.request.max_output_bytes(),
            )?
            else {
                return Ok(None);
            };
            Some(prepared)
        } else {
            None
        };
        Ok(Some(PreparedStructuredCandidate {
            workspace,
            snapshot,
            python,
        }))
    }

    fn run_candidate(
        &self,
        candidate: &StructuredCandidate,
    ) -> Result<StructuredExecution, EngineError> {
        let Some(prepared) = self.realize(candidate)? else {
            return Ok(StructuredExecution::PreparationRejected);
        };
        let command = if let Some(python) = &prepared.python {
            python.command_for(
                self.request.program(),
                self.request.arguments(),
                self.request.timeout(),
                self.request.max_output_bytes(),
            )?
        } else {
            CommandSpec::new(
                self.request.program.clone(),
                self.request.arguments.clone(),
                prepared.workspace.root().to_path_buf(),
                self.request.timeout,
                self.request.max_output_bytes,
            )
        };
        Ok(StructuredExecution::Observed {
            observation: ProcessRunner::run(&command)?,
            realized: prepared.snapshot,
        })
    }
}

#[derive(Clone, Debug)]
struct StructuredPayload {
    candidate: StructuredCandidate,
    digest: ContentDigest,
}

struct PreparedStructuredCandidate {
    workspace: CandidateWorkspace,
    snapshot: ProjectSnapshot,
    python: Option<PreparedPythonCandidate>,
}

enum StructuredExecution {
    Observed {
        observation: ExecutionObservation,
        realized: ProjectSnapshot,
    },
    PreparationRejected,
}

struct StructuredEvaluationContext<'context, 'request> {
    shared: &'context StructuredReductionContext<'request>,
    realized: &'context Mutex<HashMap<ContentDigest, ProjectSnapshot>>,
    current_digest: ContentDigest,
}

impl StructuredEvaluationContext<'_, '_> {
    fn evaluate(&self, payload: &StructuredPayload) -> CandidateVerdict {
        if has_error(self.shared.first_error) {
            return CandidateVerdict::Inconclusive;
        }
        if let Some(record) = lock(self.shared.memory_cache).get(&payload.digest).cloned() {
            self.shared.cache_hits.fetch_add(1, Ordering::Relaxed);
            lock(self.shared.attempts_by_digest).insert(payload.digest, record.clone());
            return record.verdict();
        }
        if let Some(writer) = self.shared.writer {
            match writer.lookup_cache(payload.digest) {
                Ok(Some(cached)) => {
                    let record = AttemptRecord::new(
                        payload.digest,
                        cached.verdict(),
                        cached.observed_runs(),
                        cached.inconclusive_runs(),
                        cached.evidence_json().to_owned(),
                    );
                    self.shared.cache_hits.fetch_add(1, Ordering::Relaxed);
                    lock(self.shared.memory_cache).insert(payload.digest, record.clone());
                    lock(self.shared.attempts_by_digest).insert(payload.digest, record.clone());
                    return record.verdict();
                }
                Ok(None) => {}
                Err(error) => {
                    set_error(self.shared.first_error, EngineError::State(error));
                    return CandidateVerdict::Inconclusive;
                }
            }
        }

        let local_error = Mutex::new(None);
        let mut realized = None::<ProjectSnapshot>;
        let mut nondeterministic_preparation = false;
        let evidence = self.shared.policy.aggregate(std::iter::from_fn(|| {
            if has_error(&local_error) {
                return None;
            }
            Some(match self.shared.run_candidate(&payload.candidate) {
                Ok(StructuredExecution::PreparationRejected) => CandidateVerdict::Rejected,
                Ok(StructuredExecution::Observed {
                    observation,
                    realized: current,
                }) => {
                    if realized
                        .as_ref()
                        .is_some_and(|previous| previous.digest() != current.digest())
                    {
                        nondeterministic_preparation = true;
                        CandidateVerdict::Inconclusive
                    } else {
                        let material_changed = current.digest() != self.current_digest;
                        realized = Some(current);
                        if material_changed {
                            self.shared.oracle.classify(&observation)
                        } else {
                            CandidateVerdict::Rejected
                        }
                    }
                }
                Err(error) => {
                    set_error(&local_error, error);
                    CandidateVerdict::Inconclusive
                }
            })
        }));
        if let Some(error) = take_error(&local_error) {
            set_error(self.shared.first_error, error);
            return CandidateVerdict::Inconclusive;
        }
        let verdict = if nondeterministic_preparation {
            CandidateVerdict::Inconclusive
        } else {
            aggregate_verdict(evidence.decision())
        };
        if verdict == CandidateVerdict::Inconclusive {
            self.shared
                .inconclusive_attempts
                .fetch_add(1, Ordering::Relaxed);
        }
        if verdict == CandidateVerdict::Preserved {
            if let Some(realized) = realized {
                lock(self.realized).insert(payload.digest, realized);
            }
        }
        let record = AttemptRecord::new(
            payload.digest,
            verdict,
            evidence.observed_runs(),
            evidence.inconclusive_runs(),
            if nondeterministic_preparation {
                nondeterministic_preparation_json(&evidence)
            } else {
                evidence_json(&evidence)
            },
        );
        if let Some(writer) = self.shared.writer {
            if let Err(error) = writer.record_attempt(record.clone()) {
                set_error(self.shared.first_error, EngineError::State(error));
                return CandidateVerdict::Inconclusive;
            }
        }
        if verdict != CandidateVerdict::Inconclusive {
            lock(self.shared.memory_cache).insert(payload.digest, record.clone());
        }
        lock(self.shared.attempts_by_digest).insert(payload.digest, record);
        verdict
    }
}

fn structured_candidate_digest(
    candidate: &StructuredCandidate,
    oracle_spec: ContentDigest,
    preparation_digest: ContentDigest,
) -> ContentDigest {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(b"REPROCUT-STRUCTURED-V2\0");
    encoded.extend_from_slice(candidate.snapshot().digest().as_bytes());
    encoded.extend_from_slice(oracle_spec.as_bytes());
    encoded.extend_from_slice(preparation_digest.as_bytes());
    if let Some(preparation) = candidate.preparation() {
        for command in preparation.commands() {
            encode_field(&mut encoded, command.program().to_string_lossy().as_bytes());
            for argument in command.arguments() {
                encode_field(&mut encoded, argument.to_string_lossy().as_bytes());
            }
        }
    }
    ContentDigest::of(&encoded)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidatePayload {
    unit_ids: Vec<u32>,
    digest: ContentDigest,
}

struct FrontierEvaluationContext<'a> {
    request: &'a ReductionRequest,
    inventory: &'a ProjectInventory,
    source_snapshot: &'a ProjectSnapshot,
    python_preparation: Option<&'a FrozenPythonPreparation>,
    oracle: &'a FailureOracle,
    policy: EvaluationPolicy,
    writer: Option<&'a WriterHandle>,
    memory_cache: &'a Mutex<HashMap<ContentDigest, AttemptRecord>>,
    attempts_by_digest: &'a Mutex<HashMap<ContentDigest, AttemptRecord>>,
    first_error: &'a Mutex<Option<EngineError>>,
    inconclusive_attempts: &'a AtomicU64,
    cache_hits: &'a AtomicU64,
}

impl FrontierEvaluationContext<'_> {
    fn evaluate(&self, payload: &CandidatePayload) -> CandidateVerdict {
        if has_error(self.first_error) {
            return CandidateVerdict::Inconclusive;
        }
        if let Some(record) = lock(self.memory_cache).get(&payload.digest).cloned() {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            lock(self.attempts_by_digest).insert(payload.digest, record.clone());
            return record.verdict();
        }
        if let Some(writer) = self.writer {
            match writer.lookup_cache(payload.digest) {
                Ok(Some(cached)) => {
                    let record = AttemptRecord::new(
                        payload.digest,
                        cached.verdict(),
                        cached.observed_runs(),
                        cached.inconclusive_runs(),
                        cached.evidence_json().to_owned(),
                    );
                    self.cache_hits.fetch_add(1, Ordering::Relaxed);
                    lock(self.memory_cache).insert(payload.digest, record.clone());
                    lock(self.attempts_by_digest).insert(payload.digest, record.clone());
                    return record.verdict();
                }
                Ok(None) => {}
                Err(error) => {
                    set_error(self.first_error, EngineError::State(error));
                    return CandidateVerdict::Inconclusive;
                }
            }
        }

        let kept = match payload
            .unit_ids
            .iter()
            .map(|&id| {
                let index = usize::try_from(id).map_err(|_| EngineError::InvalidCandidate)?;
                self.inventory
                    .units()
                    .get(index)
                    .ok_or(EngineError::InvalidCandidate)
            })
            .collect::<Result<Vec<_>, EngineError>>()
        {
            Ok(kept) => kept,
            Err(error) => {
                set_error(self.first_error, error);
                return CandidateVerdict::Inconclusive;
            }
        };
        let local_error = Mutex::new(None);
        let evidence = self.policy.aggregate(std::iter::from_fn(|| {
            if has_error(&local_error) {
                return None;
            }
            Some(
                match run_candidate(
                    self.request,
                    self.source_snapshot,
                    &kept,
                    self.python_preparation,
                ) {
                    Ok(CandidateExecution::Observed(observation)) => {
                        self.oracle.classify(&observation)
                    }
                    Ok(CandidateExecution::PreparationRejected) => CandidateVerdict::Rejected,
                    Err(error) => {
                        set_error(&local_error, error);
                        CandidateVerdict::Inconclusive
                    }
                },
            )
        }));
        if let Some(error) = take_error(&local_error) {
            set_error(self.first_error, error);
            return CandidateVerdict::Inconclusive;
        }
        let verdict = aggregate_verdict(evidence.decision());
        if verdict == CandidateVerdict::Inconclusive {
            self.inconclusive_attempts.fetch_add(1, Ordering::Relaxed);
        }
        let record = AttemptRecord::new(
            payload.digest,
            verdict,
            evidence.observed_runs(),
            evidence.inconclusive_runs(),
            evidence_json(&evidence),
        );
        if let Some(writer) = self.writer {
            if let Err(error) = writer.record_attempt(record.clone()) {
                set_error(self.first_error, EngineError::State(error));
                return CandidateVerdict::Inconclusive;
            }
        }
        if verdict != CandidateVerdict::Inconclusive {
            lock(self.memory_cache).insert(payload.digest, record.clone());
        }
        lock(self.attempts_by_digest).insert(payload.digest, record);
        verdict
    }
}

fn evidence_json(evidence: &AggregateEvidence) -> String {
    serde_json::json!({
        "decision": match evidence.decision() {
            AggregateDecision::Preserved => "preserved",
            AggregateDecision::Rejected => "rejected",
            AggregateDecision::Inconclusive => "inconclusive",
        },
        "observed_runs": evidence.observed_runs(),
        "preserved_runs": evidence.preserved_runs(),
        "rejected_runs": evidence.rejected_runs(),
        "inconclusive_runs": evidence.inconclusive_runs(),
        "wilson_95": evidence.wilson_95().map(|interval| {
            serde_json::json!({"lower": interval.lower(), "upper": interval.upper()})
        }),
    })
    .to_string()
}

fn nondeterministic_preparation_json(evidence: &AggregateEvidence) -> String {
    serde_json::json!({
        "decision": "inconclusive",
        "reason": "nondeterministic_preparation",
        "observed_runs": evidence.observed_runs(),
        "preserved_runs": evidence.preserved_runs(),
        "rejected_runs": evidence.rejected_runs(),
        "inconclusive_runs": evidence.inconclusive_runs(),
    })
    .to_string()
}

fn open_state(
    mode: &SessionMode,
    contract: SessionContract,
) -> Result<(Option<StateStore>, bool), EngineError> {
    match mode {
        SessionMode::Ephemeral => Ok((None, false)),
        SessionMode::Create(path) => Ok((Some(StateStore::create(path, contract)?), false)),
        SessionMode::Resume(path) => Ok((Some(StateStore::resume(path, contract)?), true)),
        SessionMode::Restart(path) => Ok((Some(StateStore::restart(path, contract)?), false)),
    }
}

fn session_contract(
    request: &ReductionRequest,
    source: ContentDigest,
    preparation_digest: ContentDigest,
) -> SessionContract {
    let mut command = Vec::new();
    command.extend_from_slice(b"REPROCUT-COMMAND-V2\0");
    encode_field(
        &mut command,
        request.program().as_os_str().to_string_lossy().as_bytes(),
    );
    command.extend_from_slice(
        &u64::try_from(request.arguments().len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    for argument in request.arguments() {
        encode_field(&mut command, argument.to_string_lossy().as_bytes());
    }
    command.extend_from_slice(&request.timeout().as_nanos().to_le_bytes());
    command.extend_from_slice(
        &u64::try_from(request.max_output_bytes())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    let mut adapter_version = String::from("files-v3");
    adapter_version.push(char::from(
        b'0' + match request.ecosystem() {
            Ecosystem::Cargo => 1,
            Ecosystem::Python => 2,
            Ecosystem::Npm => 3,
            Ecosystem::None => 0,
        },
    ));
    adapter_version.push(char::from(
        b'0' + match request.preparation_mode() {
            PreparationMode::None => 0,
            PreparationMode::Offline => 1,
            PreparationMode::LifecycleScripts => 2,
            PreparationMode::IsolatedPython => 3,
        },
    ));
    for name in request.inventory_policy().excluded_directory_names() {
        adapter_version.push('\0');
        adapter_version.push_str(name);
    }
    SessionContract::new_v2(
        source,
        ContentDigest::of(&command),
        request.oracle_spec().digest(),
        preparation_digest,
        evaluation_policy_digest(request.evaluation_policy()),
        adapter_version,
        env!("CARGO_PKG_VERSION").to_owned(),
    )
}

fn candidate_cache_digest(
    snapshot: ContentDigest,
    oracle_spec: ContentDigest,
    preparation: ContentDigest,
) -> ContentDigest {
    let mut encoded = Vec::with_capacity(128);
    encoded.extend_from_slice(b"REPROCUT-CANDIDATE-CACHE-V2\0");
    encoded.extend_from_slice(snapshot.as_bytes());
    encoded.extend_from_slice(oracle_spec.as_bytes());
    encoded.extend_from_slice(preparation.as_bytes());
    ContentDigest::of(&encoded)
}

fn evaluation_policy_digest(policy: EvaluationPolicy) -> ContentDigest {
    let mut encoded = Vec::with_capacity(32);
    encoded.extend_from_slice(b"REPROCUT-POLICY-V1\0");
    encoded.push(match policy {
        EvaluationPolicy::Strict => 0,
        EvaluationPolicy::Flaky { .. } => 1,
    });
    encoded.extend_from_slice(&policy.runs().to_le_bytes());
    encoded.extend_from_slice(&policy.required().to_le_bytes());
    ContentDigest::of(&encoded)
}

fn builtin_preparation_digest(request: &ReductionRequest) -> ContentDigest {
    let mut encoded = Vec::with_capacity(64);
    encoded.extend_from_slice(b"REPROCUT-BUILTIN-PREPARATION-V1\0");
    encoded.push(match request.ecosystem() {
        Ecosystem::None => 0,
        Ecosystem::Cargo => 1,
        Ecosystem::Python => 2,
        Ecosystem::Npm => 3,
    });
    encoded.push(match request.preparation_mode() {
        PreparationMode::None => 0,
        PreparationMode::Offline => 1,
        PreparationMode::LifecycleScripts => 2,
        PreparationMode::IsolatedPython => 3,
    });
    ContentDigest::of(&encoded)
}

fn encode_field(encoded: &mut Vec<u8>, bytes: &[u8]) {
    encoded.extend_from_slice(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    encoded.extend_from_slice(bytes);
}

fn earliest_terminal_preserved(verdicts: &[Option<CandidateVerdict>]) -> Option<usize> {
    for (index, verdict) in verdicts.iter().enumerate() {
        match verdict {
            Some(CandidateVerdict::Preserved) => return Some(index),
            Some(CandidateVerdict::Rejected | CandidateVerdict::Inconclusive) => {}
            None => return None,
        }
    }
    None
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn has_error(error: &Mutex<Option<EngineError>>) -> bool {
    lock(error).is_some()
}

fn set_error(slot: &Mutex<Option<EngineError>>, error: EngineError) {
    let mut slot = lock(slot);
    if slot.is_none() {
        *slot = Some(error);
    }
}

fn take_error(slot: &Mutex<Option<EngineError>>) -> Option<EngineError> {
    lock(slot).take()
}

#[cfg(test)]
mod measurement_tests {
    use reprocut_workspace::{InventoryPolicy, ProjectInventory, ProjectSnapshot};
    use std::fs;

    #[test]
    fn source_identity_and_measurements_share_one_snapshot() {
        let root = tempfile::tempdir().expect("temporary project");
        fs::write(root.path().join("a.txt"), b"one\ntwo").expect("text fixture");
        fs::write(root.path().join("b.bin"), b"\0\n").expect("binary fixture");
        let policy = InventoryPolicy::source_only();
        let inventory =
            ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
        let snapshot = ProjectSnapshot::capture(&inventory, &policy).expect("snapshot");

        assert_eq!(snapshot.measurements().files(), 2);
        assert_eq!(snapshot.measurements().bytes(), 9);
        assert_eq!(snapshot.measurements().lines(), 3);
    }
}
