use reprocut_core::{
    CandidateVerdict, ExecutionObservation, FailureOracle, OracleError,
};

fn failed(stderr: &str) -> ExecutionObservation {
    ExecutionObservation::new(
        Some(1),
        None,
        Vec::new(),
        stderr.as_bytes().to_vec(),
        false,
        false,
    )
}

#[test]
fn stable_baselines_create_an_oracle() {
    let oracle = FailureOracle::from_baselines(&[
        failed("thread 91: TypeError: currency at C:\\tmp\\a.py:84"),
        failed("thread 17: TypeError: currency at C:\\tmp\\a.py:84"),
        failed("thread 02: TypeError: currency at C:\\tmp\\a.py:84"),
    ])
    .expect("volatile identifiers must normalize");

    assert!(oracle.fingerprint().anchor().contains("TypeError: currency"));
}

#[test]
fn unrelated_compile_error_is_rejected() {
    let oracle = FailureOracle::from_baselines(&[
        failed("TypeError: currency"),
        failed("TypeError: currency"),
        failed("TypeError: currency"),
    ])
    .expect("baseline is stable");

    assert_eq!(
        oracle.classify(&failed("ModuleNotFoundError: checkout")),
        CandidateVerdict::Rejected
    );
}

#[test]
fn truncated_or_timed_out_execution_is_inconclusive() {
    let oracle = FailureOracle::from_baselines(&[
        failed("TypeError: currency"),
        failed("TypeError: currency"),
        failed("TypeError: currency"),
    ])
    .expect("baseline is stable");
    let timed_out = ExecutionObservation::new(
        None,
        None,
        Vec::new(),
        Vec::new(),
        true,
        false,
    );

    assert_eq!(
        oracle.classify(&timed_out),
        CandidateVerdict::Inconclusive
    );
}

#[test]
fn unstable_baselines_are_refused() {
    let error = FailureOracle::from_baselines(&[
        failed("TypeError: currency"),
        failed("TypeError: locale"),
        failed("TypeError: currency"),
    ])
    .expect_err("different diagnostics must not form one oracle");

    assert_eq!(error, OracleError::UnstableDiagnostic);
}
