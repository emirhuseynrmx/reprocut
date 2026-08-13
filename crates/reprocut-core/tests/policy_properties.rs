//! Property tests for aggregate evaluation policies.

use reprocut_core::{AggregateDecision, CandidateVerdict, EvaluationPolicy};

#[test]
fn every_default_flaky_prefix_obeys_the_integer_decision_boundary() {
    let policy = EvaluationPolicy::default_flaky();
    for encoded in 0_u32..3_u32.pow(8) {
        let mut value = encoded;
        let verdicts = (0..8)
            .map(|_| {
                let verdict = match value % 3 {
                    0 => CandidateVerdict::Preserved,
                    1 => CandidateVerdict::Rejected,
                    _ => CandidateVerdict::Inconclusive,
                };
                value /= 3;
                verdict
            })
            .collect::<Vec<_>>();
        let evidence = policy.aggregate(verdicts.iter().copied());
        if evidence.decision() == AggregateDecision::Preserved {
            assert!(evidence.preserved_runs() >= policy.required());
        }
        assert!(evidence.observed_runs() <= policy.runs());
        assert_eq!(
            evidence.observed_runs(),
            evidence.preserved_runs() + evidence.rejected_runs() + evidence.inconclusive_runs()
        );
    }
}

#[test]
fn wilson_interval_always_contains_the_observed_rate() {
    let policy = EvaluationPolicy::flaky(101, 68).expect("valid policy");
    for preserved in 0..=101 {
        let verdicts = (0..101).map(|index| {
            if index < preserved {
                CandidateVerdict::Preserved
            } else {
                CandidateVerdict::Rejected
            }
        });
        let evidence = policy.aggregate(verdicts);
        if let (Some(rate), Some(interval)) = (evidence.observed_rate(), evidence.wilson_95()) {
            assert!(interval.lower() <= rate);
            assert!(rate <= interval.upper());
        }
    }
}
