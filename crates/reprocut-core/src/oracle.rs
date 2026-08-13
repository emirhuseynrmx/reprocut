use regex::Regex;
use thiserror::Error;

use crate::{
    diagnostic::{normalize_bytes, stable_discriminators},
    CandidateVerdict, ContentDigest, DiagnosticChannel, ExecutionObservation, FailureFingerprint,
    OracleMode, TerminationReason,
};

const MAX_PATTERNS: usize = 16;
const MAX_PATTERN_BYTES: usize = 4096;
const COMBINED_DELIMITER: &str = "\n--- REPROCUT STREAM ---\n";

/// A failure-oracle construction or configuration error.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OracleError {
    /// At least two independent observations are required.
    #[error("at least two baseline observations are required")]
    TooFewBaselines,
    /// A baseline timed out or exceeded its relevant capture budget.
    #[error("a baseline observation is incomplete")]
    IncompleteBaseline,
    /// Baseline processes did not terminate in the same way.
    #[error("baseline exit states are unstable")]
    UnstableExitState,
    /// Baseline diagnostics have no exact discriminator in common.
    #[error("baseline diagnostics are unstable")]
    UnstableDiagnostic,
    /// No failure-bearing diagnostic line remained after normalization.
    #[error("baseline diagnostic has no stable discriminative anchor")]
    EmptyAnchor,
    /// Oracle fields were combined in a mode that does not permit them.
    #[error("invalid oracle mode configuration")]
    InvalidConfiguration,
    /// A caller-owned regular expression did not compile.
    #[error("invalid oracle regular expression")]
    InvalidPattern,
    /// A caller-owned regular expression exceeded its byte budget.
    #[error("oracle regular expression exceeds 4096 UTF-8 bytes")]
    PatternTooLong,
    /// A required or reject list exceeded its independent count budget.
    #[error("oracle accepts at most 16 required and 16 reject expressions")]
    TooManyPatterns,
    /// A required expression did not identify every baseline.
    #[error("a required expression does not match every baseline")]
    BaselinePatternMismatch,
    /// A reject expression matched an original baseline.
    #[error("a reject expression matches an original baseline")]
    BaselineUnexpectedReject,
    /// Exit-zero mode requires every original baseline to return zero.
    #[error("exit-zero mode requires every baseline to exit with code zero")]
    ExitZeroBaselineRequired,
}

/// Canonical, validated failure-recognition configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleSpec {
    mode: OracleMode,
    channel: DiagnosticChannel,
    failure_patterns: Vec<String>,
    reject_patterns: Vec<String>,
    digest: ContentDigest,
}

impl OracleSpec {
    /// Validates and canonicalizes one complete oracle contract.
    ///
    /// # Errors
    ///
    /// Returns an error when pattern limits, mode-specific fields, or regular expressions are
    /// invalid.
    pub fn new(
        mode: OracleMode,
        channel: DiagnosticChannel,
        mut failure_patterns: Vec<String>,
        mut reject_patterns: Vec<String>,
    ) -> Result<Self, OracleError> {
        if failure_patterns.len() > MAX_PATTERNS || reject_patterns.len() > MAX_PATTERNS {
            return Err(OracleError::TooManyPatterns);
        }
        if failure_patterns
            .iter()
            .chain(&reject_patterns)
            .any(|pattern| pattern.len() > MAX_PATTERN_BYTES)
        {
            return Err(OracleError::PatternTooLong);
        }
        match mode {
            OracleMode::Automatic if !failure_patterns.is_empty() => {
                return Err(OracleError::InvalidConfiguration);
            }
            OracleMode::Regex if failure_patterns.is_empty() => {
                return Err(OracleError::InvalidConfiguration);
            }
            OracleMode::ExitZero if !failure_patterns.is_empty() || !reject_patterns.is_empty() => {
                return Err(OracleError::InvalidConfiguration);
            }
            OracleMode::Automatic | OracleMode::Regex | OracleMode::ExitZero => {}
        }
        if failure_patterns
            .iter()
            .chain(&reject_patterns)
            .any(|pattern| Regex::new(pattern).is_err())
        {
            return Err(OracleError::InvalidPattern);
        }
        failure_patterns.sort_unstable();
        failure_patterns.dedup();
        reject_patterns.sort_unstable();
        reject_patterns.dedup();
        let digest = spec_digest(mode, channel, &failure_patterns, &reject_patterns);
        Ok(Self {
            mode,
            channel,
            failure_patterns,
            reject_patterns,
            digest,
        })
    }

    /// Returns automatic schema-5 inference for one stream policy.
    ///
    /// # Panics
    ///
    /// Panics only if the crate's built-in empty automatic configuration becomes invalid.
    pub fn automatic(channel: DiagnosticChannel) -> Self {
        Self::new(OracleMode::Automatic, channel, Vec::new(), Vec::new())
            .expect("the built-in automatic oracle spec is valid")
    }

    /// Returns the configured oracle mode.
    pub const fn mode(&self) -> OracleMode {
        self.mode
    }

    /// Returns the selected process stream.
    pub const fn channel(&self) -> DiagnosticChannel {
        self.channel
    }

    /// Returns required patterns in canonical lexical order.
    pub fn failure_patterns(&self) -> &[String] {
        &self.failure_patterns
    }

    /// Returns reject patterns in canonical lexical order.
    pub fn reject_patterns(&self) -> &[String] {
        &self.reject_patterns
    }

    /// Returns the canonical contract identity.
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Conservatively recognizes one exact, stabilized failure.
#[derive(Clone, Debug)]
pub struct FailureOracle {
    spec: OracleSpec,
    fingerprint: FailureFingerprint,
    required: Vec<Regex>,
    reject: Vec<Regex>,
}

impl FailureOracle {
    /// Builds an automatic oracle from repeated observations of the original failure.
    ///
    /// # Errors
    ///
    /// Returns an error when the baselines are incomplete, unstable, or lack a discriminator.
    pub fn from_baselines(baselines: &[ExecutionObservation]) -> Result<Self, OracleError> {
        Self::from_baselines_with_channel(DiagnosticChannel::Auto, baselines)
    }

    /// Builds an automatic oracle under an explicit stream policy.
    ///
    /// # Errors
    ///
    /// Returns an error when the baselines are incomplete, unstable, or lack a discriminator.
    pub fn from_baselines_with_channel(
        channel: DiagnosticChannel,
        baselines: &[ExecutionObservation],
    ) -> Result<Self, OracleError> {
        Self::from_spec_and_baselines(OracleSpec::automatic(channel), baselines)
    }

    /// Builds an oracle from a fully validated contract and repeated baselines.
    ///
    /// # Errors
    ///
    /// Returns an error when baselines violate the selected mode, termination, pattern, or
    /// diagnostic-stability contract.
    pub fn from_spec_and_baselines(
        spec: OracleSpec,
        baselines: &[ExecutionObservation],
    ) -> Result<Self, OracleError> {
        validate_baseline_count(baselines)?;
        let required = compile_patterns(spec.failure_patterns());
        let reject = compile_patterns(spec.reject_patterns());
        let termination = match spec.mode() {
            OracleMode::ExitZero => {
                if baselines
                    .iter()
                    .any(|observation| observation.termination() != TerminationReason::ExitCode(0))
                {
                    return Err(OracleError::ExitZeroBaselineRequired);
                }
                Some(TerminationReason::ExitCode(0))
            }
            OracleMode::Automatic | OracleMode::Regex => {
                validate_complete_text_baselines(baselines)?;
                let first = baselines[0].termination();
                if baselines
                    .iter()
                    .skip(1)
                    .any(|observation| observation.termination() != first)
                {
                    return Err(OracleError::UnstableExitState);
                }
                Some(first)
            }
        };
        let anchors = match spec.mode() {
            OracleMode::Automatic => {
                for baseline in baselines {
                    let raw = diagnostic_view(spec.channel(), baseline);
                    if reject.iter().any(|pattern| pattern.is_match(&raw)) {
                        return Err(OracleError::BaselineUnexpectedReject);
                    }
                }
                let anchors = stable_discriminators(spec.channel(), baselines);
                if anchors.is_empty() {
                    return Err(OracleError::EmptyAnchor);
                }
                anchors
            }
            OracleMode::Regex => {
                for baseline in baselines {
                    let raw = diagnostic_view(spec.channel(), baseline);
                    if reject.iter().any(|pattern| pattern.is_match(&raw)) {
                        return Err(OracleError::BaselineUnexpectedReject);
                    }
                    if !required.iter().all(|pattern| pattern.is_match(&raw)) {
                        return Err(OracleError::BaselinePatternMismatch);
                    }
                }
                Vec::new()
            }
            OracleMode::ExitZero => Vec::new(),
        };
        let fingerprint = FailureFingerprint::from_oracle(
            spec.mode(),
            termination,
            anchors,
            spec.failure_patterns().to_vec(),
            spec.reject_patterns().to_vec(),
            spec.digest(),
        );
        Ok(Self {
            spec,
            fingerprint,
            required,
            reject,
        })
    }

    /// Returns the stabilized failure identity.
    pub const fn fingerprint(&self) -> &FailureFingerprint {
        &self.fingerprint
    }

    /// Classifies a candidate without accepting incomplete evidence.
    pub fn classify(&self, observation: &ExecutionObservation) -> CandidateVerdict {
        if matches!(
            observation.termination(),
            TerminationReason::TimedOut | TerminationReason::RunnerFailure
        ) {
            return CandidateVerdict::Inconclusive;
        }
        match self.spec.mode() {
            OracleMode::ExitZero => match observation.termination() {
                TerminationReason::ExitCode(0) => CandidateVerdict::Preserved,
                TerminationReason::ExitCode(_) => CandidateVerdict::Rejected,
                TerminationReason::UnixSignal(_)
                | TerminationReason::TimedOut
                | TerminationReason::RunnerFailure => CandidateVerdict::Inconclusive,
            },
            OracleMode::Automatic | OracleMode::Regex => self.classify_text(observation),
        }
    }

    fn classify_text(&self, observation: &ExecutionObservation) -> CandidateVerdict {
        if observation.streams_truncated() {
            return CandidateVerdict::Inconclusive;
        }
        if observation.termination() != self.fingerprint.termination() {
            return CandidateVerdict::Rejected;
        }
        let raw = diagnostic_view(self.spec.channel(), observation);
        if self.reject.iter().any(|pattern| pattern.is_match(&raw)) {
            return CandidateVerdict::Rejected;
        }
        match self.spec.mode() {
            OracleMode::Regex => {
                if self.required.iter().all(|pattern| pattern.is_match(&raw)) {
                    CandidateVerdict::Preserved
                } else {
                    CandidateVerdict::Rejected
                }
            }
            OracleMode::Automatic => {
                let stdout = normalize_bytes(observation.stdout());
                let stderr = normalize_bytes(observation.stderr());
                if self.fingerprint.anchors().iter().all(|anchor| {
                    let diagnostic = match anchor.channel() {
                        DiagnosticChannel::Stdout => &stdout,
                        DiagnosticChannel::Stderr => &stderr,
                        DiagnosticChannel::Auto | DiagnosticChannel::Combined => return false,
                    };
                    diagnostic.lines().any(|line| line == anchor.text())
                }) {
                    CandidateVerdict::Preserved
                } else {
                    CandidateVerdict::Rejected
                }
            }
            OracleMode::ExitZero => CandidateVerdict::Inconclusive,
        }
    }
}

fn validate_baseline_count(baselines: &[ExecutionObservation]) -> Result<(), OracleError> {
    if baselines.len() < 2 {
        Err(OracleError::TooFewBaselines)
    } else {
        Ok(())
    }
}

fn validate_complete_text_baselines(baselines: &[ExecutionObservation]) -> Result<(), OracleError> {
    if baselines.iter().any(|observation| {
        observation.timed_out()
            || observation.streams_truncated()
            || observation.termination() == TerminationReason::RunnerFailure
    }) {
        Err(OracleError::IncompleteBaseline)
    } else {
        Ok(())
    }
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .map(|pattern| Regex::new(pattern).expect("OracleSpec validates every regex"))
        .collect()
}

fn diagnostic_view(channel: DiagnosticChannel, observation: &ExecutionObservation) -> String {
    let stdout = canonical_raw(observation.stdout());
    let stderr = canonical_raw(observation.stderr());
    match channel {
        DiagnosticChannel::Stdout => stdout,
        DiagnosticChannel::Stderr => stderr,
        DiagnosticChannel::Auto | DiagnosticChannel::Combined => {
            let mut combined = String::with_capacity(
                stdout
                    .len()
                    .saturating_add(COMBINED_DELIMITER.len())
                    .saturating_add(stderr.len()),
            );
            combined.push_str(&stdout);
            combined.push_str(COMBINED_DELIMITER);
            combined.push_str(&stderr);
            combined
        }
    }
}

fn canonical_raw(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn spec_digest(
    mode: OracleMode,
    channel: DiagnosticChannel,
    required: &[String],
    reject: &[String],
) -> ContentDigest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"REPROCUT-ORACLE-SPEC-V2\0");
    bytes.push(match mode {
        OracleMode::Automatic => 0,
        OracleMode::Regex => 1,
        OracleMode::ExitZero => 2,
    });
    bytes.push(match channel {
        DiagnosticChannel::Auto => 0,
        DiagnosticChannel::Stderr => 1,
        DiagnosticChannel::Stdout => 2,
        DiagnosticChannel::Combined => 3,
    });
    encode_patterns(&mut bytes, required);
    encode_patterns(&mut bytes, reject);
    ContentDigest::of(&bytes)
}

fn encode_patterns(bytes: &mut Vec<u8>, patterns: &[String]) {
    bytes.extend_from_slice(
        &u64::try_from(patterns.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    for pattern in patterns {
        bytes.extend_from_slice(
            &u64::try_from(pattern.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        bytes.extend_from_slice(pattern.as_bytes());
    }
}
