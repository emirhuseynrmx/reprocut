//! End-to-end reduction orchestration for ReproCut.

mod scheduler;

pub use scheduler::{CandidatePlan, FrontierOutcome, FrontierScheduler, SchedulerError};

use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use reprocut_core::{
    reduce_hierarchical_frontiers, AggregateDecision, AggregateEvidence, CandidateRank,
    CandidateVerdict, ContentDigest, DiagnosticChannel, EvaluationPolicy, ExecutionObservation,
    FailureFingerprint, FailureOracle, FrontierClass, OracleError, ReductionResult, ReductionUnit,
};
use reprocut_runner::{CommandSpec, ProcessRunner, RunnerError};
use reprocut_state::{
    AttemptRecord, SessionContract, StateError, StateStore, TransitionRecord, WriterHandle,
};
use reprocut_workspace::{
    CandidateWorkspace, DirectoryHierarchy, ProjectInventory, WorkspaceError,
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

/// A complete reduction request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionRequest {
    source_root: PathBuf,
    program: PathBuf,
    arguments: Vec<OsString>,
    timeout: Duration,
    max_output_bytes: usize,
    diagnostic_channel: DiagnosticChannel,
    evaluation_policy: EvaluationPolicy,
    jobs: usize,
    session_mode: SessionMode,
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
            evaluation_policy: EvaluationPolicy::strict(),
            jobs: 1,
            session_mode: SessionMode::Ephemeral,
        }
    }

    /// Returns a request using an explicit failure channel and aggregate policy.
    pub fn with_evaluation(
        mut self,
        diagnostic_channel: DiagnosticChannel,
        evaluation_policy: EvaluationPolicy,
    ) -> Self {
        self.diagnostic_channel = diagnostic_channel;
        self.evaluation_policy = evaluation_policy;
        self
    }

    /// Returns a request with bounded parallelism and an explicit state policy.
    pub fn with_runtime(mut self, jobs: usize, session_mode: SessionMode) -> Self {
        self.jobs = jobs;
        self.session_mode = session_mode;
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
}

/// A completed, repeatedly verified reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionOutcome {
    original_files: usize,
    reduction: ReductionResult,
    fingerprint: FailureFingerprint,
    baseline_runs: u16,
    final_verifications: u16,
    inconclusive_attempts: u64,
    cache_hits: u64,
    state_path: Option<PathBuf>,
    resumed: bool,
}

impl ReductionOutcome {
    /// Returns the original regular-file count.
    pub const fn original_files(&self) -> usize {
        self.original_files
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
    /// The final candidate did not repeatedly preserve the failure.
    #[error("final verification did not preserve the configured failure")]
    FinalVerificationFailed,
    /// Durable state could not be safely created, validated, or updated.
    #[error(transparent)]
    State(#[from] StateError),
    /// A parallel frontier violated its total-order contract.
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    /// A generated candidate referenced an invalid inventory index.
    #[error("candidate referenced an invalid inventory unit")]
    InvalidCandidate,
}

/// Stateless entry point for deterministic project reduction.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReductionEngine;

impl ReductionEngine {
    /// Stabilizes, minimizes, and re-verifies one failing command.
    #[allow(clippy::too_many_lines)]
    pub fn run(request: &ReductionRequest) -> Result<ReductionOutcome, EngineError> {
        let inventory = ProjectInventory::scan(request.source_root())?;
        if inventory.units().is_empty() {
            return Err(EngineError::EmptyProject);
        }
        let source_digest = inventory_digest(&inventory)?;
        let contract = session_contract(request, source_digest);
        let (state, resumed) = open_state(request.session_mode(), contract)?;
        let state_path = state.as_ref().map(|store| store.path().to_path_buf());
        let writer = state.as_ref().map(StateStore::writer);
        let all_units = inventory.units().iter().collect::<Vec<_>>();
        let policy = request.evaluation_policy();
        let mut baselines = Vec::with_capacity(usize::from(policy.runs()));

        for _ in 0..policy.runs() {
            let observation = run_candidate(request, &inventory, &all_units)?;
            if policy == EvaluationPolicy::strict()
                && observation.exit_code() == Some(0)
                && observation.signal().is_none()
            {
                return Err(EngineError::BaselineSucceeded);
            }
            baselines.push(observation);
        }
        let oracle = stabilize_oracle(request.diagnostic_channel(), policy, &baselines)?;
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
                let mut slot_digests = Vec::with_capacity(frontier.len());

                for (slot, candidate) in frontier.iter().enumerate() {
                    let unit_ids = candidate.iter().map(|unit| unit.id()).collect::<Vec<_>>();
                    let digest = candidate_digest(&unit_ids);
                    slot_digests.push(digest);
                    if let Some(&unique) = unique_by_digest.get(&digest) {
                        slot_to_unique.push(unique);
                        continue;
                    }
                    let Ok(start) = u32::try_from(slot) else {
                        set_error(&first_error, EngineError::InvalidCandidate);
                        return vec![None; frontier.len()];
                    };
                    let unique = unique_plans.len();
                    unique_by_digest.insert(digest, unique);
                    slot_to_unique.push(unique);
                    unique_plans.push(CandidatePlan::new(
                        CandidateRank::new(
                            phase,
                            u32::try_from(frontier.len()).unwrap_or(u32::MAX),
                            FrontierClass::Structured,
                            start,
                            digest,
                        ),
                        CandidatePayload { unit_ids, digest },
                    ));
                }

                let evaluation = FrontierEvaluationContext {
                    request,
                    inventory: &inventory,
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

                if let Some(winner) = earliest_terminal_preserved(&verdicts) {
                    if let Some(writer) = &writer {
                        let attempt = lock(&attempts_by_digest)
                            .get(&slot_digests[winner])
                            .cloned();
                        let Some(attempt) = attempt else {
                            set_error(&first_error, EngineError::InvalidCandidate);
                            return vec![None; frontier.len()];
                        };
                        let transition = TransitionRecord::new(
                            transition_ordinal,
                            from_digest,
                            slot_digests[winner],
                            slot_digests[winner],
                            u64::try_from(frontier[winner].len()).unwrap_or(u64::MAX),
                        );
                        if let Err(error) = writer.accept_transition(attempt, transition) {
                            set_error(&first_error, EngineError::State(error));
                            verdicts.fill(None);
                            return verdicts;
                        }
                    }
                    from_digest = slot_digests[winner];
                    transition_ordinal = transition_ordinal.saturating_add(1);
                }
                verdicts
            });
        if let Some(error) = take_error(&first_error) {
            return Err(error);
        }

        let kept = reduction.kept().iter().collect::<Vec<_>>();
        let mut final_error = None;
        let final_evidence = policy.aggregate(std::iter::from_fn(|| {
            if final_error.is_some() {
                return None;
            }
            Some(match run_candidate(request, &inventory, &kept) {
                Ok(observation) => oracle.classify(&observation),
                Err(error) => {
                    final_error = Some(error);
                    CandidateVerdict::Inconclusive
                }
            })
        }));
        if let Some(error) = final_error {
            return Err(error);
        }
        if final_evidence.decision() != AggregateDecision::Preserved {
            return Err(EngineError::FinalVerificationFailed);
        }

        Ok(ReductionOutcome {
            original_files: inventory.units().len(),
            reduction,
            fingerprint: oracle.fingerprint().clone(),
            baseline_runs: u16::try_from(baselines.len())
                .expect("evaluation policy run count is represented by u16"),
            final_verifications: final_evidence.observed_runs(),
            inconclusive_attempts: inconclusive_attempts.load(Ordering::Relaxed),
            cache_hits: cache_hits.load(Ordering::Relaxed),
            state_path,
            resumed,
        })
    }
}

fn stabilize_oracle(
    channel: DiagnosticChannel,
    policy: EvaluationPolicy,
    baselines: &[ExecutionObservation],
) -> Result<FailureOracle, OracleError> {
    if policy == EvaluationPolicy::strict() {
        return FailureOracle::from_baselines_with_channel(channel, baselines);
    }

    let mut best = None::<(u16, usize, FailureOracle)>;
    for left in 0..baselines.len() {
        for right in (left + 1)..baselines.len() {
            if is_success(&baselines[left]) || is_success(&baselines[right]) {
                continue;
            }
            let pair = [&baselines[left], &baselines[right]];
            let pair = [pair[0].clone(), pair[1].clone()];
            let Ok(candidate) = FailureOracle::from_baselines_with_channel(channel, &pair) else {
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
    inventory: &ProjectInventory,
    kept: &[&ReductionUnit],
) -> Result<reprocut_core::ExecutionObservation, EngineError> {
    let candidate = CandidateWorkspace::materialize(inventory, kept)?;
    let command = CommandSpec::new(
        request.program.clone(),
        request.arguments.clone(),
        candidate.root().to_path_buf(),
        request.timeout,
        request.max_output_bytes,
    );
    Ok(ProcessRunner::run(&command)?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidatePayload {
    unit_ids: Vec<u32>,
    digest: ContentDigest,
}

struct FrontierEvaluationContext<'a> {
    request: &'a ReductionRequest,
    inventory: &'a ProjectInventory,
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
            Some(match run_candidate(self.request, self.inventory, &kept) {
                Ok(observation) => self.oracle.classify(&observation),
                Err(error) => {
                    set_error(&local_error, error);
                    CandidateVerdict::Inconclusive
                }
            })
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

fn session_contract(request: &ReductionRequest, source: ContentDigest) -> SessionContract {
    let mut command = Vec::new();
    encode_field(
        &mut command,
        request.program().as_os_str().to_string_lossy().as_bytes(),
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
    command.push(match request.diagnostic_channel() {
        DiagnosticChannel::Auto => 0,
        DiagnosticChannel::Stderr => 1,
        DiagnosticChannel::Stdout => 2,
        DiagnosticChannel::Combined => 3,
    });
    command.extend_from_slice(&request.evaluation_policy().runs().to_le_bytes());
    command.extend_from_slice(&request.evaluation_policy().required().to_le_bytes());
    SessionContract::new(
        source,
        ContentDigest::of(&command),
        1,
        "files-v1".to_owned(),
        env!("CARGO_PKG_VERSION").to_owned(),
    )
}

fn inventory_digest(inventory: &ProjectInventory) -> Result<ContentDigest, EngineError> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(b"REPROCUT-SOURCE\0");
    for unit in inventory.units() {
        encode_field(&mut encoded, unit.path().as_bytes());
        let path = inventory.root().join(unit.path());
        let bytes = fs::read(&path).map_err(|source| WorkspaceError::Io {
            operation: "hash source file",
            path,
            source,
        })?;
        encode_field(&mut encoded, &bytes);
    }
    Ok(ContentDigest::of(&encoded))
}

fn candidate_digest(ids: &[u32]) -> ContentDigest {
    let mut encoded = Vec::with_capacity(20_usize.saturating_add(ids.len().saturating_mul(4)));
    encoded.extend_from_slice(b"REPROCUT-CANDIDATE\0");
    for id in ids {
        encoded.extend_from_slice(&id.to_le_bytes());
    }
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
