use serde::{Deserialize, Serialize};

use crate::transformation::ContentDigest;

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

/// A platform-neutral account of how a candidate command ended.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum TerminationReason {
    /// The process returned a conventional numeric status.
    ExitCode(i32),
    /// A Unix host reported a terminating signal.
    UnixSignal(i32),
    /// ReproCut terminated the process group after its deadline.
    TimedOut,
    /// The runner could not obtain a trustworthy process result.
    RunnerFailure,
}

impl TerminationReason {
    const fn from_legacy(exit_code: Option<i32>, signal: Option<i32>, timed_out: bool) -> Self {
        if timed_out {
            Self::TimedOut
        } else if let Some(signal) = signal {
            Self::UnixSignal(signal)
        } else if let Some(exit_code) = exit_code {
            Self::ExitCode(exit_code)
        } else {
            Self::RunnerFailure
        }
    }
}

/// The operating-system primitive responsible for descendant teardown.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentMechanism {
    /// Compatibility observations that do not claim process-tree ownership.
    DirectChild,
    /// A POSIX process group owned by the runner.
    PosixProcessGroup,
    /// A Windows Job Object owned by the runner.
    WindowsJobObject,
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
    termination: TerminationReason,
    containment: ContainmentMechanism,
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
        let termination = TerminationReason::from_legacy(exit_code, signal, timed_out);
        Self {
            exit_code,
            signal,
            stdout,
            stderr,
            timed_out,
            streams_truncated,
            termination,
            containment: ContainmentMechanism::DirectChild,
        }
    }

    /// Creates an observation that records portable termination and containment evidence.
    pub fn new_contained(
        termination: TerminationReason,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        streams_truncated: bool,
        containment: ContainmentMechanism,
    ) -> Self {
        let (exit_code, signal, timed_out) = match termination {
            TerminationReason::ExitCode(code) => (Some(code), None, false),
            TerminationReason::UnixSignal(signal) => (None, Some(signal), false),
            TerminationReason::TimedOut => (None, None, true),
            TerminationReason::RunnerFailure => (None, None, false),
        };
        Self {
            exit_code,
            signal,
            stdout,
            stderr,
            timed_out,
            streams_truncated,
            termination,
            containment,
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

    /// Returns the portable process termination reason.
    pub const fn termination(&self) -> TerminationReason {
        self.termination
    }

    /// Returns the process-tree ownership primitive used for this run.
    pub const fn containment(&self) -> ContainmentMechanism {
        self.containment
    }
}

/// Selects the exact evidence contract used to recognize interesting candidates.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleMode {
    /// Derive exact stable diagnostic discriminators from repeated baselines.
    Automatic,
    /// Require caller-owned regular expressions and reject veto expressions.
    Regex,
    /// Treat command exit code zero as interesting without inspecting output.
    ExitZero,
}

/// A stable, serializable identity for the failure ReproCut must preserve.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FailureFingerprint {
    mode: OracleMode,
    exit_code: Option<i32>,
    signal: Option<i32>,
    termination: TerminationReason,
    anchor: String,
    anchors: Vec<DiagnosticAnchor>,
    failure_patterns: Vec<String>,
    reject_patterns: Vec<String>,
    normalization_schema: u16,
    oracle_spec_digest: ContentDigest,
}

impl FailureFingerprint {
    /// Creates a fingerprint from a stable exit state and textual anchor.
    pub fn new(exit_code: Option<i32>, signal: Option<i32>, anchor: String) -> Self {
        let anchors = vec![DiagnosticAnchor::new(
            DiagnosticChannel::Stderr,
            anchor.clone(),
        )];
        Self {
            mode: OracleMode::Automatic,
            exit_code,
            signal,
            termination: TerminationReason::from_legacy(exit_code, signal, false),
            anchor,
            anchors,
            failure_patterns: Vec::new(),
            reject_patterns: Vec::new(),
            normalization_schema: crate::NORMALIZATION_SCHEMA,
            oracle_spec_digest: ContentDigest::of(b"REPROCUT-LEGACY-AUTOMATIC-SPEC\0"),
        }
    }

    /// Creates a mode-aware fingerprint from an already validated oracle contract.
    pub(crate) fn from_oracle(
        mode: OracleMode,
        termination: Option<TerminationReason>,
        anchors: Vec<DiagnosticAnchor>,
        failure_patterns: Vec<String>,
        reject_patterns: Vec<String>,
        oracle_spec_digest: ContentDigest,
    ) -> Self {
        let anchor = anchors
            .first()
            .map(|item| item.text.clone())
            .unwrap_or_default();
        Self {
            mode,
            exit_code: match termination {
                Some(TerminationReason::ExitCode(code)) => Some(code),
                _ => None,
            },
            signal: match termination {
                Some(TerminationReason::UnixSignal(signal)) => Some(signal),
                _ => None,
            },
            termination: termination.unwrap_or(TerminationReason::RunnerFailure),
            anchor,
            anchors,
            failure_patterns,
            reject_patterns,
            normalization_schema: crate::NORMALIZATION_SCHEMA,
            oracle_spec_digest,
        }
    }

    /// Returns the configured interestingness mode.
    pub const fn mode(&self) -> OracleMode {
        self.mode
    }

    /// Returns the expected process exit code.
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Returns the expected terminating signal.
    pub const fn signal(&self) -> Option<i32> {
        self.signal
    }

    /// Returns the expected portable process termination reason.
    pub const fn termination(&self) -> TerminationReason {
        self.termination
    }

    /// Returns the stable diagnostic anchor.
    pub fn anchor(&self) -> &str {
        &self.anchor
    }

    /// Returns every stream-qualified anchor required to recognize the failure.
    pub fn anchors(&self) -> &[DiagnosticAnchor] {
        &self.anchors
    }

    /// Returns caller-owned regexes that must all match.
    pub fn failure_patterns(&self) -> &[String] {
        &self.failure_patterns
    }

    /// Returns caller-owned regexes that veto a match.
    pub fn reject_patterns(&self) -> &[String] {
        &self.reject_patterns
    }

    /// Returns the version of the deterministic normalization contract.
    pub const fn normalization_schema(&self) -> u16 {
        self.normalization_schema
    }

    /// Returns the digest of the validated oracle configuration.
    pub const fn oracle_spec_digest(&self) -> ContentDigest {
        self.oracle_spec_digest
    }

    /// Returns a canonical SHA-256 identity for every failure evidence field.
    pub fn digest(&self) -> ContentDigest {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"REPROCUT-FINGERPRINT-V2\0");
        bytes.push(match self.mode {
            OracleMode::Automatic => 0,
            OracleMode::Regex => 1,
            OracleMode::ExitZero => 2,
        });
        encode_termination(&mut bytes, self.termination);
        bytes.extend_from_slice(&self.normalization_schema.to_le_bytes());
        bytes.extend_from_slice(self.oracle_spec_digest.as_bytes());
        encode_len(&mut bytes, self.anchors.len());
        for anchor in &self.anchors {
            bytes.push(match anchor.channel {
                DiagnosticChannel::Auto => 0,
                DiagnosticChannel::Stderr => 1,
                DiagnosticChannel::Stdout => 2,
                DiagnosticChannel::Combined => 3,
            });
            encode_text(&mut bytes, &anchor.text);
        }
        encode_strings(&mut bytes, &self.failure_patterns);
        encode_strings(&mut bytes, &self.reject_patterns);
        ContentDigest::of(&bytes)
    }
}

fn encode_termination(bytes: &mut Vec<u8>, termination: TerminationReason) {
    match termination {
        TerminationReason::ExitCode(code) => {
            bytes.push(0);
            bytes.extend_from_slice(&code.to_le_bytes());
        }
        TerminationReason::UnixSignal(signal) => {
            bytes.push(1);
            bytes.extend_from_slice(&signal.to_le_bytes());
        }
        TerminationReason::TimedOut => bytes.push(2),
        TerminationReason::RunnerFailure => bytes.push(3),
    }
}

fn encode_strings(bytes: &mut Vec<u8>, values: &[String]) {
    encode_len(bytes, values.len());
    for value in values {
        encode_text(bytes, value);
    }
}

fn encode_text(bytes: &mut Vec<u8>, value: &str) {
    encode_len(bytes, value.len());
    bytes.extend_from_slice(value.as_bytes());
}

fn encode_len(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(&u64::try_from(value).unwrap_or(u64::MAX).to_le_bytes());
}
