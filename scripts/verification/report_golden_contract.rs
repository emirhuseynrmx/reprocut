#[cfg(test)]
mod golden_contract {
    use super::{render_report, ReportModel};

    #[test]
    fn renderer_matches_the_reviewed_golden_file_byte_for_byte() {
        let actual = render_report(&ReportModel {
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
        })
        .replace("\r\n", "\n");
        let expected =
            include_str!("../../tests/golden/reduction-report.html").replace("\r\n", "\n");

        assert_eq!(actual, expected);
    }
}
