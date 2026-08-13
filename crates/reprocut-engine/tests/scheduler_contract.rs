//! Deterministic parallel-frontier scheduler contracts.

use std::time::Duration;

use reprocut_core::{CandidateRank, CandidateVerdict, ContentDigest, FrontierClass};
use reprocut_engine::{CandidatePlan, FrontierScheduler, SchedulerError};

#[test]
fn completion_order_cannot_override_the_earliest_preserved_rank() {
    let plans = vec![
        plan(2, 0, CandidateVerdict::Rejected),
        plan(0, 30, CandidateVerdict::Preserved),
        plan(1, 15, CandidateVerdict::Preserved),
    ];

    let outcome = FrontierScheduler::evaluate(plans, 3, |payload| {
        std::thread::sleep(Duration::from_millis(payload.delay_ms));
        payload.verdict
    })
    .expect("valid frontier");

    assert_eq!(outcome.winner().expect("winner").payload().id, 0);
    assert_eq!(outcome.verdict(0), Some(CandidateVerdict::Preserved));
}

#[test]
fn worker_counts_produce_the_same_winner() {
    for jobs in [1, 2, 4, 16] {
        let plans = (0..32)
            .rev()
            .map(|id| {
                let verdict = if id == 7 || id == 19 {
                    CandidateVerdict::Preserved
                } else {
                    CandidateVerdict::Rejected
                };
                plan(id, u64::from((31 - id) % 3), verdict)
            })
            .collect();
        let outcome = FrontierScheduler::evaluate(plans, jobs, |payload| {
            std::thread::sleep(Duration::from_millis(payload.delay_ms));
            payload.verdict
        })
        .expect("valid frontier");
        assert_eq!(outcome.winner().expect("winner").payload().id, 7);
    }
}

#[test]
fn duplicate_ranks_are_rejected_before_evaluation() {
    let rank = rank(0);
    let plans = vec![
        CandidatePlan::new(rank, Payload::rejected(0)),
        CandidatePlan::new(rank, Payload::rejected(1)),
    ];

    assert_eq!(
        FrontierScheduler::evaluate(plans, 2, |_| CandidateVerdict::Rejected),
        Err(SchedulerError::DuplicateRank)
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Payload {
    id: u32,
    delay_ms: u64,
    verdict: CandidateVerdict,
}

impl Payload {
    const fn rejected(id: u32) -> Self {
        Self {
            id,
            delay_ms: 0,
            verdict: CandidateVerdict::Rejected,
        }
    }
}

fn plan(id: u32, delay_ms: u64, verdict: CandidateVerdict) -> CandidatePlan<Payload> {
    CandidatePlan::new(
        rank(id),
        Payload {
            id,
            delay_ms,
            verdict,
        },
    )
}

fn rank(id: u32) -> CandidateRank {
    CandidateRank::new(
        0,
        2,
        FrontierClass::Subset,
        id,
        ContentDigest::of(&id.to_le_bytes()),
    )
}
