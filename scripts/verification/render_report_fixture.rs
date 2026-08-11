use reprocut_report::{render_report, ChannelAnchor, ReportModel, RetentionEvidence};

fn main() {
    let report = render_report(&ReportModel {
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
        issue_markdown: "# Minimal reproduction\n".to_owned(),
    });
    print!("{report}");
}
