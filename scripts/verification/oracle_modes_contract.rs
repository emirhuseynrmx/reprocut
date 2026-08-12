#[cfg(test)]
mod oracle_modes_contract {
    use crate::reprocut_core::{
        CandidateVerdict, DiagnosticChannel, ExecutionObservation, FailureOracle, OracleError,
        OracleMode, OracleSpec, TerminationReason,
    };

    fn observation(code: i32, stdout: &str, stderr: &str, truncated: bool) -> ExecutionObservation {
        ExecutionObservation::new(
            Some(code),
            None,
            stdout.as_bytes().to_vec(),
            stderr.as_bytes().to_vec(),
            false,
            truncated,
        )
    }

    fn timed_out() -> ExecutionObservation {
        ExecutionObservation::new(None, None, Vec::new(), Vec::new(), true, false)
    }

    #[test]
    fn exit_zero_uses_only_successful_termination() {
        let spec = OracleSpec::new(
            OracleMode::ExitZero,
            DiagnosticChannel::Auto,
            Vec::new(),
            Vec::new(),
        )
        .expect("exit-zero spec");
        let oracle = FailureOracle::from_spec_and_baselines(
            spec,
            &[observation(0, "", "", false), observation(0, "", "", false)],
        )
        .expect("successful baselines");

        assert_eq!(
            oracle.classify(&observation(0, "large", "output", true)),
            CandidateVerdict::Preserved
        );
        assert_eq!(
            oracle.classify(&observation(9, "", "", false)),
            CandidateVerdict::Rejected
        );
        assert_eq!(
            oracle.classify(&timed_out()),
            CandidateVerdict::Inconclusive
        );
        assert_eq!(oracle.fingerprint().mode(), OracleMode::ExitZero);
        assert_eq!(
            oracle.fingerprint().termination(),
            TerminationReason::ExitCode(0)
        );
        assert!(oracle.fingerprint().anchors().is_empty());
    }

    #[test]
    fn regex_requires_every_pattern_and_reject_vetoes_first() {
        let spec = OracleSpec::new(
            OracleMode::Regex,
            DiagnosticChannel::Stderr,
            vec![
                "TypeError: invoice [0-9]+".to_owned(),
                "currency".to_owned(),
            ],
            vec!["secondary failure".to_owned()],
        )
        .expect("regex spec");
        let oracle = FailureOracle::from_spec_and_baselines(
            spec,
            &[
                observation(1, "", "TypeError: invoice 7 currency", false),
                observation(1, "", "TypeError: invoice 8 currency", false),
            ],
        )
        .expect("patterns match baselines");

        assert_eq!(
            oracle.classify(&observation(
                1,
                "",
                "TypeError: invoice 9 currency\nsecondary failure",
                false,
            )),
            CandidateVerdict::Rejected
        );
        assert_eq!(
            oracle.classify(&observation(1, "", "TypeError: invoice 9", false)),
            CandidateVerdict::Rejected
        );
        assert_eq!(
            oracle.classify(&observation(1, "", "TypeError: invoice 9 currency", false)),
            CandidateVerdict::Preserved
        );
    }

    #[test]
    fn automatic_reject_pattern_vetoes_an_exact_anchor() {
        let spec = OracleSpec::new(
            OracleMode::Automatic,
            DiagnosticChannel::Stderr,
            Vec::new(),
            vec!["secondary failure".to_owned()],
        )
        .expect("automatic spec");
        let oracle = FailureOracle::from_spec_and_baselines(
            spec,
            &[
                observation(1, "", "TypeError: invoice 7", false),
                observation(1, "", "TypeError: invoice 7", false),
            ],
        )
        .expect("automatic baseline");

        assert_eq!(
            oracle.classify(&observation(
                1,
                "",
                "TypeError: invoice 7\nsecondary failure",
                false,
            )),
            CandidateVerdict::Rejected
        );
    }

    #[test]
    fn invalid_and_unbounded_specs_fail_before_observation() {
        assert_eq!(
            OracleSpec::new(
                OracleMode::Regex,
                DiagnosticChannel::Auto,
                Vec::new(),
                Vec::new(),
            )
            .expect_err("regex requires identity"),
            OracleError::InvalidConfiguration
        );
        assert_eq!(
            OracleSpec::new(
                OracleMode::ExitZero,
                DiagnosticChannel::Auto,
                vec!["x".to_owned()],
                Vec::new(),
            )
            .expect_err("exit-zero rejects patterns"),
            OracleError::InvalidConfiguration
        );
        assert_eq!(
            OracleSpec::new(
                OracleMode::Regex,
                DiagnosticChannel::Auto,
                vec!["(".to_owned()],
                Vec::new(),
            )
            .expect_err("invalid regex"),
            OracleError::InvalidPattern
        );
        assert_eq!(
            OracleSpec::new(
                OracleMode::Regex,
                DiagnosticChannel::Auto,
                vec!["x".repeat(4097)],
                Vec::new(),
            )
            .expect_err("pattern byte budget"),
            OracleError::PatternTooLong
        );
        assert_eq!(
            OracleSpec::new(
                OracleMode::Regex,
                DiagnosticChannel::Auto,
                (0..17).map(|index| format!("p{index}")).collect(),
                Vec::new(),
            )
            .expect_err("pattern count budget"),
            OracleError::TooManyPatterns
        );
    }

    #[test]
    fn baseline_must_satisfy_the_selected_mode() {
        let regex = OracleSpec::new(
            OracleMode::Regex,
            DiagnosticChannel::Stderr,
            vec!["TypeError".to_owned()],
            Vec::new(),
        )
        .expect("regex spec");
        assert_eq!(
            FailureOracle::from_spec_and_baselines(
                regex,
                &[
                    observation(1, "", "KeyError", false),
                    observation(1, "", "KeyError", false),
                ],
            )
            .expect_err("required pattern absent"),
            OracleError::BaselinePatternMismatch
        );

        let exit_zero = OracleSpec::new(
            OracleMode::ExitZero,
            DiagnosticChannel::Auto,
            Vec::new(),
            Vec::new(),
        )
        .expect("exit-zero spec");
        assert_eq!(
            FailureOracle::from_spec_and_baselines(
                exit_zero,
                &[observation(1, "", "", false), observation(1, "", "", false),],
            )
            .expect_err("successful baseline required"),
            OracleError::ExitZeroBaselineRequired
        );
    }

    #[test]
    fn fingerprint_identity_binds_mode_channel_and_patterns() {
        let automatic = OracleSpec::new(
            OracleMode::Automatic,
            DiagnosticChannel::Stderr,
            Vec::new(),
            Vec::new(),
        )
        .expect("automatic");
        let regex = OracleSpec::new(
            OracleMode::Regex,
            DiagnosticChannel::Stderr,
            vec!["TypeError".to_owned()],
            Vec::new(),
        )
        .expect("regex");
        let other_channel = OracleSpec::new(
            OracleMode::Regex,
            DiagnosticChannel::Stdout,
            vec!["TypeError".to_owned()],
            Vec::new(),
        )
        .expect("other channel");

        assert_ne!(automatic.digest(), regex.digest());
        assert_ne!(regex.digest(), other_channel.digest());
        let baselines = [
            observation(1, "", "TypeError: invoice", false),
            observation(1, "", "TypeError: invoice", false),
        ];
        let first = FailureOracle::from_spec_and_baselines(automatic, &baselines)
            .expect("automatic baseline");
        let second =
            FailureOracle::from_spec_and_baselines(regex, &baselines).expect("regex baseline");
        assert_ne!(first.fingerprint().digest(), second.fingerprint().digest());
        assert_eq!(first.fingerprint().normalization_schema(), 2);
    }
}
