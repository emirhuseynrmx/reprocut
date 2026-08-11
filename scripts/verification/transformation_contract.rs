#[cfg(test)]
mod transformation_contract {
    use super::reprocut_core::{
        ByteRange, Operation, ProjectPath, Transformation, TransformationError,
    };

    #[test]
    fn canonical_identity_is_permutation_independent() {
        let a = Transformation::new(vec![delete("b.py"), delete("a.py")]).expect("valid");
        let b = Transformation::new(vec![delete("a.py"), delete("b.py")]).expect("valid");
        assert_eq!(a.digest(), b.digest());
        assert_eq!(a.stable_encoding(), b.stable_encoding());
    }

    #[test]
    fn conflicting_ranges_fail_closed() {
        let path = ProjectPath::new("src/lib.rs").expect("path");
        let result = Transformation::new(vec![
            Operation::replace(
                path.clone(),
                ByteRange::new(0, 4).expect("range"),
                Vec::new(),
            ),
            Operation::replace(path, ByteRange::new(3, 9).expect("range"), Vec::new()),
        ]);
        assert_eq!(result, Err(TransformationError::OverlappingRanges));
    }

    fn delete(path: &str) -> Operation {
        Operation::delete(ProjectPath::new(path).expect("safe path"))
    }
}
