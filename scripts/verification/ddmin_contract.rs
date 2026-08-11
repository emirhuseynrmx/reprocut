#[cfg(test)]
mod ddmin_contract {
    use super::reprocut_core::{reduce, CandidateVerdict, ReductionUnit};

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
}
