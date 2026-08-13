use std::fs;

use reprocut_core::{ByteRange, Operation, ProjectPath, Transformation};
use reprocut_workspace::{
    CandidateWorkspace, InventoryPolicy, ProjectInventory, ProjectSnapshot, WorkspaceError,
};

#[test]
fn live_source_mutation_cannot_change_a_frozen_subset() {
    let root = tempfile::tempdir().expect("source");
    fs::write(root.path().join("bug.rs"), b"original bytes").expect("fixture");
    let policy = InventoryPolicy::source_only();
    let inventory = ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
    let snapshot = ProjectSnapshot::capture(&inventory, &policy).expect("capture");

    fs::write(root.path().join("bug.rs"), b"mutated live bytes").expect("mutation");
    let subset = snapshot.subset(inventory.units()).expect("subset");

    assert_eq!(subset.file("bug.rs"), Some(&b"original bytes"[..]));
    assert_eq!(subset.measurements().files(), 1);
    assert_eq!(subset.measurements().bytes(), 14);
    assert_eq!(subset.measurements().lines(), 1);
}

#[test]
fn inventory_membership_drift_fails_closed() {
    let root = tempfile::tempdir().expect("source");
    fs::write(root.path().join("bug.rs"), b"original").expect("fixture");
    let policy = InventoryPolicy::source_only();
    let inventory = ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
    fs::write(root.path().join("late.rs"), b"late").expect("late file");

    let error = ProjectSnapshot::capture(&inventory, &policy).expect_err("membership drift");
    assert!(matches!(error, WorkspaceError::SourceDrift { path, .. } if path == "."));
}

#[test]
fn replacement_and_materialization_preserve_executable_metadata() {
    let root = tempfile::tempdir().expect("source");
    fs::write(root.path().join("tool.sh"), b"#!/bin/sh\nexit 1\n").expect("fixture");
    set_mask(root.path().join("tool.sh"), 0b101);
    let policy = InventoryPolicy::source_only();
    let inventory = ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
    let snapshot = ProjectSnapshot::capture(&inventory, &policy).expect("capture");
    let transformation = Transformation::new(vec![Operation::replace(
        ProjectPath::new("tool.sh").expect("path"),
        ByteRange::new(10, 16).expect("range"),
        b"exit 2".to_vec(),
    )])
    .expect("transformation");
    let transformed = snapshot
        .with_transformation(&transformation)
        .expect("transformed");
    let materialized =
        CandidateWorkspace::materialize_snapshot(&transformed).expect("materialized");

    assert_eq!(
        transformed.files()[0].executable_mask(),
        platform_mask(0b101)
    );
    assert_eq!(
        read_mask(materialized.root().join("tool.sh")),
        platform_mask(0b101)
    );
}

#[cfg(unix)]
fn set_mask(path: std::path::PathBuf, mask: u8) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    let execute = ((u32::from(mask & 0b100) >> 2) * 0o100)
        | ((u32::from(mask & 0b010) >> 1) * 0o010)
        | u32::from(mask & 0b001);
    permissions.set_mode((permissions.mode() & !0o111) | execute);
    fs::set_permissions(path, permissions).expect("permissions");
}

#[cfg(not(unix))]
fn set_mask(_: std::path::PathBuf, _: u8) {}

#[cfg(unix)]
fn read_mask(path: std::path::PathBuf) -> u8 {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path).expect("metadata").permissions().mode();
    (u8::from(mode & 0o100 != 0) << 2)
        | (u8::from(mode & 0o010 != 0) << 1)
        | u8::from(mode & 0o001 != 0)
}

#[cfg(not(unix))]
fn read_mask(_: std::path::PathBuf) -> u8 {
    0
}

const fn platform_mask(mask: u8) -> u8 {
    #[cfg(unix)]
    {
        mask
    }
    #[cfg(not(unix))]
    {
        let _ = mask;
        0
    }
}
