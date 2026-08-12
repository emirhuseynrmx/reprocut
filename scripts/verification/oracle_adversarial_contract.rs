#[cfg(test)]
mod oracle_adversarial_contract {
    use crate::reprocut_core::{
        CandidateVerdict, DiagnosticChannel, ExecutionObservation, FailureOracle, OracleError,
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
}
