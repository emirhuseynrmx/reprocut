//! What a reduction learns before its first cut, kept instead of discarded.
//!
//! The engine has always run these checks. It just threw the observations away when one
//! of them failed, which is exactly when a reader needs them most.

use std::path::{Path, PathBuf};

use reprocut_core::{
    DiagnosticChannel, ExecutionObservation, FailureOracle, OracleError, OracleMode,
};

/// The result of every check that runs before the first candidate is cut.
#[derive(Debug)]
pub struct Preflight {
    requested_program: PathBuf,
    executed_program: Option<PathBuf>,
    mode: OracleMode,
    channel: DiagnosticChannel,
    runs_planned: u16,
    observations: Vec<ExecutionObservation>,
    verdict: PreflightVerdict,
}

/// Whether reduction may begin, and what stopped it when it may not.
#[derive(Debug)]
pub enum PreflightVerdict {
    /// The failure is stable and an oracle recognizes it.
    Ready(Box<FailureOracle>),
    /// A preparation command did not succeed, so no baseline was observed.
    PreparationRejected,
    /// The command completed successfully where the oracle mode requires a failure.
    CommandSucceeded,
    /// The observations were taken but no oracle could be stabilized from them.
    NoStableOracle(OracleError),
}

impl Preflight {
    pub(crate) const fn new(
        requested_program: PathBuf,
        executed_program: Option<PathBuf>,
        mode: OracleMode,
        channel: DiagnosticChannel,
        runs_planned: u16,
        observations: Vec<ExecutionObservation>,
        verdict: PreflightVerdict,
    ) -> Self {
        Self {
            requested_program,
            executed_program,
            mode,
            channel,
            runs_planned,
            observations,
            verdict,
        }
    }

    /// Returns the program as the caller wrote it.
    pub fn requested_program(&self) -> &Path {
        &self.requested_program
    }

    /// Returns the path the runner was given, which isolated preparation may rewrite.
    ///
    /// This is not a `PATH` lookup performed here. It is the program of the command the
    /// engine actually built, so a report that quotes it describes the run that happened
    /// rather than a second resolution that might disagree with it.
    pub fn executed_program(&self) -> Option<&Path> {
        self.executed_program.as_deref()
    }

    /// Returns the failure-recognition mode the request selected.
    pub const fn mode(&self) -> OracleMode {
        self.mode
    }

    /// Returns the stream policy the request selected.
    pub const fn channel(&self) -> DiagnosticChannel {
        self.channel
    }

    /// Returns how many observations the evaluation policy asked for.
    pub const fn runs_planned(&self) -> u16 {
        self.runs_planned
    }

    /// Returns the observations that were taken, which may be fewer than planned.
    ///
    /// The loop stops at the first run that settles the question, so a request rejected
    /// on its first run reports one observation rather than a padded three.
    pub fn observations(&self) -> &[ExecutionObservation] {
        &self.observations
    }

    /// Returns the verdict.
    pub const fn verdict(&self) -> &PreflightVerdict {
        &self.verdict
    }

    /// Returns whether reduction may begin.
    pub const fn is_ready(&self) -> bool {
        matches!(self.verdict, PreflightVerdict::Ready(_))
    }

    pub(crate) fn into_parts(self) -> (Vec<ExecutionObservation>, PreflightVerdict) {
        (self.observations, self.verdict)
    }
}
