#[cfg(test)]
mod oracle_v2_contract {
    use super::reprocut_core::{
        CandidateVerdict, DiagnosticChannel, ExecutionObservation, FailureOracle,
    };

    #[test]
    fn auto_requires_every_stable_non_empty_channel() {
        let oracle = FailureOracle::from_baselines_with_channel(
            DiagnosticChannel::Auto,
            &[
                observation(1, "stable stdout", "stable stderr"),
                observation(1, "stable stdout", "stable stderr"),
                observation(1, "stable stdout", "stable stderr"),
            ],
        )
        .expect("both channels are stable");

        assert_eq!(oracle.fingerprint().anchors().len(), 2);
        assert_eq!(
            oracle.classify(&observation(1, "changed stdout", "stable stderr")),
            CandidateVerdict::Rejected
        );
        assert_eq!(
            oracle.classify(&observation(1, "stable stdout", "stable stderr")),
            CandidateVerdict::Preserved
        );
    }

    #[test]
    fn auto_uses_stdout_when_stderr_is_empty() {
        let oracle = FailureOracle::from_baselines_with_channel(
            DiagnosticChannel::Auto,
            &[
                observation(7, "panic: emitted on stdout", ""),
                observation(7, "panic: emitted on stdout", ""),
            ],
        )
        .expect("stdout is a valid diagnostic source");

        assert_eq!(
            oracle.fingerprint().anchors()[0].channel(),
            DiagnosticChannel::Stdout
        );
        assert_eq!(
            oracle.classify(&observation(7, "panic: emitted on stdout", "")),
            CandidateVerdict::Preserved
        );
    }

    #[test]
    fn explicit_stderr_does_not_require_stable_stdout() {
        let oracle = FailureOracle::from_baselines_with_channel(
            DiagnosticChannel::Stderr,
            &[
                observation(1, "progress one", "TypeError: stable"),
                observation(1, "progress two", "TypeError: stable"),
            ],
        )
        .expect("unselected stdout cannot destabilize stderr mode");

        assert_eq!(oracle.fingerprint().anchors().len(), 1);
        assert_eq!(
            oracle.classify(&observation(1, "anything", "TypeError: stable")),
            CandidateVerdict::Preserved
        );
    }

    #[test]
    fn incomplete_candidate_is_never_preserved() {
        let oracle = FailureOracle::from_baselines_with_channel(
            DiagnosticChannel::Stdout,
            &[
                observation(1, "TypeError: stable", ""),
                observation(1, "TypeError: stable", ""),
            ],
        )
        .expect("stdout baseline is stable");
        let incomplete = ExecutionObservation::new(
            Some(1),
            None,
            b"TypeError: stable".to_vec(),
            Vec::new(),
            false,
            true,
        );

        assert_eq!(oracle.classify(&incomplete), CandidateVerdict::Inconclusive);
    }

    fn observation(exit_code: i32, stdout: &str, stderr: &str) -> ExecutionObservation {
        ExecutionObservation::new(
            Some(exit_code),
            None,
            stdout.as_bytes().to_vec(),
            stderr.as_bytes().to_vec(),
            false,
            false,
        )
    }
}
