//! Aggregate evaluation-policy contracts.

use reprocut_core::{AggregateDecision, CandidateVerdict, EvaluationPolicy, PolicyError};

#[test]
fn flaky_policy_stops_as_soon_as_nine_successes_decide_the_result() {
    let policy = EvaluationPolicy::flaky(11, 9).expect("valid supermajority");
    let evidence = policy.aggregate(std::iter::repeat(CandidateVerdict::Preserved));

    assert_eq!(evidence.decision(), AggregateDecision::Preserved);
    assert_eq!(evidence.observed_runs(), 9);
    assert_eq!(evidence.preserved_runs(), 9);
    assert!(evidence.wilson_95().is_some());
}

#[test]
fn flaky_policy_stops_when_the_threshold_is_unreachable() {
    let policy = EvaluationPolicy::flaky(11, 9).expect("valid supermajority");
    let evidence = policy.aggregate([
        CandidateVerdict::Rejected,
        CandidateVerdict::Rejected,
        CandidateVerdict::Rejected,
        CandidateVerdict::Preserved,
    ]);

    assert_eq!(evidence.decision(), AggregateDecision::Rejected);
    assert_eq!(evidence.observed_runs(), 3);
}

#[test]
fn incomplete_evidence_can_never_be_reported_as_rejection_or_preservation() {
    let policy = EvaluationPolicy::flaky(5, 4).expect("valid supermajority");
    let evidence = policy.aggregate([
        CandidateVerdict::Inconclusive,
        CandidateVerdict::Inconclusive,
    ]);

    assert_eq!(evidence.decision(), AggregateDecision::Inconclusive);
    assert_eq!(evidence.inconclusive_runs(), 2);
}

#[test]
fn bare_majority_is_rejected_by_validation() {
    assert_eq!(
        EvaluationPolicy::flaky(11, 6),
        Err(PolicyError::RequiredNotSupermajority)
    );
}

#[test]
fn strict_policy_requires_three_preserved_runs() {
    let policy = EvaluationPolicy::strict();
    let evidence = policy.aggregate([
        CandidateVerdict::Preserved,
        CandidateVerdict::Preserved,
        CandidateVerdict::Preserved,
    ]);

    assert_eq!(evidence.decision(), AggregateDecision::Preserved);
    assert_eq!(evidence.observed_runs(), 3);
}
