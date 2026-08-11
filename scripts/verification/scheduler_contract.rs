#[cfg(test)]
mod scheduler_contract {
    use std::time::Duration;

    use crate::reprocut_core::{CandidateRank, CandidateVerdict, ContentDigest, FrontierClass};
    use crate::reprocut_engine::{CandidatePlan, FrontierScheduler, SchedulerError};

    #[test]
    fn lower_rank_wins_even_when_a_later_candidate_finishes_first() {
        let plans = vec![
            plan(2, 0, CandidateVerdict::Rejected),
            plan(0, 30, CandidateVerdict::Preserved),
            plan(1, 10, CandidateVerdict::Preserved),
        ];
        let outcome = FrontierScheduler::evaluate(plans, 3, |payload| {
            std::thread::sleep(Duration::from_millis(payload.delay_ms));
            payload.verdict
        })
        .expect("valid frontier");
        assert_eq!(outcome.winner().expect("winner").payload().id, 0);
    }

    #[test]
    fn one_two_four_and_sixteen_workers_select_the_same_rank() {
        for jobs in [1, 2, 4, 16] {
            let plans = (0..24)
                .rev()
                .map(|id| {
                    let verdict = if id == 7 || id == 19 {
                        CandidateVerdict::Preserved
                    } else {
                        CandidateVerdict::Rejected
                    };
                    plan(id, u64::from((23 - id) % 3), verdict)
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
    fn duplicate_rank_is_fail_closed() {
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
}
