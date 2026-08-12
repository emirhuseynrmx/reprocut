use std::{env, ffi::OsString, fs, path::Path, time::Duration};

use reprocut_engine::{PythonIsolationRequest, ReductionEngine, ReductionRequest};

#[test]
fn frozen_wheelhouse_is_used_for_every_python_reduction_phase() {
    let source = tempfile::tempdir().expect("source");
    copy_tree(&fixture_root().join("project"), source.path());
    let original_manifest = fs::read(source.path().join("pyproject.toml")).expect("manifest");
    let isolation = PythonIsolationRequest::new(python_executable(), fixture_root().join("wheels"));
    let request = ReductionRequest::new(
        source.path().to_path_buf(),
        "python".into(),
        vec![OsString::from("tests/test_failure.py")],
        Duration::from_secs(30),
        64 * 1_024,
    )
    .with_python_isolation(isolation);

    let outcome = ReductionEngine::run(&request).expect("isolated reduction");
    let manifest = String::from_utf8_lossy(
        outcome
            .snapshot()
            .file("pyproject.toml")
            .expect("retained manifest"),
    );

    assert!(manifest.contains("required-dep"));
    assert!(!manifest.contains("unused-dep"));
    assert!(outcome
        .accepted_structured_edits()
        .iter()
        .any(|key| key.contains("unused-dep")));
    assert_eq!(
        fs::read(source.path().join("pyproject.toml")).expect("original"),
        original_manifest
    );
}

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/python_isolation")
}

fn python_executable() -> std::path::PathBuf {
    let requested: std::path::PathBuf = env::var_os("TEST_PYTHON")
        .map(Into::into)
        .unwrap_or_else(|| "python3".into());
    if requested.is_absolute() {
        return requested;
    }
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let candidate = directory.join(&requested);
        if candidate.is_file() {
            return candidate;
        }
        #[cfg(windows)]
        {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    panic!("TEST_PYTHON could not be resolved to an explicit interpreter")
}

fn copy_tree(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).expect("fixture directory") {
        let entry = entry.expect("fixture entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            fs::create_dir_all(&target).expect("target directory");
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("fixture file");
        }
    }
}
