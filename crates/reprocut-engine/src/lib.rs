//! End-to-end reduction orchestration for ReproCut.

use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use reprocut_core::{
    reduce, CandidateVerdict, FailureFingerprint, FailureOracle, OracleError, ReductionResult,
    ReductionUnit,
};
use reprocut_runner::{CommandSpec, ProcessRunner, RunnerError};
use reprocut_workspace::{CandidateWorkspace, ProjectInventory, WorkspaceError};
use thiserror::Error;

const STABILITY_RUNS: u8 = 3;

/// A complete reduction request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionRequest {
    source_root: PathBuf,
    program: PathBuf,
    arguments: Vec<OsString>,
    timeout: Duration,
    max_output_bytes: usize,
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
        }
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
}

/// A completed, repeatedly verified reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionOutcome {
    original_files: usize,
    reduction: ReductionResult,
    fingerprint: FailureFingerprint,
    baseline_runs: u8,
    final_verifications: u8,
    inconclusive_attempts: u64,
    cache_hits: u64,
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
    pub const fn baseline_runs(&self) -> u8 {
        self.baseline_runs
    }

    /// Returns the number of final verification executions.
    pub const fn final_verifications(&self) -> u8 {
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
}

/// Stateless entry point for deterministic project reduction.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReductionEngine;

impl ReductionEngine {
    /// Stabilizes, minimizes, and re-verifies one failing command.
    pub fn run(request: &ReductionRequest) -> Result<ReductionOutcome, EngineError> {
        let inventory = ProjectInventory::scan(request.source_root())?;
        if inventory.units().is_empty() {
            return Err(EngineError::EmptyProject);
        }
        let all_units = inventory.units().iter().collect::<Vec<_>>();
        let mut baselines = Vec::with_capacity(usize::from(STABILITY_RUNS));

        for _ in 0..STABILITY_RUNS {
            let observation = run_candidate(request, &inventory, &all_units)?;
            if observation.exit_code() == Some(0) && observation.signal().is_none() {
                return Err(EngineError::BaselineSucceeded);
            }
            baselines.push(observation);
        }
        let oracle = FailureOracle::from_baselines(&baselines)?;
        let mut cache = HashMap::<Vec<u32>, CandidateVerdict>::new();
        let mut first_error = None;
        let mut inconclusive_attempts = 0_u64;
        let mut cache_hits = 0_u64;

        let reduction = reduce(inventory.units(), |kept| {
            if first_error.is_some() {
                inconclusive_attempts = inconclusive_attempts.saturating_add(1);
                return CandidateVerdict::Inconclusive;
            }
            let key = candidate_key(kept);
            if let Some(&verdict) = cache.get(&key) {
                cache_hits = cache_hits.saturating_add(1);
                return verdict;
            }

            let verdict = match run_candidate(request, &inventory, kept) {
                Ok(observation) => oracle.classify(&observation),
                Err(error) => {
                    first_error = Some(error);
                    CandidateVerdict::Inconclusive
                }
            };
            if verdict == CandidateVerdict::Inconclusive {
                inconclusive_attempts = inconclusive_attempts.saturating_add(1);
            }
            cache.insert(key, verdict);
            verdict
        });
        if let Some(error) = first_error {
            return Err(error);
        }

        let kept = reduction.kept().iter().collect::<Vec<_>>();
        for _ in 0..STABILITY_RUNS {
            let observation = run_candidate(request, &inventory, &kept)?;
            if oracle.classify(&observation) != CandidateVerdict::Preserved {
                return Err(EngineError::FinalVerificationFailed);
            }
        }

        Ok(ReductionOutcome {
            original_files: inventory.units().len(),
            reduction,
            fingerprint: oracle.fingerprint().clone(),
            baseline_runs: STABILITY_RUNS,
            final_verifications: STABILITY_RUNS,
            inconclusive_attempts,
            cache_hits,
        })
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

fn candidate_key(kept: &[&ReductionUnit]) -> Vec<u32> {
    let mut key = Vec::with_capacity(kept.len());
    key.extend(kept.iter().map(|unit| unit.id()));
    key
}
