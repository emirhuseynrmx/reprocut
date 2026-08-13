//! Property tests for reduction minimality and determinism.

use std::collections::BTreeSet;

use proptest::prelude::*;
use reprocut_core::{reduce, CandidateVerdict, ReductionUnit};

proptest! {
    #[test]
    fn retained_ids_equal_the_required_set(
        universe_len in 1_usize..33,
        raw_required in prop::collection::vec(0_u32..32, 1..16),
    ) {
        let universe_len = u32::try_from(universe_len).expect("strategy bounds fit u32");
        let required = raw_required
            .into_iter()
            .map(|id| id % universe_len)
            .collect::<BTreeSet<_>>();
        let units = (0..universe_len)
            .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
            .collect::<Vec<_>>();

        let result = reduce(&units, |kept| {
            let kept_ids = kept
                .iter()
                .copied()
                .map(ReductionUnit::id)
                .collect::<BTreeSet<_>>();
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

#[test]
fn every_required_subset_through_eight_units_is_recovered() {
    for universe_len in 1_u32..=8 {
        let units = (0..universe_len)
            .map(|id| ReductionUnit::new(id, format!("unit-{id}")))
            .collect::<Vec<_>>();
        for required_mask in 1_u32..(1_u32 << universe_len) {
            let result = reduce(&units, |kept| {
                let kept_mask = kept
                    .iter()
                    .fold(0_u32, |mask, unit| mask | (1_u32 << unit.id()));
                if kept_mask & required_mask == required_mask {
                    CandidateVerdict::Preserved
                } else {
                    CandidateVerdict::Rejected
                }
            });
            let actual = result
                .kept()
                .iter()
                .fold(0_u32, |mask, unit| mask | (1_u32 << unit.id()));
            assert_eq!(actual, required_mask);
        }
    }
}
