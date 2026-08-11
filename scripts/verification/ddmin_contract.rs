#[cfg(test)]
mod ddmin_contract {
    use super::reprocut_core::{
        reduce, reduce_hierarchical_frontiers, CandidateVerdict, ReductionUnit,
    };

    #[test]
    fn direct_subset_breaks_a_complement_only_local_minimum() {
        let units = (0..6)
            .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
            .collect::<Vec<_>>();
        let result = reduce(&units, |kept| {
            let ids = kept.iter().map(|unit| unit.id()).collect::<Vec<_>>();
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
    fn frontier_batching_is_deterministic_and_fail_closed() {
        let units = (0..8)
            .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
            .collect::<Vec<_>>();
        let mut saw_batch = false;
        let result = reduce_hierarchical_frontiers(&units, &[], |frontier| {
            saw_batch |= frontier.len() > 1;
            let mut results = frontier
                .iter()
                .map(|candidate| {
                    Some(if candidate.iter().any(|unit| unit.id() == 3) {
                        CandidateVerdict::Preserved
                    } else {
                        CandidateVerdict::Rejected
                    })
                })
                .collect::<Vec<_>>();
            if let Some(winner) = results
                .iter()
                .position(|result| *result == Some(CandidateVerdict::Preserved))
            {
                results
                    .iter_mut()
                    .skip(winner + 1)
                    .for_each(|result| *result = None);
            }
            results
        });
        assert!(saw_batch);
        assert_eq!(
            result
                .kept()
                .iter()
                .map(ReductionUnit::id)
                .collect::<Vec<_>>(),
            vec![3]
        );
    }
}
