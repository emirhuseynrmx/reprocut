use std::{ops::Range, sync::Arc};

use crate::CandidateVerdict;

/// One removable project element with stable identity and shared path storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionUnit {
    id: u32,
    path: Arc<str>,
}

impl ReductionUnit {
    /// Creates a reduction unit.
    pub fn new(id: u32, path: String) -> Self {
        Self {
            id,
            path: Arc::from(path),
        }
    }

    /// Returns the stable inventory identifier.
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the project-relative display path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// The deterministic output of one reduction search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReductionResult {
    kept: Vec<ReductionUnit>,
    attempts: u64,
    accepted_sizes: Vec<usize>,
}

impl ReductionResult {
    /// Returns the locally irreducible retained units in inventory order.
    pub fn kept(&self) -> &[ReductionUnit] {
        &self.kept
    }

    /// Returns the number of candidate evaluations.
    pub const fn attempts(&self) -> u64 {
        self.attempts
    }

    /// Returns retained counts after each accepted transition.
    pub fn accepted_sizes(&self) -> &[usize] {
        &self.accepted_sizes
    }
}

/// Reduces an ordered universe while accepting only preserved candidates.
pub fn reduce<F>(units: &[ReductionUnit], mut evaluate: F) -> ReductionResult
where
    F: FnMut(&[&ReductionUnit]) -> CandidateVerdict,
{
    let mut active = (0..units.len()).collect::<Vec<_>>();
    let mut candidate_indices = Vec::with_capacity(units.len());
    let mut candidate_units = Vec::with_capacity(units.len());
    let mut accepted_sizes = Vec::new();
    let mut attempts = 0_u64;
    let mut granularity = 2_usize;

    while active.len() >= 2 {
        let active_len = active.len();
        let chunk_size = active_len.div_ceil(granularity);
        let mut accepted = false;
        let mut start = 0_usize;

        while start < active_len {
            let end = start.saturating_add(chunk_size).min(active_len);
            fill_candidate(
                units,
                &active,
                start..end,
                &mut candidate_indices,
                &mut candidate_units,
            );
            attempts = attempts.saturating_add(1);

            if evaluate(&candidate_units) == CandidateVerdict::Preserved {
                std::mem::swap(&mut active, &mut candidate_indices);
                accepted_sizes.push(active.len());
                granularity = granularity.saturating_sub(1).max(2);
                accepted = true;
                break;
            }
            start = end;
        }

        if accepted {
            continue;
        }
        if granularity >= active_len {
            break;
        }
        granularity = granularity.saturating_mul(2).min(active_len);
    }

    let mut position = 0_usize;
    while position < active.len() {
        fill_candidate(
            units,
            &active,
            position..position + 1,
            &mut candidate_indices,
            &mut candidate_units,
        );
        attempts = attempts.saturating_add(1);

        if evaluate(&candidate_units) == CandidateVerdict::Preserved {
            std::mem::swap(&mut active, &mut candidate_indices);
            accepted_sizes.push(active.len());
        } else {
            position += 1;
        }
    }

    let mut kept = Vec::with_capacity(active.len());
    kept.extend(active.into_iter().map(|index| units[index].clone()));

    ReductionResult {
        kept,
        attempts,
        accepted_sizes,
    }
}

fn fill_candidate<'a>(
    units: &'a [ReductionUnit],
    active: &[usize],
    removed: Range<usize>,
    candidate_indices: &mut Vec<usize>,
    candidate_units: &mut Vec<&'a ReductionUnit>,
) {
    candidate_indices.clear();
    candidate_units.clear();

    for (position, &index) in active.iter().enumerate() {
        if !removed.contains(&position) {
            candidate_indices.push(index);
            candidate_units.push(&units[index]);
        }
    }
}
