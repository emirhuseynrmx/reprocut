#[cfg(test)]
mod workspace_transformation_contract {
    use std::fs;

    use super::reprocut_core::{ByteRange, Operation, ProjectPath, Transformation};
    use super::reprocut_workspace::{CandidateWorkspace, ProjectInventory};

    #[test]
    fn canonical_ranges_materialize_without_touching_source() {
        let source = tempfile::tempdir().expect("source");
        let path = source.path().join("sample.txt");
        fs::write(&path, b"0123456789").expect("fixture");
        let inventory = ProjectInventory::scan(source.path()).expect("inventory");
        let project_path = ProjectPath::new("sample.txt").expect("path");
        let transformation = Transformation::new(vec![
            Operation::replace(
                project_path.clone(),
                ByteRange::new(1, 3).expect("range"),
                b"A".to_vec(),
            ),
            Operation::replace(
                project_path,
                ByteRange::new(7, 9).expect("range"),
                b"LONG".to_vec(),
            ),
        ])
        .expect("transformation");

        let candidate = CandidateWorkspace::materialize_transformation(&inventory, &transformation)
            .expect("candidate");
        assert_eq!(
            fs::read(candidate.root().join("sample.txt")).expect("candidate"),
            b"0A3456LONG9"
        );
        assert_eq!(fs::read(path).expect("source"), b"0123456789");
    }
}
