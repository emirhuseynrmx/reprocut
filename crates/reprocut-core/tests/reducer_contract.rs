use reprocut_core::{reduce, CandidateVerdict, ReductionUnit};

#[test]
fn removes_every_unit_not_required_for_failure() {
    let units = ["a", "bug.py", "b", "c"]
        .into_iter()
        .enumerate()
        .map(|(id, path)| ReductionUnit::new(id as u32, path.into()))
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
        .map(|(id, path)| ReductionUnit::new(id as u32, path.into()))
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

fn required_pair_fixture() -> Vec<ReductionUnit> {
    ["noise-a", "left", "noise-b", "right", "noise-c"]
        .into_iter()
        .enumerate()
        .map(|(id, path)| ReductionUnit::new(id as u32, path.into()))
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
