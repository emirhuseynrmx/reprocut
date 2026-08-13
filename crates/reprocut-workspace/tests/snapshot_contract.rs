//! Project snapshot capture contracts.

use std::fs;

use reprocut_core::{ByteRange, Operation, ProjectPath, Transformation};
use reprocut_workspace::{CandidateWorkspace, ProjectInventory, ProjectSnapshot, WorkspaceError};

#[test]
fn snapshot_transformations_are_immutable_and_content_addressed() {
    let root = tempfile::tempdir().expect("source");
    fs::create_dir(root.path().join("src")).expect("src");
    fs::write(root.path().join("Cargo.toml"), b"[package]\nname='demo'\n").expect("manifest");
    fs::write(
        root.path().join("src/lib.rs"),
        b"fn keep() {}\nfn drop() {}\n",
    )
    .expect("source");
    let inventory = ProjectInventory::scan(root.path()).expect("inventory");
    let snapshot =
        ProjectSnapshot::from_inventory(&inventory, inventory.units()).expect("snapshot");
    let before = snapshot.digest();
    let operation = Operation::replace(
        ProjectPath::new("src/lib.rs").expect("path"),
        ByteRange::new(13, 26).expect("range"),
        Vec::new(),
    );
    let transformation = Transformation::new(vec![operation]).expect("transformation");

    let reduced = snapshot
        .with_transformation(&transformation)
        .expect("reduced snapshot");

    assert_ne!(reduced.digest(), before);
    assert_eq!(
        snapshot.file("src/lib.rs").expect("original"),
        b"fn keep() {}\nfn drop() {}\n"
    );
    assert_eq!(
        reduced.file("src/lib.rs").expect("reduced"),
        b"fn keep() {}\n"
    );
    assert_eq!(
        reduced.file("Cargo.toml").expect("unchanged"),
        snapshot.file("Cargo.toml").expect("unchanged")
    );
}

#[test]
fn snapshot_materialization_writes_only_snapshot_files() {
    let root = tempfile::tempdir().expect("source");
    fs::write(root.path().join("keep.txt"), b"keep").expect("keep");
    fs::write(root.path().join("drop.txt"), b"drop").expect("drop");
    let inventory = ProjectInventory::scan(root.path()).expect("inventory");
    let kept = inventory
        .units()
        .iter()
        .filter(|unit| unit.path() == "keep.txt")
        .collect::<Vec<_>>();
    let snapshot =
        ProjectSnapshot::from_inventory(&inventory, kept.iter().copied()).expect("snapshot");

    let candidate = CandidateWorkspace::materialize_snapshot(&snapshot).expect("candidate");

    assert_eq!(
        fs::read(candidate.root().join("keep.txt")).expect("kept"),
        b"keep"
    );
    assert!(!candidate.root().join("drop.txt").exists());
}

#[test]
fn preparation_capture_adds_only_named_regular_files() {
    let root = tempfile::tempdir().expect("source");
    fs::write(root.path().join("Cargo.toml"), b"[workspace]\n").expect("manifest");
    let inventory = ProjectInventory::scan(root.path()).expect("inventory");
    let snapshot =
        ProjectSnapshot::from_inventory(&inventory, inventory.units()).expect("snapshot");
    let candidate = CandidateWorkspace::materialize_snapshot(&snapshot).expect("candidate");
    fs::write(candidate.root().join("Cargo.lock"), b"version = 4\n").expect("lock");
    fs::write(candidate.root().join("untrusted.tmp"), b"do not capture").expect("noise");

    let prepared = snapshot
        .capture_prepared(candidate.root(), &["Cargo.lock"])
        .expect("prepared snapshot");

    assert_eq!(prepared.file("Cargo.lock").expect("lock"), b"version = 4\n");
    assert!(prepared.file("untrusted.tmp").is_none());
}

#[test]
fn preparation_capture_fails_closed_when_a_required_file_disappears() {
    let root = tempfile::tempdir().expect("source");
    fs::write(root.path().join("required.rs"), b"fn required() {}\n").expect("source file");
    let inventory = ProjectInventory::scan(root.path()).expect("inventory");
    let snapshot =
        ProjectSnapshot::from_inventory(&inventory, inventory.units()).expect("snapshot");
    let candidate = CandidateWorkspace::materialize_snapshot(&snapshot).expect("candidate");
    fs::remove_file(candidate.root().join("required.rs")).expect("remove required file");

    let error = snapshot
        .capture_prepared(candidate.root(), &[])
        .expect_err("missing required file must not be silently omitted");

    assert!(matches!(error, WorkspaceError::Io { .. }));
}
