#[cfg(test)]
mod policy_contract {
    use super::reprocut_core::{
        AggregateDecision, CandidateVerdict, EvaluationPolicy, PolicyError,
    };

    #[test]
    fn success_and_failure_are_decided_early() {
        let policy = EvaluationPolicy::flaky(11, 9).expect("valid policy");
        let success = policy.aggregate(std::iter::repeat(CandidateVerdict::Preserved));
        assert_eq!(success.decision(), AggregateDecision::Preserved);
        assert_eq!(success.observed_runs(), 9);

        let failure = policy.aggregate(std::iter::repeat(CandidateVerdict::Rejected));
        assert_eq!(failure.decision(), AggregateDecision::Rejected);
        assert_eq!(failure.observed_runs(), 3);
    }

    #[test]
    fn validation_rejects_a_bare_majority() {
        assert_eq!(
            EvaluationPolicy::flaky(11, 6),
            Err(PolicyError::RequiredNotSupermajority)
        );
    }

    #[test]
    fn wilson_interval_is_bounded() {
        let evidence = EvaluationPolicy::flaky(11, 9)
            .expect("valid policy")
            .aggregate([CandidateVerdict::Preserved; 9]);
        let interval = evidence.wilson_95().expect("complete observations");
        assert!((0.0..=1.0).contains(&interval.lower()));
        assert!((0.0..=1.0).contains(&interval.upper()));
        assert!(interval.lower() <= interval.upper());
    }
}
