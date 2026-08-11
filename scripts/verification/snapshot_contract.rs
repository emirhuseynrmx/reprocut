#[cfg(test)]
mod snapshot_contract {
    use std::fs;

    use super::reprocut_core::{ByteRange, Operation, ProjectPath, Transformation};
    use super::reprocut_workspace::{CandidateWorkspace, ProjectInventory, ProjectSnapshot};

    #[test]
    fn immutable_snapshot_applies_and_materializes_one_copy_on_write_edit() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("keep.txt"), b"0123456789").expect("fixture");
        let inventory = ProjectInventory::scan(source.path()).expect("inventory");
        let snapshot =
            ProjectSnapshot::from_inventory(&inventory, inventory.units()).expect("snapshot");
        let transformation = Transformation::new(vec![Operation::replace(
            ProjectPath::new("keep.txt").expect("path"),
            ByteRange::new(2, 8).expect("range"),
            b"X".to_vec(),
        )])
        .expect("transformation");

        let reduced = snapshot
            .with_transformation(&transformation)
            .expect("reduced");
        let candidate = CandidateWorkspace::materialize_snapshot(&reduced).expect("candidate");

        assert_eq!(snapshot.file("keep.txt").expect("original"), b"0123456789");
        assert_eq!(reduced.file("keep.txt").expect("reduced"), b"01X89");
        assert_eq!(
            fs::read(candidate.root().join("keep.txt")).expect("file"),
            b"01X89"
        );
    }

    #[test]
    fn preparation_capture_adds_only_explicit_lockfiles() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("Cargo.toml"), b"[workspace]\n").expect("fixture");
        let inventory = ProjectInventory::scan(source.path()).expect("inventory");
        let snapshot =
            ProjectSnapshot::from_inventory(&inventory, inventory.units()).expect("snapshot");
        let candidate = CandidateWorkspace::materialize_snapshot(&snapshot).expect("candidate");
        fs::write(candidate.root().join("Cargo.lock"), b"version = 4\n").expect("lock");
        fs::write(candidate.root().join("noise.tmp"), b"noise").expect("noise");

        let captured = snapshot
            .capture_prepared(candidate.root(), &["Cargo.lock"])
            .expect("capture");

        assert_eq!(captured.file("Cargo.lock").expect("lock"), b"version = 4\n");
        assert!(captured.file("noise.tmp").is_none());
    }
}
