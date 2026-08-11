use std::sync::OnceLock;

use regex::Regex;
use thiserror::Error;

use crate::{
    CandidateVerdict, DiagnosticAnchor, DiagnosticChannel, ExecutionObservation, FailureFingerprint,
};

/// A failure-oracle construction error.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OracleError {
    /// At least two independent observations are required.
    #[error("at least two baseline observations are required")]
    TooFewBaselines,
    /// A baseline timed out or exceeded its capture budget.
    #[error("a baseline observation is incomplete")]
    IncompleteBaseline,
    /// Baseline processes did not terminate in the same way.
    #[error("baseline exit states are unstable")]
    UnstableExitState,
    /// Baseline diagnostics differ after volatile data normalization.
    #[error("baseline diagnostics are unstable")]
    UnstableDiagnostic,
    /// No stable diagnostic line remained after normalization.
    #[error("baseline diagnostic has no stable non-empty anchor")]
    EmptyAnchor,
}

/// Conservatively recognizes one exact, stabilized failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureOracle {
    fingerprint: FailureFingerprint,
}

impl FailureOracle {
    /// Builds an oracle from repeated observations of the original failure.
    pub fn from_baselines(baselines: &[ExecutionObservation]) -> Result<Self, OracleError> {
        Self::from_baselines_with_channel(DiagnosticChannel::Auto, baselines)
    }

    /// Builds an oracle under an explicit process-stream selection policy.
    pub fn from_baselines_with_channel(
        channel: DiagnosticChannel,
        baselines: &[ExecutionObservation],
    ) -> Result<Self, OracleError> {
        let Some(first) = baselines.first() else {
            return Err(OracleError::TooFewBaselines);
        };
        if baselines.len() < 2 {
            return Err(OracleError::TooFewBaselines);
        }
        if baselines
            .iter()
            .any(|observation| observation.timed_out() || observation.streams_truncated())
        {
            return Err(OracleError::IncompleteBaseline);
        }
        if baselines.iter().skip(1).any(|observation| {
            observation.exit_code() != first.exit_code() || observation.signal() != first.signal()
        }) {
            return Err(OracleError::UnstableExitState);
        }

        let stdout = stable_stream(baselines, ExecutionObservation::stdout);
        let stderr = stable_stream(baselines, ExecutionObservation::stderr);
        let mut anchors = Vec::with_capacity(2);

        match channel {
            DiagnosticChannel::Auto => {
                if let StableStream::Stable(value) = &stdout {
                    anchors.push(anchor_for(DiagnosticChannel::Stdout, value)?);
                }
                if let StableStream::Stable(value) = &stderr {
                    anchors.push(anchor_for(DiagnosticChannel::Stderr, value)?);
                }
                if anchors.is_empty() {
                    return Err(auto_stream_error(&stdout, &stderr));
                }
            }
            DiagnosticChannel::Stdout => {
                anchors.push(required_anchor(DiagnosticChannel::Stdout, stdout)?);
            }
            DiagnosticChannel::Stderr => {
                anchors.push(required_anchor(DiagnosticChannel::Stderr, stderr)?);
            }
            DiagnosticChannel::Combined => {
                anchors.push(required_anchor(DiagnosticChannel::Stdout, stdout)?);
                anchors.push(required_anchor(DiagnosticChannel::Stderr, stderr)?);
            }
        }

        Ok(Self {
            fingerprint: FailureFingerprint::from_anchors(
                first.exit_code(),
                first.signal(),
                anchors,
            ),
        })
    }

    /// Returns the stabilized failure identity.
    pub const fn fingerprint(&self) -> &FailureFingerprint {
        &self.fingerprint
    }

    /// Classifies a candidate without accepting incomplete evidence.
    pub fn classify(&self, observation: &ExecutionObservation) -> CandidateVerdict {
        if observation.timed_out() || observation.streams_truncated() {
            return CandidateVerdict::Inconclusive;
        }
        if observation.exit_code() != self.fingerprint.exit_code()
            || observation.signal() != self.fingerprint.signal()
        {
            return CandidateVerdict::Rejected;
        }

        let mut normalized_stdout = None;
        let mut normalized_stderr = None;
        let matches = self.fingerprint.anchors().iter().all(|anchor| {
            let normalized =
                match anchor.channel() {
                    DiagnosticChannel::Stdout => normalized_stdout
                        .get_or_insert_with(|| normalize_bytes(observation.stdout())),
                    DiagnosticChannel::Stderr => normalized_stderr
                        .get_or_insert_with(|| normalize_bytes(observation.stderr())),
                    DiagnosticChannel::Auto | DiagnosticChannel::Combined => return false,
                };
            normalized.lines().any(|line| line == anchor.text())
        });
        if matches {
            CandidateVerdict::Preserved
        } else {
            CandidateVerdict::Rejected
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StableStream {
    Stable(String),
    Empty,
    Unstable,
}

fn stable_stream(
    baselines: &[ExecutionObservation],
    select: fn(&ExecutionObservation) -> &[u8],
) -> StableStream {
    let first = normalize_bytes(select(&baselines[0]));
    if baselines
        .iter()
        .skip(1)
        .any(|observation| normalize_bytes(select(observation)) != first)
    {
        StableStream::Unstable
    } else if first.is_empty() {
        StableStream::Empty
    } else {
        StableStream::Stable(first)
    }
}

fn required_anchor(
    channel: DiagnosticChannel,
    stream: StableStream,
) -> Result<DiagnosticAnchor, OracleError> {
    match stream {
        StableStream::Stable(value) => anchor_for(channel, &value),
        StableStream::Empty => Err(OracleError::EmptyAnchor),
        StableStream::Unstable => Err(OracleError::UnstableDiagnostic),
    }
}

fn anchor_for(
    channel: DiagnosticChannel,
    diagnostic: &str,
) -> Result<DiagnosticAnchor, OracleError> {
    diagnostic
        .lines()
        .filter(|line| !line.is_empty())
        .max_by_key(|line| line.len())
        .map(|line| DiagnosticAnchor::new(channel, line.to_owned()))
        .ok_or(OracleError::EmptyAnchor)
}

fn auto_stream_error(stdout: &StableStream, stderr: &StableStream) -> OracleError {
    if matches!(stdout, StableStream::Unstable) || matches!(stderr, StableStream::Unstable) {
        OracleError::UnstableDiagnostic
    } else {
        OracleError::EmptyAnchor
    }
}

/// Removes volatile process-specific fragments from diagnostic text.
pub fn normalize_diagnostic(input: &str) -> String {
    static WINDOWS_PATH: OnceLock<Regex> = OnceLock::new();
    static UNIX_PATH: OnceLock<Regex> = OnceLock::new();
    static HEX_ADDRESS: OnceLock<Regex> = OnceLock::new();
    static DECIMAL_ID: OnceLock<Regex> = OnceLock::new();
    static HORIZONTAL_SPACE: OnceLock<Regex> = OnceLock::new();

    let windows_path = WINDOWS_PATH.get_or_init(|| {
        Regex::new(r"[A-Za-z]:\\(?:[^\\ \t\r\n:]+\\)*[^\\ \t\r\n:]+")
            .expect("Windows path regex is static and valid")
    });
    let unix_path = UNIX_PATH.get_or_init(|| {
        Regex::new(r"/(?:[^/ \t\r\n:]+/)*[^/ \t\r\n:]+")
            .expect("Unix path regex is static and valid")
    });
    let hex_address = HEX_ADDRESS
        .get_or_init(|| Regex::new(r"0[xX][0-9a-fA-F]+").expect("hex regex is static and valid"));
    let decimal_id = DECIMAL_ID
        .get_or_init(|| Regex::new(r"[0-9]+").expect("decimal regex is static and valid"));
    let horizontal_space = HORIZONTAL_SPACE
        .get_or_init(|| Regex::new(r"[\t ]+").expect("whitespace regex is static and valid"));

    let normalized_newlines = input.replace("\r\n", "\n").replace('\r', "\n");
    let without_windows_paths = windows_path.replace_all(&normalized_newlines, "<path>");
    let without_paths = unix_path.replace_all(&without_windows_paths, "<path>");
    let without_addresses = hex_address.replace_all(&without_paths, "<hex>");
    let without_ids = decimal_id.replace_all(&without_addresses, "<n>");

    without_ids
        .lines()
        .map(|line| horizontal_space.replace_all(line.trim(), " "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_bytes(bytes: &[u8]) -> String {
    normalize_diagnostic(&String::from_utf8_lossy(bytes))
}
