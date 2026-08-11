use std::io::Write;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current machine-readable reduction evidence schema.
pub const EVIDENCE_SCHEMA_VERSION: u16 = 2;

/// The single immutable model used by every publication surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReductionEvidence {
    /// Evidence schema generation.
    pub schema_version: u16,
    /// Display-only source root recorded by the caller.
    pub source_root: String,
    /// Display-only artifact destination recorded by the caller.
    pub output: String,
    /// Exact reproduction argument vector.
    pub command: Vec<String>,
    /// Selected ecosystem adapter name.
    pub ecosystem: String,
    /// Candidate preparation policy name.
    pub preparation: String,
    /// Before/after project mass and elapsed time.
    pub measurements: MeasurementSet,
    /// Search policy, counters, and accepted history.
    pub search: SearchEvidence,
    /// Stabilized failure identity and final verdict.
    pub failure: FailureEvidence,
    /// Files present in the final verified snapshot.
    pub kept_files: Vec<RetentionEvidence>,
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
    /// Primary backward-compatible diagnostic anchor.
    pub anchor: String,
    /// All stream-qualified anchors used for classification.
    pub anchors: Vec<ChannelAnchor>,
    /// Diagnostic normalization algorithm generation.
    pub normalization_schema: u16,
}

/// Honest evidence for a path present in the final verified snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionEvidence {
    /// Normalized project-relative path.
    pub path: String,
    /// Honest observation supporting its presence in the final record.
    pub observation: String,
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
