use std::io::Write;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{RetainedEntryKind, RetainedManifest};

/// Current machine-readable reduction evidence schema.
pub const EVIDENCE_SCHEMA_VERSION: u16 = reprocut_core::EVIDENCE_SCHEMA;

/// Diagnostic normalization contract accepted by this evidence schema.
pub const NORMALIZATION_SCHEMA_VERSION: u16 = reprocut_core::NORMALIZATION_SCHEMA;

/// The single immutable model used by every publication surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReductionEvidence {
    /// Evidence schema generation.
    pub schema_version: u16,
    /// Display-only source root recorded by the caller.
    pub source_root: String,
    /// SHA-256 identity of the immutable source snapshot used for the session.
    pub source_snapshot_sha256: String,
    /// Display-only artifact destination recorded by the caller.
    pub output: String,
    /// Exact reproduction argument vector.
    pub command: Vec<String>,
    /// Selected ecosystem adapter name.
    pub ecosystem: String,
    /// Candidate preparation policy and complete contract identity.
    pub preparation: PreparationEvidence,
    /// Before/after project mass and elapsed time.
    pub measurements: MeasurementSet,
    /// Search policy, counters, and accepted history.
    pub search: SearchEvidence,
    /// Stabilized failure identity and final verdict.
    pub failure: FailureEvidence,
    /// Files present in the final verified snapshot.
    pub kept_files: Vec<RetentionEvidence>,
    /// Canonical byte and metadata identity for every retained entry.
    pub retained_manifest: RetainedManifest,
    /// Every individual execution used to authorize final publication.
    pub final_observations: Vec<FinalObservationEvidence>,
    /// Canonical descriptions of accepted manifest or syntax edits.
    pub accepted_structured_edits: Vec<String>,
    /// Durable candidate observations in event order.
    pub attempts: Vec<AttemptSummary>,
    /// Explicit qualifications that constrain interpretation of this record.
    pub limitations: Vec<String>,
}

/// Before/after project mass and end-to-end elapsed time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeasurementSet {
    /// Mass of the original immutable snapshot.
    pub original: MaterialMeasurement,
    /// Mass of the final repeatedly verified snapshot.
    pub retained: MaterialMeasurement,
    /// One end-to-end wall-clock observation in milliseconds.
    pub elapsed_ms: u64,
}

/// One consistent project-mass measurement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MaterialMeasurement {
    /// Number of regular files.
    pub files: u64,
    /// Saturating sum of file byte lengths.
    pub bytes: u64,
    /// Saturating logical line count.
    pub lines: u64,
    /// Optional grammar-valid syntax-node count.
    pub syntax_nodes: Option<u64>,
}

/// Deterministic search counters and accepted-state history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchEvidence {
    /// Total file and structured candidate attempts.
    pub attempts: u64,
    /// File/directory search attempts.
    pub file_attempts: u64,
    /// Manifest and syntax search attempts.
    pub structured_attempts: u64,
    /// Attempts whose evidence could not authorize a decision.
    pub inconclusive_attempts: u64,
    /// Candidate results reused from compatible memory or durable state.
    pub cache_hits: u64,
    /// Observations used to stabilize the untouched failure.
    pub baseline_runs: u16,
    /// Preserved observations required before final publication.
    pub final_verifications: u16,
    /// Requested maximum worker count; zero means engine-selected.
    pub jobs: usize,
    /// Optional durable journal path.
    pub state: Option<String>,
    /// Whether compatible evidence was reused from an earlier session.
    pub resumed: bool,
    /// Original and successively accepted file counts.
    pub accepted_file_sizes: Vec<usize>,
    /// Repeated-execution classification policy.
    pub evaluation_policy: EvaluationPolicyEvidence,
}

/// Repeated-run threshold used to classify every candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvaluationPolicyEvidence {
    /// `strict` or `flaky`.
    pub mode: String,
    /// Maximum observations for one aggregate verdict.
    pub runs: u16,
    /// Preserved observations required for acceptance.
    pub required: u16,
}

/// Frozen candidate-preparation evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparationEvidence {
    /// `none`, `offline`, `lifecycle_scripts`, or `isolated_python`.
    pub mode: String,
    /// Complete preparation identity when the engine can bind it.
    pub contract_sha256: Option<String>,
    /// Explicit limits when no preparation identity is available.
    pub limitations: Vec<String>,
}

/// One stream-qualified normalized diagnostic anchor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChannelAnchor {
    /// `stdout` or `stderr` channel identity.
    pub channel: String,
    /// Stable normalized diagnostic line.
    pub text: String,
}

/// Stable failure identity and final same-failure result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FailureEvidence {
    /// Whether final verification preserved the stabilized identity.
    pub same_failure: bool,
    /// Domain-separated lowercase SHA-256 failure identity.
    pub fingerprint_sha256: String,
    /// Process exit code when termination was code-based.
    pub exit_code: Option<i32>,
    /// Unix signal number when termination was signal-based.
    pub signal: Option<i32>,
    /// Portable human-readable termination description.
    pub termination: String,
    /// Configured diagnostic stream selection.
    pub oracle_stream: String,
    /// `automatic`, `regex`, or `exit_zero`.
    pub oracle_mode: String,
    /// Primary backward-compatible diagnostic anchor.
    pub anchor: String,
    /// All stream-qualified anchors used for classification.
    pub anchors: Vec<ChannelAnchor>,
    /// Diagnostic normalization algorithm generation.
    pub normalization_schema: u16,
    /// Canonical required regex expressions.
    pub failure_patterns: Vec<String>,
    /// Canonical reject regex expressions.
    pub reject_patterns: Vec<String>,
    /// SHA-256 identity of the complete oracle configuration.
    pub oracle_spec_sha256: String,
}

/// Honest evidence for a path present in the final verified snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionEvidence {
    /// Normalized project-relative path.
    pub path: String,
    /// Honest observation supporting its presence in the final record.
    pub observation: String,
}

/// One individual final same-failure execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FinalObservationEvidence {
    /// One-based observation order.
    pub ordinal: u16,
    /// `preserved`, `rejected`, or `inconclusive` oracle classification.
    pub verdict: String,
    /// Portable process termination description.
    pub termination: String,
    /// Process exit status when code-based.
    pub exit_code: Option<i32>,
    /// Unix signal number when signal-based.
    pub signal: Option<i32>,
    /// Whether the execution deadline elapsed.
    pub timed_out: bool,
    /// Whether either bounded stream was truncated.
    pub streams_truncated: bool,
    /// Process-tree ownership primitive used for cleanup.
    pub containment: String,
    /// SHA-256 identity of exact bounded standard-output bytes.
    pub stdout_sha256: String,
    /// Exact captured standard-output byte length.
    pub stdout_bytes: u64,
    /// SHA-256 identity of exact bounded standard-error bytes.
    pub stderr_sha256: String,
    /// Exact captured standard-error byte length.
    pub stderr_bytes: u64,
}

/// One append-only aggregate candidate event, including retries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttemptSummary {
    /// Monotonic durable event identifier.
    pub event_id: u64,
    /// Lowercase SHA-256 candidate snapshot identity.
    pub candidate_sha256: String,
    /// Aggregate `preserved`, `rejected`, or `inconclusive` verdict.
    pub verdict: String,
    /// Completed child observations contributing to the verdict.
    pub observed_runs: u16,
    /// Incomplete observations contributing no authorization.
    pub inconclusive_runs: u16,
    /// Completion time as Unix seconds for audit ordering.
    pub completed_at_unix: i64,
    /// Versioned engine-specific aggregate evidence payload.
    pub evidence: Value,
}

/// Streams newline-delimited attempts without building a second JSON array.
///
/// # Errors
///
/// Returns a [`serde_json::Error`] when an attempt cannot be serialized or the destination writer
/// rejects a record or newline.
pub fn write_attempts_jsonl<W: Write>(
    attempts: &[AttemptSummary],
    mut output: W,
) -> serde_json::Result<()> {
    for attempt in attempts {
        serde_json::to_writer(&mut output, attempt)?;
        output.write_all(b"\n").map_err(serde_json::Error::io)?;
    }
    Ok(())
}

impl ReductionEvidence {
    /// Returns a deterministic display command derived only from the argv model.
    pub fn display_command(&self) -> String {
        display_command(&self.command)
    }

    /// Validates cryptographic fields and mode-specific evidence invariants.
    ///
    /// # Errors
    ///
    /// Returns a stable reason string when the schema, digest encoding, preparation identity,
    /// final verification, or mode-specific oracle contract is invalid.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != EVIDENCE_SCHEMA_VERSION {
            return Err("unsupported evidence schema");
        }
        if !lower_sha256(&self.source_snapshot_sha256)
            || !lower_sha256(&self.failure.fingerprint_sha256)
            || !lower_sha256(&self.failure.oracle_spec_sha256)
        {
            return Err("evidence digests must be lowercase SHA-256 hex");
        }
        match &self.preparation.contract_sha256 {
            Some(digest) if !lower_sha256(digest) => {
                return Err("preparation digest must be lowercase SHA-256 hex")
            }
            None if self.preparation.limitations.is_empty() => {
                return Err("missing preparation digest requires an explicit limitation")
            }
            Some(_) | None => {}
        }
        if !self.failure.same_failure || self.search.final_verifications == 0 {
            return Err("same-failure evidence requires final verification");
        }
        self.retained_manifest
            .validate()
            .map_err(|_| "invalid retained manifest")?;
        let retained_file_entries = self
            .retained_manifest
            .entries()
            .iter()
            .filter(|entry| entry.kind == RetainedEntryKind::RegularFile)
            .count();
        let retained_paths = self
            .retained_manifest
            .entries()
            .iter()
            .filter(|entry| entry.kind == RetainedEntryKind::RegularFile)
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        let kept_paths = self
            .kept_files
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        if u64::try_from(retained_file_entries).unwrap_or(u64::MAX)
            != self.measurements.retained.files
            || self.retained_manifest.total_bytes() != self.measurements.retained.bytes
            || retained_paths != kept_paths
        {
            return Err("retained manifest disagrees with measurements");
        }
        if usize::from(self.search.final_verifications) != self.final_observations.len()
            || self
                .final_observations
                .iter()
                .enumerate()
                .any(|(index, observation)| {
                    observation.ordinal
                        != u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX)
                        || observation.verdict != "preserved"
                        || observation.timed_out
                        || observation.streams_truncated
                        || observation.containment != "direct_child"
                            && observation.containment != "posix_process_group"
                            && observation.containment != "windows_job_object"
                        || !termination_fields_agree(observation)
                        || !lower_sha256(&observation.stdout_sha256)
                        || !lower_sha256(&observation.stderr_sha256)
                })
        {
            return Err("final observations do not prove publication");
        }
        match self.failure.oracle_mode.as_str() {
            "automatic"
                if self.failure.failure_patterns.is_empty()
                    && !self.failure.anchors.is_empty()
                    && self.failure.normalization_schema == NORMALIZATION_SCHEMA_VERSION => {}
            "regex"
                if !self.failure.failure_patterns.is_empty()
                    && self.failure.anchors.is_empty()
                    && self.failure.normalization_schema == NORMALIZATION_SCHEMA_VERSION => {}
            "exit_zero"
                if self.failure.failure_patterns.is_empty()
                    && self.failure.reject_patterns.is_empty()
                    && self.failure.anchors.is_empty()
                    && self.failure.anchor.is_empty()
                    && self.failure.normalization_schema == NORMALIZATION_SCHEMA_VERSION => {}
            "automatic" | "regex" | "exit_zero" => {
                return Err("oracle evidence violates its mode contract")
            }
            _ => return Err("unsupported oracle evidence mode"),
        }
        Ok(())
    }
}

fn termination_fields_agree(observation: &FinalObservationEvidence) -> bool {
    match (
        observation.termination.as_str(),
        observation.exit_code,
        observation.signal,
    ) {
        (termination, Some(exit_code), None) => termination == format!("exit {exit_code}"),
        (termination, None, Some(signal)) => termination == format!("signal {signal}"),
        ("timed out", None, None) | ("runner failure", None, None) => true,
        _ => false,
    }
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Formats argv for human-facing surfaces without changing executable scripts.
pub fn display_command(command: &[String]) -> String {
    command
        .iter()
        .map(|argument| {
            if argument
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-._/:\\".contains(character))
            {
                argument.clone()
            } else {
                format!("\"{}\"", argument.replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
