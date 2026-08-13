//! Transformation identity and ordering contracts.

use reprocut_core::{
    ByteRange, ContentDigest, ContentHasher, Operation, ProjectPath, Transformation,
    TransformationError,
};

#[test]
fn operation_order_does_not_change_candidate_digest_or_encoding() {
    let a = Transformation::new(vec![delete("b.py"), delete("a.py")]).expect("valid operations");
    let b = Transformation::new(vec![delete("a.py"), delete("b.py")]).expect("valid operations");

    assert_eq!(a.digest(), b.digest());
    assert_eq!(a.stable_encoding(), b.stable_encoding());
    assert_eq!(a.operations()[0].path().as_str(), "a.py");
}

#[test]
fn overlapping_byte_ranges_are_refused() {
    let path = ProjectPath::new("src/main.rs").expect("safe path");
    let error = Transformation::new(vec![
        Operation::replace(
            path.clone(),
            ByteRange::new(2, 8).expect("range"),
            b"a".to_vec(),
        ),
        Operation::replace(path, ByteRange::new(7, 10).expect("range"), b"b".to_vec()),
    ])
    .expect_err("overlap would make operation order observable");

    assert_eq!(error, TransformationError::OverlappingRanges);
}

#[test]
fn unsafe_project_paths_and_empty_ranges_are_refused() {
    assert!(ProjectPath::new("../secret").is_err());
    assert!(ProjectPath::new("C:/secret").is_err());
    assert!(ByteRange::new(3, 3).is_err());
}

#[test]
fn streaming_digest_matches_one_shot_digest_across_empty_chunks() {
    let payload = b"REPROCUT-SOURCE\0\x05\0\0\0\0\0\0\0hello";
    let mut hasher = ContentHasher::new();
    hasher.update(&payload[..7]);
    hasher.update(&[]);
    hasher.update(&payload[7..19]);
    hasher.update(&payload[19..]);

    assert_eq!(hasher.finalize(), ContentDigest::of(payload));
}

fn delete(path: &str) -> Operation {
    Operation::delete(ProjectPath::new(path).expect("safe path"))
}
