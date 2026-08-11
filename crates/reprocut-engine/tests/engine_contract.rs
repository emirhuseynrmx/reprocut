use std::{
    collections::hash_map::DefaultHasher,
    env,
    ffi::OsString,
    fs,
    hash::{Hash, Hasher},
    path::Path,
    time::Duration,
};

use reprocut_core::ReductionUnit;
use reprocut_engine::{EngineError, ReductionEngine, ReductionRequest};

#[test]
fn real_python_failure_is_stabilized_reduced_and_verified() {
    let source = fixture_copy();
    let before = tree_digest(source.path());
    let request = ReductionRequest::new(
        source.path().to_path_buf(),
        python_executable(),
        vec![OsString::from("bug.py")],
        Duration::from_secs(5),
        64 * 1_024,
    );

    let outcome = ReductionEngine::run(&request).expect("reduction must complete");

    assert_eq!(outcome.baseline_runs(), 3);
    assert_eq!(outcome.final_verifications(), 3);
    assert_eq!(
        outcome
            .reduction()
            .kept()
            .iter()
            .map(ReductionUnit::path)
            .collect::<Vec<_>>(),
        vec!["bug.py"]
    );
    assert_eq!(tree_digest(source.path()), before);
}

#[test]
fn a_successful_baseline_is_not_presented_as_a_failure() {
    let source = tempfile::tempdir().expect("source tempdir");
    fs::write(source.path().join("ok.py"), b"print('ok')").expect("fixture");
    let request = ReductionRequest::new(
        source.path().to_path_buf(),
        python_executable(),
        vec![OsString::from("ok.py")],
        Duration::from_secs(5),
        64 * 1_024,
    );

    let error = ReductionEngine::run(&request).expect_err("successful command is not a failure");

    assert!(matches!(error, EngineError::BaselineSucceeded));
}

fn fixture_copy() -> tempfile::TempDir {
    let source = tempfile::tempdir().expect("source tempdir");
    fs::create_dir(source.path().join("nested")).expect("nested directory");
    fs::copy(fixture_root().join("bug.py"), source.path().join("bug.py")).expect("bug fixture");
    fs::copy(
        fixture_root().join("noise.txt"),
        source.path().join("noise.txt"),
    )
    .expect("noise fixture");
    fs::copy(
        fixture_root().join("nested/unused.txt"),
        source.path().join("nested/unused.txt"),
    )
    .expect("nested fixture");
    source
}

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python_failure")
}

fn python_executable() -> std::path::PathBuf {
    env::var_os("TEST_PYTHON")
        .map(Into::into)
        .unwrap_or_else(|| "python3".into())
}

fn tree_digest(root: &Path) -> u64 {
    let mut files = walk_files(root);
    files.sort();
    let mut hasher = DefaultHasher::new();
    for path in files {
        let relative = path.strip_prefix(root).expect("fixture path");
        relative.hash(&mut hasher);
        fs::read(path).expect("fixture content").hash(&mut hasher);
    }
    hasher.finish()
}

fn walk_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("fixture directory") {
            let path = entry.expect("fixture entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}
