//! Minimal OCI build-context contracts.

use std::fs;

use reprocut_oci::{export_archive, prepare_context, OciError, OciRequest, RuntimeFamily};

#[test]
fn context_contains_only_verified_project_and_generated_dockerfile() {
    let artifact = tempfile::tempdir().expect("artifact");
    fs::create_dir(artifact.path().join("project")).expect("project");
    fs::write(
        artifact.path().join("project/bug.py"),
        b"raise ValueError()\n",
    )
    .expect("source");
    fs::write(artifact.path().join("reduction.json"), b"secret state").expect("evidence");
    fs::write(artifact.path().join("credentials.env"), b"TOKEN=secret").expect("credential");
    let request = OciRequest::new(
        artifact.path().to_path_buf(),
        artifact.path().join("repro.oci.tar"),
        RuntimeFamily::Python,
        vec![
            "python".to_owned(),
            "bug.py".to_owned(),
            "two words".to_owned(),
        ],
        "abc123".to_owned(),
    );

    let context = prepare_context(&request).expect("minimal context");
    let dockerfile = fs::read_to_string(context.root().join("Dockerfile")).expect("Dockerfile");

    assert!(context.root().join("project/bug.py").is_file());
    assert!(!context.root().join("reduction.json").exists());
    assert!(!context.root().join("credentials.env").exists());
    assert!(dockerfile.contains("FROM python:3.13-slim"));
    assert!(dockerfile.contains("ENTRYPOINT [\"python\",\"bug.py\",\"two words\"]"));
    assert!(dockerfile.contains("org.reprocut.failure-fingerprint=\"abc123\""));
}

#[test]
fn existing_output_is_rejected_before_builder_detection() {
    let artifact = tempfile::tempdir().expect("artifact");
    fs::create_dir(artifact.path().join("project")).expect("project");
    fs::write(artifact.path().join("project/bug"), b"bug").expect("source");
    let output = artifact.path().join("owned.tar");
    fs::write(&output, b"user data").expect("owned output");
    let request = OciRequest::new(
        artifact.path().to_path_buf(),
        output.clone(),
        RuntimeFamily::Generic,
        vec!["./bug".to_owned()],
        "abc123".to_owned(),
    );

    let error = export_archive(&request).expect_err("no-clobber");
    assert!(matches!(error, OciError::OutputExists(path) if path == output));
    assert_eq!(fs::read(output).expect("owned output"), b"user data");
}

#[cfg(unix)]
#[test]
fn project_symlinks_are_refused_instead_of_followed() {
    use std::os::unix::fs::symlink;

    let artifact = tempfile::tempdir().expect("artifact");
    fs::create_dir(artifact.path().join("project")).expect("project");
    fs::write(artifact.path().join("outside"), b"secret").expect("outside");
    symlink("../outside", artifact.path().join("project/link")).expect("symlink");
    let request = OciRequest::new(
        artifact.path().to_path_buf(),
        artifact.path().join("repro.tar"),
        RuntimeFamily::Generic,
        vec!["./bug".to_owned()],
        "abc123".to_owned(),
    );

    assert!(matches!(
        prepare_context(&request),
        Err(OciError::UnsupportedEntry(_))
    ));
}
