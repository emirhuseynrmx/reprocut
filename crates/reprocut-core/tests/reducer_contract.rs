//! Deterministic delta-reduction contracts.

use reprocut_core::{reduce, reduce_hierarchical_frontiers, CandidateVerdict, ReductionUnit};

#[test]
fn removes_every_unit_not_required_for_failure() {
    let units = ["a", "bug.py", "b", "c"]
        .into_iter()
        .enumerate()
        .map(|(id, path)| {
            ReductionUnit::new(
                u32::try_from(id).expect("fixture identifier fits u32"),
                path.into(),
            )
        })
        .collect::<Vec<_>>();

    let result = reduce(&units, |kept| {
        if kept.iter().any(|unit| unit.path() == "bug.py") {
            CandidateVerdict::Preserved
        } else {
            CandidateVerdict::Rejected
        }
    });

    assert_eq!(
        result
            .kept()
            .iter()
            .map(ReductionUnit::path)
            .collect::<Vec<_>>(),
        vec!["bug.py"]
    );
    assert!(result.attempts() < 10);
}

#[test]
fn inconclusive_candidates_are_never_accepted() {
    let units = ["oracle.txt", "noise.txt"]
        .into_iter()
        .enumerate()
        .map(|(id, path)| {
            ReductionUnit::new(
                u32::try_from(id).expect("fixture identifier fits u32"),
                path.into(),
            )
        })
        .collect::<Vec<_>>();

    let result = reduce(&units, |kept| {
        if kept.len() == 1 {
            CandidateVerdict::Inconclusive
        } else {
            CandidateVerdict::Preserved
        }
    });

    assert_eq!(result.kept(), units);
}

#[test]
fn result_is_independent_of_input_storage_addresses() {
    let first = required_pair_fixture();
    let second = required_pair_fixture();

    let first_result = reduce_required_pair(&first);
    let second_result = reduce_required_pair(&second);

    assert_eq!(first_result, second_result);
}

#[test]
fn direct_subset_search_escapes_a_complement_only_local_minimum() {
    let units = (0..6)
        .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
        .collect::<Vec<_>>();

    let result = reduce(&units, |kept| {
        let ids = kept
            .iter()
            .copied()
            .map(ReductionUnit::id)
            .collect::<Vec<_>>();
        if ids == [0, 1, 2, 3, 4, 5] || ids == [2, 3] {
            CandidateVerdict::Preserved
        } else {
            CandidateVerdict::Rejected
        }
    });

    assert_eq!(
        result
            .kept()
            .iter()
            .map(ReductionUnit::id)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

#[test]
fn batched_frontiers_accept_only_a_terminal_prefix_winner() {
    let units = (0..8)
        .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
        .collect::<Vec<_>>();
    let mut observed_parallel_frontier = false;

    let result = reduce_hierarchical_frontiers(&units, &[], |frontier| {
        observed_parallel_frontier |= frontier.len() > 1;
        let mut verdicts = frontier
            .iter()
            .map(|candidate| {
                Some(if candidate.iter().any(|unit| unit.id() == 3) {
                    CandidateVerdict::Preserved
                } else {
                    CandidateVerdict::Rejected
                })
            })
            .collect::<Vec<_>>();
        if let Some(winner) = verdicts
            .iter()
            .position(|verdict| *verdict == Some(CandidateVerdict::Preserved))
        {
            verdicts
                .iter_mut()
                .skip(winner + 1)
                .for_each(|slot| *slot = None);
        }
        verdicts
    });

    assert!(observed_parallel_frontier);
    assert_eq!(
        result
            .kept()
            .iter()
            .map(ReductionUnit::id)
            .collect::<Vec<_>>(),
        vec![3]
    );
}

#[test]
fn missing_evidence_before_a_preserved_rank_is_fail_closed() {
    let units = (0..4)
        .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
        .collect::<Vec<_>>();

    let result = reduce_hierarchical_frontiers(&units, &[], |frontier| {
        let mut verdicts = vec![Some(CandidateVerdict::Rejected); frontier.len()];
        if verdicts.len() > 1 {
            verdicts[0] = None;
            verdicts[1] = Some(CandidateVerdict::Preserved);
        }
        verdicts
    });

    assert_eq!(result.kept(), units);
}

fn required_pair_fixture() -> Vec<ReductionUnit> {
    ["noise-a", "left", "noise-b", "right", "noise-c"]
        .into_iter()
        .enumerate()
        .map(|(id, path)| {
            ReductionUnit::new(
                u32::try_from(id).expect("fixture identifier fits u32"),
                path.into(),
            )
        })
        .collect()
}

fn reduce_required_pair(units: &[ReductionUnit]) -> Vec<String> {
    reduce(units, |kept| {
        let left = kept.iter().any(|unit| unit.path() == "left");
        let right = kept.iter().any(|unit| unit.path() == "right");
        if left && right {
            CandidateVerdict::Preserved
        } else {
            CandidateVerdict::Rejected
        }
    })
    .kept()
    .iter()
    .map(|unit| unit.path().to_owned())
    .collect()
}
