use std::{ops::Range, sync::Arc};

use crate::{CandidateVerdict, FrontierClass};

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

/// One deterministic subset or complement in a ddmin frontier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontierPartition {
    class: FrontierClass,
    positions: Range<usize>,
}

impl FrontierPartition {
    /// Returns whether this plan keeps the subset or its complement.
    pub const fn class(&self) -> FrontierClass {
        self.class
    }

    /// Returns the half-open position interval in the active universe.
    pub fn positions(&self) -> Range<usize> {
        self.positions.clone()
    }
}

/// Builds subset plans followed by complement plans in total deterministic order.
pub fn ordered_frontier(active_len: usize, granularity: usize) -> Vec<FrontierPartition> {
    if active_len == 0 {
        return Vec::new();
    }
    let granularity = granularity.clamp(1, active_len);
    let chunk_size = active_len.div_ceil(granularity);
    let partition_count = active_len.div_ceil(chunk_size);
    let mut frontier = Vec::with_capacity(partition_count.saturating_mul(2));
    for class in [FrontierClass::Subset, FrontierClass::Complement] {
        let mut start = 0_usize;
        while start < active_len {
            let end = start.saturating_add(chunk_size).min(active_len);
            frontier.push(FrontierPartition {
                class,
                positions: start..end,
            });
            start = end;
        }
    }
    frontier
}

/// Reduces an ordered universe while accepting only preserved candidates.
pub fn reduce<F>(units: &[ReductionUnit], evaluate: F) -> ReductionResult
where
    F: FnMut(&[&ReductionUnit]) -> CandidateVerdict,
{
    reduce_hierarchical(units, &[], evaluate)
}

/// Applies ordered directory groups before subset/complement ddmin.
pub fn reduce_hierarchical<F>(
    units: &[ReductionUnit],
    directory_groups: &[Vec<u32>],
    mut evaluate: F,
) -> ReductionResult
where
    F: FnMut(&[&ReductionUnit]) -> CandidateVerdict,
{
    let mut active = (0..units.len()).collect::<Vec<_>>();
    let mut candidate_indices = Vec::with_capacity(units.len());
    let mut candidate_units = Vec::with_capacity(units.len());
    let mut accepted_sizes = Vec::new();
    let mut attempts = 0_u64;

    for group in directory_groups {
        fill_without_ids(
            units,
            &active,
            group,
            &mut candidate_indices,
            &mut candidate_units,
        );
        if candidate_indices.len() == active.len() {
            continue;
        }
        attempts = attempts.saturating_add(1);
        if evaluate(&candidate_units) == CandidateVerdict::Preserved {
            std::mem::swap(&mut active, &mut candidate_indices);
            accepted_sizes.push(active.len());
        }
    }

    let mut granularity = 2_usize;
    while active.len() >= 2 {
        let active_len = active.len();
        let mut accepted = false;
        for plan in ordered_frontier(active_len, granularity) {
            fill_partition(
                units,
                &active,
                &plan,
                &mut candidate_indices,
                &mut candidate_units,
            );
            attempts = attempts.saturating_add(1);
            if evaluate(&candidate_units) != CandidateVerdict::Preserved {
                continue;
            }
            std::mem::swap(&mut active, &mut candidate_indices);
            accepted_sizes.push(active.len());
            granularity = match plan.class() {
                FrontierClass::Subset => 2,
                FrontierClass::Complement => granularity.saturating_sub(1).max(2),
                _ => unreachable!("ordered_frontier emits only subset and complement plans"),
            };
            accepted = true;
            break;
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
        let plan = FrontierPartition {
            class: FrontierClass::Complement,
            positions: position..position + 1,
        };
        fill_partition(
            units,
            &active,
            &plan,
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

fn fill_partition<'a>(
    units: &'a [ReductionUnit],
    active: &[usize],
    plan: &FrontierPartition,
    candidate_indices: &mut Vec<usize>,
    candidate_units: &mut Vec<&'a ReductionUnit>,
) {
    candidate_indices.clear();
    candidate_units.clear();
    for (position, &index) in active.iter().enumerate() {
        let inside = plan.positions.contains(&position);
        let keep = match plan.class {
            FrontierClass::Subset => inside,
            FrontierClass::Complement => !inside,
            _ => false,
        };
        if keep {
            candidate_indices.push(index);
            candidate_units.push(&units[index]);
        }
    }
}

fn fill_without_ids<'a>(
    units: &'a [ReductionUnit],
    active: &[usize],
    removed_ids: &[u32],
    candidate_indices: &mut Vec<usize>,
    candidate_units: &mut Vec<&'a ReductionUnit>,
) {
    debug_assert!(removed_ids.windows(2).all(|pair| pair[0] < pair[1]));
    candidate_indices.clear();
    candidate_units.clear();
    for &index in active {
        if removed_ids.binary_search(&units[index].id()).is_err() {
            candidate_indices.push(index);
            candidate_units.push(&units[index]);
        }
    }
}
