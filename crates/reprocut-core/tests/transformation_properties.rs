use reprocut_core::{Operation, ProjectPath, Transformation};

#[test]
fn every_permutation_of_the_same_deletes_has_one_identity() {
    let paths = ["a", "b", "c", "d"];
    let expected = transformation(&paths).digest();
    for first in 0..paths.len() {
        for second in 0..paths.len() {
            if second == first {
                continue;
            }
            let mut permutation = paths.to_vec();
            permutation.swap(0, first);
            permutation.swap(1, second);
            assert_eq!(transformation(&permutation).digest(), expected);
        }
    }
}

fn transformation(paths: &[&str]) -> Transformation {
    Transformation::new(
        paths
            .iter()
            .map(|path| Operation::delete(ProjectPath::new(*path).expect("safe path")))
            .collect(),
    )
    .expect("non-conflicting operations")
}
