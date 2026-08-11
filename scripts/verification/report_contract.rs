#[cfg(test)]
mod contract_tests {
    use super::{render_report, ReportModel};

    fn fixture() -> ReportModel {
        ReportModel {
            command: "python bug.py --case <edge>".to_owned(),
            original_files: 18,
            retained_files: 3,
            attempts: 41,
            inconclusive_attempts: 2,
            cache_hits: 7,
            accepted_sizes: vec![18, 9, 5, 3],
            fingerprint: "exit 1 · ValueError: unstable <input>".to_owned(),
            kept_files: vec![
                "bug.py".to_owned(),
                "fixtures/input.json".to_owned(),
                "pyproject.toml".to_owned(),
            ],
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
            kept_files: vec!["<img src=x>".to_owned()],
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
