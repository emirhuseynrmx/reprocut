use proptest::prelude::*;
use reprocut_core::{normalize_diagnostic, CandidateVerdict, ExecutionObservation, FailureOracle};

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

proptest! {
    #[test]
    fn normalization_is_idempotent(input in ".{0,512}") {
        let once = normalize_diagnostic(&input);
        let twice = normalize_diagnostic(&once);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn an_identical_non_empty_failure_is_preserved(anchor in "[a-zA-Z][a-zA-Z0-9 :_-]{0,128}") {
        let observation = failed(&anchor);
        let oracle = FailureOracle::from_baselines(&[
            observation.clone(), observation.clone(), observation.clone(),
        ]).expect("generated anchor is non-empty and stable");

        prop_assert_eq!(oracle.classify(&observation), CandidateVerdict::Preserved);
    }
}
