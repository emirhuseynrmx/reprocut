use std::fmt::Write as _;

use crate::ReductionEvidence;

/// Renders paste-ready GitHub issue Markdown from the shared evidence model.
pub fn render_issue(evidence: &ReductionEvidence) -> String {
    let anchor_line = evidence
        .failure
        .anchor
        .lines()
        .next()
        .unwrap_or("preserved failure");
    let title = escape_markdown_text(&truncate_chars(anchor_line, 100));
    let removed_files = evidence
        .measurements
        .original
        .files
        .saturating_sub(evidence.measurements.retained.files);
    let removed_bytes = evidence
        .measurements
        .original
        .bytes
        .saturating_sub(evidence.measurements.retained.bytes);
    let mut issue = String::with_capacity(4_096);
    writeln!(issue, "# Minimal reproduction: {title}\n").expect("writing to String cannot fail");
    writeln!(
        issue,
        "> **Same failure verified.** Fingerprint `{}` matched across {} final execution(s).\n",
        evidence.failure.fingerprint_sha256, evidence.search.final_verifications,
    )
    .expect("writing to String cannot fail");
    issue.push_str("## Reduction\n\n");
    issue.push_str("| Measure | Before | After | Removed |\n");
    issue.push_str("|---|---:|---:|---:|\n");
    writeln!(
        issue,
        "| Files | {} | {} | {} |",
        evidence.measurements.original.files, evidence.measurements.retained.files, removed_files,
    )
    .expect("writing to String cannot fail");
    writeln!(
        issue,
        "| Bytes | {} | {} | {} |",
        evidence.measurements.original.bytes, evidence.measurements.retained.bytes, removed_bytes,
    )
    .expect("writing to String cannot fail");
    writeln!(
        issue,
        "| Lines | {} | {} | {} |\n",
        evidence.measurements.original.lines,
        evidence.measurements.retained.lines,
        evidence
            .measurements
            .original
            .lines
            .saturating_sub(evidence.measurements.retained.lines),
    )
    .expect("writing to String cannot fail");

    issue.push_str("## Failure identity\n\n");
    writeln!(
        issue,
        "- Termination: `{}`",
        escape_markdown_text(&evidence.failure.termination),
    )
    .expect("writing to String cannot fail");
    writeln!(
        issue,
        "- Oracle stream: `{}`",
        escape_markdown_text(&evidence.failure.oracle_stream),
    )
    .expect("writing to String cannot fail");
    writeln!(
        issue,
        "- Normalization schema: `{}`\n",
        evidence.failure.normalization_schema,
    )
    .expect("writing to String cannot fail");
    issue.push_str(&fenced("text", &evidence.failure.anchor));

    issue.push_str("## Reproduce\n\n");
    issue.push_str(&fenced("sh", &evidence.display_command()));

    issue.push_str("## Retained project\n\n");
    if evidence.kept_files.is_empty() {
        issue.push_str("_No regular files were retained._\n\n");
    } else {
        for file in &evidence.kept_files {
            writeln!(issue, "- `{}`", escape_markdown_text(&file.path))
                .expect("writing to String cannot fail");
        }
        issue.push('\n');
    }

    issue.push_str("## Search evidence\n\n");
    writeln!(issue, "- Candidate attempts: {}", evidence.search.attempts)
        .expect("writing to String cannot fail");
    writeln!(issue, "- Cache reuses: {}", evidence.search.cache_hits)
        .expect("writing to String cannot fail");
    writeln!(
        issue,
        "- Inconclusive candidates: {}",
        evidence.search.inconclusive_attempts,
    )
    .expect("writing to String cannot fail");
    writeln!(
        issue,
        "- Wall time: {} ms\n",
        evidence.measurements.elapsed_ms
    )
    .expect("writing to String cannot fail");

    issue.push_str("## Included evidence\n\n");
    issue.push_str("- `project/` — exact final verified snapshot\n");
    issue.push_str("- `reduction.json` — versioned shared evidence\n");
    issue.push_str("- `attempts.jsonl` — append-only candidate events\n");
    issue.push_str("- `report.html` — self-contained visual record\n");
    issue.push_str("- `reproduce.sh` / `reproduce.ps1` — quoted argv launchers\n\n");

    issue.push_str("## Limits\n\n");
    for limitation in &evidence.limitations {
        writeln!(issue, "- {}", escape_markdown_text(limitation))
            .expect("writing to String cannot fail");
    }
    issue
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut output = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        output.push('…');
    }
    output
}

fn escape_markdown_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '|' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

fn fenced(language: &str, value: &str) -> String {
    let fence_length = longest_backtick_run(value).saturating_add(1).max(3);
    let fence = "`".repeat(fence_length);
    format!("{fence}{language}\n{value}\n{fence}\n\n")
}

fn longest_backtick_run(value: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in value.chars() {
        if character == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}
