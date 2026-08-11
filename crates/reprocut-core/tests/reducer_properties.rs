use std::collections::BTreeSet;

use proptest::prelude::*;
use reprocut_core::{reduce, CandidateVerdict, ReductionUnit};

proptest! {
    #[test]
    fn retained_ids_equal_the_required_set(
        universe_len in 1_usize..33,
        raw_required in prop::collection::vec(0_u32..32, 1..16),
    ) {
        let required = raw_required
            .into_iter()
            .map(|id| id % universe_len as u32)
            .collect::<BTreeSet<_>>();
        let units = (0..universe_len as u32)
            .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
            .collect::<Vec<_>>();

        let result = reduce(&units, |kept| {
            let kept_ids = kept.iter().map(|unit| unit.id()).collect::<BTreeSet<_>>();
            if required.is_subset(&kept_ids) {
                CandidateVerdict::Preserved
            } else {
                CandidateVerdict::Rejected
            }
        });
        let actual = result.kept().iter().map(ReductionUnit::id).collect::<BTreeSet<_>>();

        prop_assert_eq!(actual, required);
    }
}
