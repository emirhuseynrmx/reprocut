//! Byte-stable report golden contracts.

use reprocut_report::{
    render_report, ChannelAnchor, DriftEvidence, ReportModel, RetentionEvidence,
};

fn fixture_model() -> ReportModel {
    ReportModel {
        command: "python bug.py --case <edge>".to_owned(),
        original_files: 18,
        retained_files: 3,
        original_bytes: 12_000,
        retained_bytes: 1_024,
        original_lines: 900,
        retained_lines: 72,
        elapsed_ms: 4_200,
        attempts: 41,
        inconclusive_attempts: 2,
        cache_hits: 7,
        final_verifications: 3,
        accepted_sizes: vec![18, 9, 5, 3],
        fingerprint: "exit 1 · ValueError: unstable <input>".to_owned(),
        fingerprint_sha256: "0123456789abcdef".repeat(4),
        oracle_stream: "stderr".to_owned(),
        oracle_mode: "automatic".to_owned(),
        failure_patterns: Vec::new(),
        reject_patterns: Vec::new(),
        oracle_spec_sha256: "1".repeat(64),
        source_snapshot_sha256: "2".repeat(64),
        preparation_mode: "offline".to_owned(),
        preparation_contract: "3".repeat(64),
        normalization_schema: 5,
        anchors: vec![ChannelAnchor {
            channel: "stderr".to_owned(),
            text: "ValueError: unstable <input>".to_owned(),
        }],
        kept_files: vec![
            RetentionEvidence {
                path: "bug.py".to_owned(),
                observation: "Present in the final verified snapshot.".to_owned(),
            },
            RetentionEvidence {
                path: "fixtures/input.json".to_owned(),
                observation: "Present in the final verified snapshot.".to_owned(),
            },
            RetentionEvidence {
                path: "pyproject.toml".to_owned(),
                observation: "Present in the final verified snapshot.".to_owned(),
            },
        ],
        structured_edits: vec!["syntax:delete:bug.py:0..24".to_owned()],
        limitations: vec!["Timing is wall-clock, not a benchmark.".to_owned()],
        diagnostic_drift: Some(DriftEvidence {
            baseline_lines: 6,
            final_lines: 4,
            retained_lines: 4,
            novel_lines: 0,
            reportable: false,
            novel_sample: Vec::new(),
        }),
        issue_markdown: "# Minimal reproduction\n".to_owned(),
    }
}

fn drifted_model() -> ReportModel {
    ReportModel {
        diagnostic_drift: Some(DriftEvidence {
            baseline_lines: 2,
            final_lines: 4,
            retained_lines: 1,
            novel_lines: 3,
            reportable: true,
            novel_sample: vec!["examples/sky/original/00-standard-libs missing".to_owned()],
        }),
        ..fixture_model()
    }
}

#[test]
fn a_clean_reduction_reports_the_failure_as_verified() {
    let report = render_report(&fixture_model());

    assert!(report.contains("Same failure verified"));
    assert!(!report.contains("id=\"drift-title\""));
}

// A reader who only sees the masthead must not be told the bug was preserved when the
// minimized project's diagnostic no longer resembles the original's.
#[test]
fn a_drifted_reduction_says_so_in_the_masthead_and_the_body() {
    let report = render_report(&drifted_model());

    assert!(report.contains("Same oracle — review the drift"));
    assert!(!report.contains("Same failure verified"));
    assert!(report.contains("id=\"drift-title\""));
    assert!(report.contains("3 of the 4 diagnostic line(s)"));
    assert!(report.contains("examples/sky/original/00-standard-libs missing"));
}

#[test]
fn renders_the_reviewed_report_byte_for_byte() {
    let actual = render_report(&fixture_model()).replace("\r\n", "\n");
    let expected =
        include_str!("../../../tests/golden/reduction-report.html").replace("\r\n", "\n");

    assert_eq!(actual, expected);
}

#[test]
fn escapes_every_user_controlled_field() {
    let report = render_report(&ReportModel {
        command: "<script>alert('x')</script>".to_owned(),
        fingerprint: "bad & <worse>".to_owned(),
        kept_files: vec![RetentionEvidence {
            path: "<img src=x onerror=alert(1)>".to_owned(),
            observation: "bad <observation>".to_owned(),
        }],
        ..fixture_model()
    });

    assert!(!report.contains("<script>alert"));
    assert!(!report.contains("<img src=x"));
    assert!(report.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
    assert!(report.contains("bad &amp; &lt;worse&gt;"));
}

#[test]
fn is_self_contained_and_accessible_by_contract() {
    let report = render_report(&fixture_model());

    assert!(!report.contains("https://"));
    assert!(!report.contains("http://"));
    assert!(report.contains("prefers-reduced-motion"));
    assert!(report.contains("aria-label=\"Reduction progress\""));
    assert!(report.contains("<style>"));
    assert!(report.contains("<script>"));
}
