use std::sync::OnceLock;

use regex::Regex;
use thiserror::Error;

use crate::{CandidateVerdict, ExecutionObservation, FailureFingerprint};

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
            observation.exit_code() != first.exit_code()
                || observation.signal() != first.signal()
        }) {
            return Err(OracleError::UnstableExitState);
        }

        let first_diagnostic = normalize_bytes(first.stderr());
        if baselines
            .iter()
            .skip(1)
            .any(|observation| normalize_bytes(observation.stderr()) != first_diagnostic)
        {
            return Err(OracleError::UnstableDiagnostic);
        }

        let anchor = first_diagnostic
            .lines()
            .filter(|line| !line.is_empty())
            .max_by_key(|line| line.len())
            .ok_or(OracleError::EmptyAnchor)?
            .to_owned();

        Ok(Self {
            fingerprint: FailureFingerprint::new(first.exit_code(), first.signal(), anchor),
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

        let normalized = normalize_bytes(observation.stderr());
        if normalized
            .lines()
            .any(|line| line == self.fingerprint.anchor())
        {
            CandidateVerdict::Preserved
        } else {
            CandidateVerdict::Rejected
        }
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
    let hex_address = HEX_ADDRESS.get_or_init(|| {
        Regex::new(r"0[xX][0-9a-fA-F]+").expect("hex regex is static and valid")
    });
    let decimal_id = DECIMAL_ID.get_or_init(|| {
        Regex::new(r"[0-9]+").expect("decimal regex is static and valid")
    });
    let horizontal_space = HORIZONTAL_SPACE.get_or_init(|| {
        Regex::new(r"[\t ]+").expect("whitespace regex is static and valid")
    });

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
