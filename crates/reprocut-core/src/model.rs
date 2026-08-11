use serde::{Deserialize, Serialize};

/// The conservative result of evaluating one reduction candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateVerdict {
    /// The configured failure was observed.
    Preserved,
    /// The configured failure was not observed.
    Rejected,
    /// The execution could not safely be classified.
    Inconclusive,
}

/// Bounded, process-level evidence captured from one command execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionObservation {
    exit_code: Option<i32>,
    signal: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
    streams_truncated: bool,
}

impl ExecutionObservation {
    /// Creates an observation from already-bounded process output.
    pub fn new(
        exit_code: Option<i32>,
        signal: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        timed_out: bool,
        streams_truncated: bool,
    ) -> Self {
        Self {
            exit_code,
            signal,
            stdout,
            stderr,
            timed_out,
            streams_truncated,
        }
    }

    /// Returns the platform exit code when one was reported.
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Returns the terminating signal on platforms that expose one.
    pub const fn signal(&self) -> Option<i32> {
        self.signal
    }

    /// Returns bounded standard output bytes.
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Returns bounded standard error bytes.
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    /// Reports whether the configured execution deadline elapsed.
    pub const fn timed_out(&self) -> bool {
        self.timed_out
    }

    /// Reports whether either captured stream exceeded its byte budget.
    pub const fn streams_truncated(&self) -> bool {
        self.streams_truncated
    }
}

/// A stable, serializable identity for the failure ReproCut must preserve.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FailureFingerprint {
    exit_code: Option<i32>,
    signal: Option<i32>,
    anchor: String,
}

impl FailureFingerprint {
    /// Creates a fingerprint from a stable exit state and textual anchor.
    pub fn new(exit_code: Option<i32>, signal: Option<i32>, anchor: String) -> Self {
        Self {
            exit_code,
            signal,
            anchor,
        }
    }

    /// Returns the expected process exit code.
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Returns the expected terminating signal.
    pub const fn signal(&self) -> Option<i32> {
        self.signal
    }

    /// Returns the stable diagnostic anchor.
    pub fn anchor(&self) -> &str {
        &self.anchor
    }
}
