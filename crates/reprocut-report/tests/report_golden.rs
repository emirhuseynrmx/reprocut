use reprocut_report::{ReportModel, render_report};

fn fixture_model() -> ReportModel {
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
fn renders_the_reviewed_report_byte_for_byte() {
    let actual = render_report(&fixture_model()).replace("\r\n", "\n");
    let expected = include_str!("../../../tests/golden/reduction-report.html")
        .replace("\r\n", "\n");

    assert_eq!(actual, expected);
}

#[test]
fn escapes_every_user_controlled_field() {
    let report = render_report(&ReportModel {
        command: "<script>alert('x')</script>".to_owned(),
        fingerprint: "bad & <worse>".to_owned(),
        kept_files: vec!["<img src=x onerror=alert(1)>".to_owned()],
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
