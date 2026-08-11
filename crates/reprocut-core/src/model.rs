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

/// Selects which bounded process stream contributes failure evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticChannel {
    /// Use every stable, non-empty stream and require all selected anchors later.
    Auto,
    /// Use only standard error.
    Stderr,
    /// Use only standard output.
    Stdout,
    /// Require stable, non-empty evidence from both output streams.
    Combined,
}

/// One normalized line tied to the stream that produced it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticAnchor {
    channel: DiagnosticChannel,
    text: String,
}

impl DiagnosticAnchor {
    /// Creates a stream-qualified diagnostic anchor.
    pub fn new(channel: DiagnosticChannel, text: String) -> Self {
        Self { channel, text }
    }

    /// Returns the stream that must contain this anchor.
    pub const fn channel(&self) -> DiagnosticChannel {
        self.channel
    }

    /// Returns the normalized anchor line.
    pub fn text(&self) -> &str {
        &self.text
    }
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
    anchors: Vec<DiagnosticAnchor>,
    normalization_schema: u16,
}

impl FailureFingerprint {
    /// Creates a fingerprint from a stable exit state and textual anchor.
    pub fn new(exit_code: Option<i32>, signal: Option<i32>, anchor: String) -> Self {
        let anchors = vec![DiagnosticAnchor::new(
            DiagnosticChannel::Stderr,
            anchor.clone(),
        )];
        Self {
            exit_code,
            signal,
            anchor,
            anchors,
            normalization_schema: 1,
        }
    }

    /// Creates a fingerprint from stream-qualified anchors.
    pub(crate) fn from_anchors(
        exit_code: Option<i32>,
        signal: Option<i32>,
        anchors: Vec<DiagnosticAnchor>,
    ) -> Self {
        debug_assert!(!anchors.is_empty());
        let anchor = anchors
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();
        Self {
            exit_code,
            signal,
            anchor,
            anchors,
            normalization_schema: 1,
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

    /// Returns every stream-qualified anchor required to recognize the failure.
    pub fn anchors(&self) -> &[DiagnosticAnchor] {
        &self.anchors
    }

    /// Returns the version of the deterministic normalization contract.
    pub const fn normalization_schema(&self) -> u16 {
        self.normalization_schema
    }
}
