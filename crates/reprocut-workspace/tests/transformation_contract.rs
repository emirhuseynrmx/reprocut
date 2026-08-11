use std::fs;

use reprocut_core::{ByteRange, Operation, ProjectPath, Transformation};
use reprocut_workspace::{CandidateWorkspace, ProjectInventory};

#[test]
fn replacements_apply_from_high_offsets_without_shifting_lower_ranges() {
    let source = tempfile::tempdir().expect("source");
    fs::write(source.path().join("sample.txt"), b"0123456789").expect("fixture");
    let before = fs::read(source.path().join("sample.txt")).expect("original");
    let inventory = ProjectInventory::scan(source.path()).expect("inventory");
    let path = ProjectPath::new("sample.txt").expect("path");
    let transformation = Transformation::new(vec![
        Operation::replace(
            path.clone(),
            ByteRange::new(1, 3).expect("range"),
            b"A".to_vec(),
        ),
        Operation::replace(path, ByteRange::new(7, 9).expect("range"), b"LONG".to_vec()),
    ])
    .expect("transformation");

    let candidate = CandidateWorkspace::materialize_transformation(&inventory, &transformation)
        .expect("candidate");

    assert_eq!(
        fs::read(candidate.root().join("sample.txt")).expect("candidate bytes"),
        b"0A3456LONG9"
    );
    assert_eq!(
        fs::read(source.path().join("sample.txt")).expect("source bytes"),
        before,
        "candidate materialization must not mutate source"
    );
}

#[test]
fn whole_file_delete_removes_only_the_candidate_copy() {
    let source = tempfile::tempdir().expect("source");
    fs::write(source.path().join("delete.me"), b"owned").expect("fixture");
    let inventory = ProjectInventory::scan(source.path()).expect("inventory");
    let transformation = Transformation::new(vec![Operation::delete(
        ProjectPath::new("delete.me").expect("path"),
    )])
    .expect("transformation");

    let candidate = CandidateWorkspace::materialize_transformation(&inventory, &transformation)
        .expect("candidate");

    assert!(!candidate.root().join("delete.me").exists());
    assert!(source.path().join("delete.me").exists());
}
