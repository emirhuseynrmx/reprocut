use thiserror::Error;

use crate::{
    diagnostic::{normalize_bytes, stable_discriminators},
    CandidateVerdict, DiagnosticChannel, ExecutionObservation, FailureFingerprint,
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
    /// The selected stream has no exact failure discriminator shared by every baseline.
    #[error("baseline diagnostics have no stable discriminative line")]
    UnstableDiagnostic,
    /// No failure-bearing diagnostic line remained after normalization.
    #[error("baseline diagnostic has no stable discriminative anchor")]
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
        if baselines
            .iter()
            .skip(1)
            .any(|observation| observation.termination() != first.termination())
        {
            return Err(OracleError::UnstableExitState);
        }

        let anchors = stable_discriminators(channel, baselines);
        if anchors.is_empty() {
            return Err(OracleError::EmptyAnchor);
        }
        Ok(Self {
            fingerprint: FailureFingerprint::from_anchors(first.termination(), anchors),
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
        if observation.termination() != self.fingerprint.termination() {
            return CandidateVerdict::Rejected;
        }

        let stdout = normalize_bytes(observation.stdout());
        let stderr = normalize_bytes(observation.stderr());
        let matches = self.fingerprint.anchors().iter().all(|anchor| {
            let diagnostic = match anchor.channel() {
                DiagnosticChannel::Stdout => &stdout,
                DiagnosticChannel::Stderr => &stderr,
                DiagnosticChannel::Auto | DiagnosticChannel::Combined => return false,
            };
            diagnostic.lines().any(|line| line == anchor.text())
        });
        if matches {
            CandidateVerdict::Preserved
        } else {
            CandidateVerdict::Rejected
        }
    }
}
