#![forbid(unsafe_code)]

mod evidence;
mod issue;

use std::fmt::Write as _;

pub use evidence::{
    display_command, write_attempts_jsonl, AttemptSummary, ChannelAnchor, EvaluationPolicyEvidence,
    FailureEvidence, MaterialMeasurement, MeasurementSet, PreparationEvidence, ReductionEvidence,
    RetentionEvidence, SearchEvidence, EVIDENCE_SCHEMA_VERSION, NORMALIZATION_SCHEMA_VERSION,
};
pub use issue::render_issue;

const REPORT_SHELL: &str = include_str!("../assets/report.html");
const REPORT_CSS: &str = include_str!("../assets/report.css");
const REPORT_JS: &str = include_str!("../assets/report.js");

/// Serializable data boundary for a finished reduction report.
///
/// The renderer deliberately accepts owned, display-ready values instead of
/// retaining an engine or workspace. That keeps report generation deterministic
/// and prevents a view from gaining filesystem authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportModel {
    /// Display-safe reproduction command.
    pub command: String,
    /// Original regular-file count.
    pub original_files: usize,
    /// Retained regular-file count.
    pub retained_files: usize,
    /// Original project bytes.
    pub original_bytes: u64,
    /// Retained project bytes.
    pub retained_bytes: u64,
    /// Original logical line count.
    pub original_lines: u64,
    /// Retained logical line count.
    pub retained_lines: u64,
    /// End-to-end wall-clock observation in milliseconds.
    pub elapsed_ms: u64,
    /// Total candidate attempts.
    pub attempts: u64,
    /// Attempts classified as inconclusive.
    pub inconclusive_attempts: u64,
    /// Compatible cached candidate results reused.
    pub cache_hits: u64,
    /// Preserved final verification observations.
    pub final_verifications: u16,
    /// Original and successively accepted file counts.
    pub accepted_sizes: Vec<usize>,
    /// Human-readable termination and primary anchor.
    pub fingerprint: String,
    /// Machine-stable lowercase SHA-256 failure identity.
    pub fingerprint_sha256: String,
    /// Configured diagnostic stream selection.
    pub oracle_stream: String,
    /// Oracle mode and mode-aware human-readable recognition summary.
    pub oracle_mode: String,
    /// Required expressions in regex mode.
    pub failure_patterns: Vec<String>,
    /// Reject expressions in automatic and regex modes.
    pub reject_patterns: Vec<String>,
    /// Oracle configuration identity.
    pub oracle_spec_sha256: String,
    /// Immutable source project identity.
    pub source_snapshot_sha256: String,
    /// Candidate preparation mode.
    pub preparation_mode: String,
    /// Candidate preparation identity or an explicit unavailable marker.
    pub preparation_contract: String,
    /// Diagnostic normalization generation.
    pub normalization_schema: u16,
    /// Stream-qualified diagnostic anchors.
    pub anchors: Vec<ChannelAnchor>,
    /// Files present in the final verified snapshot.
    pub kept_files: Vec<RetentionEvidence>,
    /// Accepted manifest and syntax edit descriptions.
    pub structured_edits: Vec<String>,
    /// Explicit interpretation limitations.
    pub limitations: Vec<String>,
    /// GitHub-ready Markdown derived from the same evidence.
    pub issue_markdown: String,
}

impl From<&ReductionEvidence> for ReportModel {
    fn from(evidence: &ReductionEvidence) -> Self {
        Self {
            command: evidence.display_command(),
            original_files: usize::try_from(evidence.measurements.original.files)
                .unwrap_or(usize::MAX),
            retained_files: usize::try_from(evidence.measurements.retained.files)
                .unwrap_or(usize::MAX),
            original_bytes: evidence.measurements.original.bytes,
            retained_bytes: evidence.measurements.retained.bytes,
            original_lines: evidence.measurements.original.lines,
            retained_lines: evidence.measurements.retained.lines,
            elapsed_ms: evidence.measurements.elapsed_ms,
            attempts: evidence.search.attempts,
            inconclusive_attempts: evidence.search.inconclusive_attempts,
            cache_hits: evidence.search.cache_hits,
            final_verifications: evidence.search.final_verifications,
            accepted_sizes: evidence.search.accepted_file_sizes.clone(),
            fingerprint: failure_summary(&evidence.failure),
            fingerprint_sha256: evidence.failure.fingerprint_sha256.clone(),
            oracle_stream: evidence.failure.oracle_stream.clone(),
            oracle_mode: evidence.failure.oracle_mode.clone(),
            failure_patterns: evidence.failure.failure_patterns.clone(),
            reject_patterns: evidence.failure.reject_patterns.clone(),
            oracle_spec_sha256: evidence.failure.oracle_spec_sha256.clone(),
            source_snapshot_sha256: evidence.source_snapshot_sha256.clone(),
            preparation_mode: evidence.preparation.mode.clone(),
            preparation_contract: evidence
                .preparation
                .contract_sha256
                .clone()
                .unwrap_or_else(|| "unavailable; see limitations".to_owned()),
            normalization_schema: evidence.failure.normalization_schema,
            anchors: evidence.failure.anchors.clone(),
            kept_files: evidence.kept_files.clone(),
            structured_edits: evidence.accepted_structured_edits.clone(),
            limitations: evidence.limitations.clone(),
            issue_markdown: render_issue(evidence),
        }
    }
}

/// Render a portable report with no network or filesystem dependencies.
#[must_use]
pub fn render_report(model: &ReportModel) -> String {
    let reduction_tenths = percentage_tenths(
        model.original_files.saturating_sub(model.retained_files),
        model.original_files,
    );
    let retained_tenths = percentage_tenths(model.retained_files, model.original_files);

    REPORT_SHELL
        .replace("{{CSS}}", REPORT_CSS)
        .replace("{{JS}}", REPORT_JS)
        .replace("{{COMMAND}}", &escape_html(&model.command))
        .replace("{{ORIGINAL_FILES}}", &model.original_files.to_string())
        .replace("{{RETAINED_FILES}}", &model.retained_files.to_string())
        .replace("{{ORIGINAL_BYTES}}", &model.original_bytes.to_string())
        .replace("{{RETAINED_BYTES}}", &model.retained_bytes.to_string())
        .replace("{{ORIGINAL_LINES}}", &model.original_lines.to_string())
        .replace("{{RETAINED_LINES}}", &model.retained_lines.to_string())
        .replace("{{ELAPSED_MS}}", &model.elapsed_ms.to_string())
        .replace("{{ATTEMPTS}}", &model.attempts.to_string())
        .replace(
            "{{INCONCLUSIVE_ATTEMPTS}}",
            &model.inconclusive_attempts.to_string(),
        )
        .replace("{{CACHE_HITS}}", &model.cache_hits.to_string())
        .replace(
            "{{FINAL_VERIFICATIONS}}",
            &model.final_verifications.to_string(),
        )
        .replace("{{REDUCTION_PERCENT}}", &format_tenths(reduction_tenths))
        .replace("{{RETAINED_PERCENT}}", &format_tenths(retained_tenths))
        .replace("{{FINGERPRINT}}", &escape_html(&model.fingerprint))
        .replace(
            "{{FINGERPRINT_SHA256}}",
            &escape_html(&model.fingerprint_sha256),
        )
        .replace("{{ORACLE_STREAM}}", &escape_html(&model.oracle_stream))
        .replace("{{ORACLE_MODE}}", &escape_html(&model.oracle_mode))
        .replace(
            "{{ORACLE_SPEC_SHA256}}",
            &escape_html(&model.oracle_spec_sha256),
        )
        .replace(
            "{{SOURCE_SNAPSHOT_SHA256}}",
            &escape_html(&model.source_snapshot_sha256),
        )
        .replace(
            "{{PREPARATION_MODE}}",
            &escape_html(&model.preparation_mode),
        )
        .replace(
            "{{PREPARATION_CONTRACT}}",
            &escape_html(&model.preparation_contract),
        )
        .replace(
            "{{NORMALIZATION_SCHEMA}}",
            &model.normalization_schema.to_string(),
        )
        .replace("{{ORACLE_EVIDENCE}}", &render_oracle_evidence(model))
        .replace("{{ISSUE_BASE64}}", &base64(&model.issue_markdown))
        .replace("{{ANCHORS}}", &render_anchors(&model.anchors))
        .replace("{{STAGES}}", &render_stages(model))
        .replace("{{KEPT_FILES}}", &render_kept_files(&model.kept_files))
        .replace(
            "{{STRUCTURED_EDITS}}",
            &render_string_list(&model.structured_edits, "No structured edit was accepted."),
        )
        .replace(
            "{{LIMITATIONS}}",
            &render_string_list(&model.limitations, "No limitations were recorded."),
        )
}

fn failure_summary(failure: &FailureEvidence) -> String {
    match failure.oracle_mode.as_str() {
        "automatic" => format!("{} | {}", failure.termination, failure.anchor),
        "regex" => format!("{} | required regex contract", failure.termination),
        "exit_zero" => "exit 0 | interesting command succeeded".to_owned(),
        _ => failure.termination.clone(),
    }
}

fn render_oracle_evidence(model: &ReportModel) -> String {
    match model.oracle_mode.as_str() {
        "automatic" => format!(
            "<h3>Automatic discriminators</h3><ol class=\"anchor-list\">{}</ol>",
            render_anchors(&model.anchors)
        ),
        "regex" => format!(
            "<h3>Required regex</h3><ol class=\"diagnostic-list\">{}</ol><h3>Reject regex</h3><ol class=\"diagnostic-list\">{}</ol>",
            render_string_list(&model.failure_patterns, "No required expression recorded."),
            render_string_list(&model.reject_patterns, "No reject expression configured."),
        ),
        "exit_zero" => "<h3>Exit-zero interestingness</h3><p>The candidate is preserved only when the command exits successfully; timeout, signal, or runner failure is inconclusive.</p>".to_owned(),
        _ => "<p>Unsupported oracle evidence.</p>".to_owned(),
    }
}

fn render_stages(model: &ReportModel) -> String {
    let mut rows = String::with_capacity(model.accepted_sizes.len().saturating_mul(160));

    for (index, size) in model.accepted_sizes.iter().copied().enumerate() {
        let width = percentage_tenths(size, model.original_files).max(12);
        let stage_number = index.saturating_add(1);
        write!(
            rows,
            "<li class=\"cut-stage\" style=\"--stage-width: {}%\" data-stage=\"{}\"><span class=\"stage-index\">{:02}</span><span class=\"stage-rule\" aria-hidden=\"true\"></span><strong>{}</strong><span> files retained</span></li>",
            format_tenths(width),
            stage_number,
            stage_number,
            size,
        )
        .expect("writing to a String cannot fail");
    }

    rows
}

fn render_kept_files(files: &[RetentionEvidence]) -> String {
    let estimated_size = files
        .iter()
        .map(|file| file.path.len().saturating_add(file.observation.len()))
        .sum::<usize>()
        .saturating_add(files.len().saturating_mul(32));
    let mut items = String::with_capacity(estimated_size);

    for file in files {
        write!(
            items,
            "<li><code>{}</code><span>{}</span></li>",
            escape_html(&file.path),
            escape_html(&file.observation),
        )
        .expect("writing to a String cannot fail");
    }

    items
}

fn render_anchors(anchors: &[ChannelAnchor]) -> String {
    let mut items = String::new();
    for anchor in anchors {
        write!(
            items,
            "<li><span>{}</span><code>{}</code></li>",
            escape_html(&anchor.channel),
            escape_html(&anchor.text),
        )
        .expect("writing to a String cannot fail");
    }
    items
}

fn render_string_list(values: &[String], empty: &str) -> String {
    if values.is_empty() {
        return format!("<li>{}</li>", escape_html(empty));
    }
    let mut items = String::new();
    for value in values {
        write!(items, "<li><code>{}</code></li>", escape_html(value))
            .expect("writing to a String cannot fail");
    }
    items
}

fn base64(value: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = value.as_bytes();
    let mut encoded = String::with_capacity(bytes.len().saturating_add(2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let value = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        encoded.push(char::from(ALPHABET[((value >> 18) & 0x3f) as usize]));
        encoded.push(char::from(ALPHABET[((value >> 12) & 0x3f) as usize]));
        if chunk.len() > 1 {
            encoded.push(char::from(ALPHABET[((value >> 6) & 0x3f) as usize]));
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(char::from(ALPHABET[(value & 0x3f) as usize]));
        } else {
            encoded.push('=');
        }
    }
    encoded
}

fn percentage_tenths(part: usize, whole: usize) -> usize {
    if whole == 0 {
        return 0;
    }

    part.saturating_mul(1_000).saturating_div(whole).min(1_000)
}

fn format_tenths(value: usize) -> String {
    format!("{}.{:01}", value / 10, value % 10)
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
