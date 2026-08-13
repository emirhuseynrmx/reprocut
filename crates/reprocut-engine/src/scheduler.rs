use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

use reprocut_core::{CandidateRank, CandidateVerdict, LowestWinner};
use thiserror::Error;

/// One immutable payload assigned a total order inside a frontier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidatePlan<T> {
    rank: CandidateRank,
    payload: T,
}

impl<T> CandidatePlan<T> {
    /// Creates a ranked candidate without cloning its payload.
    pub const fn new(rank: CandidateRank, payload: T) -> Self {
        Self { rank, payload }
    }

    /// Returns the deterministic frontier rank.
    pub const fn rank(&self) -> CandidateRank {
        self.rank
    }

    /// Returns the evaluator-specific immutable payload.
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Consumes the plan and returns its payload.
    pub fn into_payload(self) -> T {
        self.payload
    }
}

/// Ordered terminal evidence collected from one bounded frontier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontierOutcome<T> {
    plans: Vec<CandidatePlan<T>>,
    verdicts: Vec<Option<CandidateVerdict>>,
    winner: Option<usize>,
}

impl<T> FrontierOutcome<T> {
    /// Returns plans in canonical rank order, regardless of input order.
    pub fn plans(&self) -> &[CandidatePlan<T>] {
        &self.plans
    }

    /// Returns one terminal verdict, or `None` when cancellation made it unnecessary.
    pub fn verdict(&self, index: usize) -> Option<CandidateVerdict> {
        self.verdicts.get(index).copied().flatten()
    }

    /// Returns all ordered verdict slots for persistence and diagnostics.
    pub fn verdicts(&self) -> &[Option<CandidateVerdict>] {
        &self.verdicts
    }

    /// Returns the earliest preserved plan after every earlier slot became terminal.
    pub fn winner(&self) -> Option<&CandidatePlan<T>> {
        self.winner.map(|index| &self.plans[index])
    }

    /// Consumes the outcome without copying candidate payloads.
    pub fn into_parts(
        self,
    ) -> (
        Vec<CandidatePlan<T>>,
        Vec<Option<CandidateVerdict>>,
        Option<usize>,
    ) {
        (self.plans, self.verdicts, self.winner)
    }
}

/// Invalid deterministic-frontier configuration.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SchedulerError {
    /// Two plans claimed the same total-order position.
    #[error("frontier contains duplicate candidate ranks")]
    DuplicateRank,
}

/// Stateless bounded scheduler whose observable winner is independent of completion order.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrontierScheduler;

impl FrontierScheduler {
    /// Evaluates a rank-ordered frontier with at most `jobs` live workers.
    ///
    /// `jobs == 0` selects the host's available parallelism. Work assignment is
    /// an atomic monotonic index, so no receiver lock or unbounded task queue is
    /// placed in the hot path. The result channel is bounded to twice the actual
    /// worker count. A later preserved result can stop unnecessary higher ranks,
    /// but it cannot commit until all lower ranks have published terminal results.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::DuplicateRank`] when two plans claim the same position in the
    /// deterministic frontier order.
    pub fn evaluate<T, F>(
        mut plans: Vec<CandidatePlan<T>>,
        jobs: usize,
        evaluator: F,
    ) -> Result<FrontierOutcome<T>, SchedulerError>
    where
        T: Sync,
        F: Fn(&T) -> CandidateVerdict + Sync,
    {
        plans.sort_unstable_by_key(CandidatePlan::rank);
        if plans.windows(2).any(|pair| pair[0].rank == pair[1].rank) {
            return Err(SchedulerError::DuplicateRank);
        }
        if plans.is_empty() {
            return Ok(FrontierOutcome {
                plans,
                verdicts: Vec::new(),
                winner: None,
            });
        }

        let requested = if jobs == 0 {
            thread::available_parallelism().map_or(1, usize::from)
        } else {
            jobs
        };
        let workers = requested.clamp(1, plans.len());
        let capacity = workers.saturating_mul(2).max(1);
        let next = AtomicUsize::new(0);
        let lowest = LowestWinner::new();
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let mut verdicts = vec![None; plans.len()];

        thread::scope(|scope| {
            for _ in 0..workers {
                let sender = sender.clone();
                let plans = &plans;
                let evaluator = &evaluator;
                let next = &next;
                let lowest = &lowest;
                scope.spawn(move || loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= plans.len() || lowest.load().is_some_and(|winner| index > winner) {
                        break;
                    }
                    let verdict = evaluator(plans[index].payload());
                    if verdict == CandidateVerdict::Preserved {
                        lowest.claim(index);
                    }
                    if sender.send((index, verdict)).is_err() {
                        break;
                    }
                });
            }
            drop(sender);
            while let Ok((index, verdict)) = receiver.recv() {
                verdicts[index] = Some(verdict);
            }
        });

        let winner = lowest.load().and_then(|candidate| {
            verdicts[..=candidate]
                .iter()
                .all(Option::is_some)
                .then_some(candidate)
        });
        Ok(FrontierOutcome {
            plans,
            verdicts,
            winner,
        })
    }
}
