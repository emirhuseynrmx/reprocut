use std::fs;

use reprocut_core::ReductionUnit;
use reprocut_workspace::{CandidateWorkspace, ProjectInventory, WorkspaceError};

#[test]
fn inventory_is_sorted_and_excludes_internal_metadata() {
    let source = tempfile::tempdir().expect("source tempdir");
    fs::create_dir_all(source.path().join("nested")).expect("nested directory");
    fs::create_dir_all(source.path().join(".git")).expect("git directory");
    fs::write(source.path().join("z.txt"), b"z").expect("z fixture");
    fs::write(source.path().join("nested/a.txt"), b"a").expect("a fixture");
    fs::write(source.path().join(".git/config"), b"secret").expect("git fixture");

    let inventory = ProjectInventory::scan(source.path()).expect("inventory");
    let paths = inventory
        .units()
        .iter()
        .map(ReductionUnit::path)
        .collect::<Vec<_>>();

    assert_eq!(paths, vec!["nested/a.txt", "z.txt"]);
}

#[test]
fn removal_changes_only_the_disposable_candidate() {
    let source = tempfile::tempdir().expect("source tempdir");
    fs::write(source.path().join("bug.py"), b"raise TypeError('currency')")
        .expect("bug fixture");
    fs::write(source.path().join("noise.txt"), b"noise").expect("noise fixture");
    let inventory = ProjectInventory::scan(source.path()).expect("inventory");
    let kept = inventory.units().iter().collect::<Vec<_>>();
    let candidate =
        CandidateWorkspace::materialize(&inventory, &kept).expect("candidate workspace");
    let noise = inventory
        .units()
        .iter()
        .find(|unit| unit.path() == "noise.txt")
        .expect("noise unit");

    candidate.remove_units(&[noise]).expect("candidate removal");

    assert!(!candidate.root().join("noise.txt").exists());
    assert_eq!(
        fs::read(source.path().join("noise.txt")).expect("source remains readable"),
        b"noise"
    );
}

#[test]
fn parent_directory_escape_is_rejected() {
    let source = tempfile::tempdir().expect("source tempdir");
    fs::write(source.path().join("bug.py"), b"bug").expect("bug fixture");
    let inventory = ProjectInventory::scan(source.path()).expect("inventory");
    let kept = inventory.units().iter().collect::<Vec<_>>();
    let candidate =
        CandidateWorkspace::materialize(&inventory, &kept).expect("candidate workspace");
    let escape = ReductionUnit::new(999, "../outside.txt".into());

    let error = candidate
        .remove_units(&[&escape])
        .expect_err("parent path must be rejected");

    assert!(matches!(error, WorkspaceError::UnsafeRelativePath { .. }));
}
