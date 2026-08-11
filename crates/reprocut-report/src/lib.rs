#![forbid(unsafe_code)]

use std::fmt::Write as _;

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
    pub command: String,
    pub original_files: usize,
    pub retained_files: usize,
    pub attempts: u64,
    pub inconclusive_attempts: u64,
    pub cache_hits: u64,
    pub accepted_sizes: Vec<usize>,
    pub fingerprint: String,
    pub kept_files: Vec<String>,
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
        .replace("{{ATTEMPTS}}", &model.attempts.to_string())
        .replace(
            "{{INCONCLUSIVE_ATTEMPTS}}",
            &model.inconclusive_attempts.to_string(),
        )
        .replace("{{CACHE_HITS}}", &model.cache_hits.to_string())
        .replace("{{REDUCTION_PERCENT}}", &format_tenths(reduction_tenths))
        .replace("{{RETAINED_PERCENT}}", &format_tenths(retained_tenths))
        .replace("{{FINGERPRINT}}", &escape_html(&model.fingerprint))
        .replace("{{STAGES}}", &render_stages(model))
        .replace("{{KEPT_FILES}}", &render_kept_files(&model.kept_files))
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

fn render_kept_files(files: &[String]) -> String {
    let estimated_size = files
        .iter()
        .map(String::len)
        .sum::<usize>()
        .saturating_add(files.len().saturating_mul(32));
    let mut items = String::with_capacity(estimated_size);

    for file in files {
        write!(items, "<li><code>{}</code></li>", escape_html(file))
            .expect("writing to a String cannot fail");
    }

    items
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
