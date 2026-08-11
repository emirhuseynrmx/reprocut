use std::io::Write;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current machine-readable reduction evidence schema.
pub const EVIDENCE_SCHEMA_VERSION: u16 = 2;

/// The single immutable model used by every publication surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReductionEvidence {
    pub schema_version: u16,
    pub source_root: String,
    pub output: String,
    pub command: Vec<String>,
    pub ecosystem: String,
    pub preparation: String,
    pub measurements: MeasurementSet,
    pub search: SearchEvidence,
    pub failure: FailureEvidence,
    pub kept_files: Vec<RetentionEvidence>,
    pub accepted_structured_edits: Vec<String>,
    pub attempts: Vec<AttemptSummary>,
    pub limitations: Vec<String>,
}

/// Before/after project mass and end-to-end elapsed time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeasurementSet {
    pub original: MaterialMeasurement,
    pub retained: MaterialMeasurement,
    pub elapsed_ms: u64,
}

/// One consistent project-mass measurement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MaterialMeasurement {
    pub files: u64,
    pub bytes: u64,
    pub lines: u64,
    pub syntax_nodes: Option<u64>,
}

/// Deterministic search counters and accepted-state history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchEvidence {
    pub attempts: u64,
    pub file_attempts: u64,
    pub structured_attempts: u64,
    pub inconclusive_attempts: u64,
    pub cache_hits: u64,
    pub baseline_runs: u16,
    pub final_verifications: u16,
    pub jobs: usize,
    pub state: Option<String>,
    pub resumed: bool,
    pub accepted_file_sizes: Vec<usize>,
    pub evaluation_policy: EvaluationPolicyEvidence,
}

/// Repeated-run threshold used to classify every candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvaluationPolicyEvidence {
    pub mode: String,
    pub runs: u16,
    pub required: u16,
}

/// One stream-qualified normalized diagnostic anchor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChannelAnchor {
    pub channel: String,
    pub text: String,
}

/// Stable failure identity and final same-failure result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FailureEvidence {
    pub same_failure: bool,
    pub fingerprint_sha256: String,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub termination: String,
    pub oracle_stream: String,
    pub anchor: String,
    pub anchors: Vec<ChannelAnchor>,
    pub normalization_schema: u16,
}

/// Honest evidence for a path present in the final verified snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionEvidence {
    pub path: String,
    pub observation: String,
}

/// One append-only aggregate candidate event, including retries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttemptSummary {
    pub event_id: u64,
    pub candidate_sha256: String,
    pub verdict: String,
    pub observed_runs: u16,
    pub inconclusive_runs: u16,
    pub completed_at_unix: i64,
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
