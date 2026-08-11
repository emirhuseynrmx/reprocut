fn main() {
    let report = render_report(&ReportModel {
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
    });
    print!("{report}");
}
