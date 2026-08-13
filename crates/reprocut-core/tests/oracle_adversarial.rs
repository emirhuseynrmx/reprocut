use reprocut_core::{
    normalize_diagnostic, CandidateVerdict, DiagnosticChannel, ExecutionObservation, FailureOracle,
    OracleError,
};

fn failed(stderr: &str) -> ExecutionObservation {
    observed("", stderr)
}

fn observed(stdout: &str, stderr: &str) -> ExecutionObservation {
    ExecutionObservation::new(
        Some(1),
        None,
        stdout.as_bytes().to_vec(),
        stderr.as_bytes().to_vec(),
        false,
        false,
    )
}

#[test]
fn separator_cannot_hide_a_changed_exception() {
    let separator = "-".repeat(80);
    let baseline = format!("{separator}\nValueError: invoice 123");
    let oracle = FailureOracle::from_baselines(&[failed(&baseline), failed(&baseline)])
        .expect("root exception is discriminative");

    assert_eq!(
        oracle.classify(&failed(&format!("{separator}\nKeyError: invoice 123"))),
        CandidateVerdict::Rejected
    );
}

#[test]
fn semantic_assertion_numbers_are_not_normalized() {
    let baseline = failed("AssertionError: left 123 right 456");
    let oracle = FailureOracle::from_baselines(&[baseline.clone(), baseline])
        .expect("assertion is discriminative");

    assert_eq!(
        oracle.classify(&failed("AssertionError: left 999 right 777")),
        CandidateVerdict::Rejected
    );
}

#[test]
fn semantic_colon_numbers_are_not_normalized_as_source_locations() {
    for (baseline, candidate) in [
        ("HTTPError: status:404", "HTTPError: status:500"),
        (
            "AssertionError: expected:123 actual:456",
            "AssertionError: expected:999 actual:777",
        ),
        ("RuntimeError: shard:12", "RuntimeError: shard:99"),
    ] {
        let oracle = FailureOracle::from_baselines(&[failed(baseline), failed(baseline)])
            .expect("semantic colon number is a discriminative failure value");

        assert_eq!(
            oracle.classify(&failed(candidate)),
            CandidateVerdict::Rejected,
            "{baseline} must differ from {candidate}"
        );
    }
}

#[test]
fn recognized_source_line_numbers_remain_volatile() {
    let oracle = FailureOracle::from_baselines(&[
        failed("TypeError: failed at src/main.rs:12"),
        failed("TypeError: failed at src/main.rs:12"),
    ])
    .expect("recognized source location is stable after normalization");

    assert_eq!(
        oracle.classify(&failed("TypeError: failed at src/main.rs:99")),
        CandidateVerdict::Preserved
    );
}

#[test]
fn combined_reserves_an_anchor_for_each_stream() {
    let stdout = "FAILED tests/a.py::test_x\nerror[E0425]: missing value\nValueError: invoice failed\nexpected 12 actual 13";
    let stderr = "fatal: disk exploded";
    let baselines = [observed(stdout, stderr), observed(stdout, stderr)];
    let oracle =
        FailureOracle::from_baselines_with_channel(DiagnosticChannel::Combined, &baselines)
            .expect("both streams contain a stable discriminator");

    let channels = oracle
        .fingerprint()
        .anchors()
        .iter()
        .map(|anchor| anchor.channel())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(oracle.fingerprint().anchors().len(), 4);
    assert!(channels.contains(&DiagnosticChannel::Stdout));
    assert!(channels.contains(&DiagnosticChannel::Stderr));
    assert_eq!(
        oracle.classify(&observed(stdout, "fatal: totally unrelated")),
        CandidateVerdict::Rejected
    );
}

#[test]
fn auto_reserves_one_anchor_for_every_error_bearing_stream_even_when_stdout_fills_four_categories()
{
    let stdout = "FAILED tests/a.py::test_x\nerror[E0425]: missing value\nValueError: invoice processing failed with detailed context\nexpected twelve actual thirteen";
    let stderr = "fatal: disk exploded";
    let baselines = [observed(stdout, stderr), observed(stdout, stderr)];
    let oracle = FailureOracle::from_baselines_with_channel(DiagnosticChannel::Auto, &baselines)
        .expect("both error-bearing streams are stable");

    let channels = oracle
        .fingerprint()
        .anchors()
        .iter()
        .map(|anchor| anchor.channel())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(oracle.fingerprint().anchors().len(), 4);
    assert!(channels.contains(&DiagnosticChannel::Stdout));
    assert!(channels.contains(&DiagnosticChannel::Stderr));
    assert_eq!(
        oracle.classify(&observed(stdout, "fatal: totally unrelated")),
        CandidateVerdict::Rejected
    );
}

#[test]
fn api_routes_and_urls_retain_semantic_status_values() {
    for (baseline, candidate) in [
        ("HTTPError: GET /api/v1:404", "HTTPError: GET /api/v1:500"),
        ("HTTPError at /api/v1:404", "HTTPError at /api/v1:500"),
        (
            "HTTPError: https://example.com/v1:404",
            "HTTPError: https://example.com/v1:500",
        ),
    ] {
        let oracle = FailureOracle::from_baselines(&[failed(baseline), failed(baseline)])
            .expect("HTTP status is a discriminative failure value");

        assert_eq!(
            oracle.classify(&failed(candidate)),
            CandidateVerdict::Rejected,
            "{baseline} must differ from {candidate}"
        );
    }
}

#[test]
fn volatile_labels_do_not_match_inside_semantic_words() {
    for (baseline, candidate) in [
        ("RuntimeError: support 404", "RuntimeError: support 500"),
        ("RuntimeError: rapid 123", "RuntimeError: rapid 999"),
        ("RuntimeError: pipeline 123", "RuntimeError: pipeline 999"),
        ("RuntimeError: 12msisdn", "RuntimeError: 13msisdn"),
    ] {
        let oracle = FailureOracle::from_baselines(&[failed(baseline), failed(baseline)])
            .expect("semantic value is a discriminative failure value");

        assert_eq!(
            oracle.classify(&failed(candidate)),
            CandidateVerdict::Rejected,
            "{baseline} must differ from {candidate}"
        );
    }
}

#[test]
fn lexically_bounded_volatile_values_remain_normalized() {
    for (baseline, candidate) in [
        ("RuntimeError: port 404", "RuntimeError: port 500"),
        ("RuntimeError: PID 123", "RuntimeError: PID 999"),
        ("RuntimeError: line 12", "RuntimeError: line 99"),
        (
            "RuntimeError: failed after 10 seconds",
            "RuntimeError: failed after 20 seconds",
        ),
    ] {
        let oracle = FailureOracle::from_baselines(&[failed(baseline), failed(baseline)])
            .expect("bounded volatile value is stable after normalization");

        assert_eq!(
            oracle.classify(&failed(candidate)),
            CandidateVerdict::Preserved,
            "{baseline} and {candidate} differ only by volatile context"
        );
    }
}

#[test]
fn explicit_extensionless_source_locations_remain_volatile() {
    for (baseline, candidate) in [
        (
            "RuntimeError: failed at src/module:12",
            "RuntimeError: failed at src/module:99",
        ),
        (
            "RuntimeError: failed at Makefile:12",
            "RuntimeError: failed at Makefile:99",
        ),
    ] {
        let oracle = FailureOracle::from_baselines(&[failed(baseline), failed(baseline)])
            .expect("source location is stable after normalization");

        assert_eq!(
            oracle.classify(&failed(candidate)),
            CandidateVerdict::Preserved,
            "{baseline} and {candidate} differ only by source line"
        );
    }
}

#[test]
fn long_duration_units_are_consumed_completely() {
    assert_eq!(
        normalize_diagnostic("RuntimeError: failed after 10 seconds"),
        "RuntimeError: failed after <duration>"
    );
}

#[test]
fn summary_cannot_hide_a_changed_pytest_node_id() {
    let summary = "================ 1 failed, 20 passed in 4.20s ================";
    let baseline = format!("FAILED tests/test_invoice.py::test_total\n{summary}");
    let oracle = FailureOracle::from_baselines(&[failed(&baseline), failed(&baseline)])
        .expect("pytest node id is discriminative");

    assert_eq!(
        oracle.classify(&failed(&format!(
            "FAILED tests/test_user.py::test_login\n{summary}"
        ))),
        CandidateVerdict::Rejected
    );
}

#[test]
fn punctuation_only_baseline_is_refused() {
    let baseline = failed("----------------------------------------");
    let error = FailureOracle::from_baselines(&[baseline.clone(), baseline])
        .expect_err("punctuation is not a failure identity");

    assert_eq!(error, OracleError::EmptyAnchor);
}

#[test]
fn shorter_stack_with_the_same_root_is_preserved() {
    let baseline = failed(
        "Traceback (most recent call last):\n  File \"src/a.py\", line 18\n  File \"src/b.py\", line 44\nValueError: invoice 123",
    );
    let oracle = FailureOracle::from_baselines_with_channel(
        DiagnosticChannel::Stderr,
        &[baseline.clone(), baseline],
    )
    .expect("root exception is stable");

    assert_eq!(
        oracle.classify(&failed("ValueError: invoice 123")),
        CandidateVerdict::Preserved
    );
}
