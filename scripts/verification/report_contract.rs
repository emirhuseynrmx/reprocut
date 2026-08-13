#[cfg(test)]
mod contract_tests {
    use crate::reprocut_report::{render_report, ChannelAnchor, ReportModel, RetentionEvidence};

    fn fixture() -> ReportModel {
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
            normalization_schema: 4,
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
        }
    }

    #[test]
    fn report_is_complete_and_measured() {
        let report = render_report(&fixture());
        assert!(report.starts_with("<!doctype html>"));
        assert!(report.contains("83.3%"));
        assert!(report.contains("41"));
        assert!(report.contains("fixtures/input.json"));
        assert!(report.contains("aria-label=\"Reduction progress\""));
        assert!(report.contains("prefers-reduced-motion"));
        assert!(!report.contains("{{"));
        assert!(!report.contains("https://"));
    }

    #[test]
    fn user_content_is_escaped() {
        let report = render_report(&ReportModel {
            command: "<script>alert('x')</script>".to_owned(),
            fingerprint: "bad & <worse>".to_owned(),
            kept_files: vec![RetentionEvidence {
                path: "<img src=x>".to_owned(),
                observation: "bad <observation>".to_owned(),
            }],
            ..fixture()
        });
        assert!(!report.contains("<script>alert"));
        assert!(!report.contains("<img src=x"));
        assert!(report.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
        assert!(report.contains("bad &amp; &lt;worse&gt;"));
    }

    #[test]
    fn zero_file_input_has_defined_percentages() {
        let report = render_report(&ReportModel {
            original_files: 0,
            retained_files: 0,
            accepted_sizes: vec![0],
            ..fixture()
        });
        assert!(report.contains("0.0%"));
    }
}
