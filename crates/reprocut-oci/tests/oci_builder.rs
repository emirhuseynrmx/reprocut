//! Real OCI builder and archive-export contracts.

use std::fs;

use reprocut_oci::{export_archive, Builder, OciRequest, RuntimeFamily};

#[test]
#[ignore = "requires Docker Buildx and a cached debian:bookworm-slim base"]
fn docker_buildx_emits_a_valid_oci_archive() {
    let artifact = tempfile::tempdir().expect("artifact");
    fs::create_dir(artifact.path().join("project")).expect("project");
    fs::write(artifact.path().join("project/README"), b"OCI fixture\n").expect("fixture");
    let output = artifact.path().join("reproduction.oci.tar");
    let request = OciRequest::new(
        artifact.path().to_path_buf(),
        output.clone(),
        RuntimeFamily::Generic,
        vec!["/bin/true".to_owned()],
        "0123456789abcdef".repeat(4),
        "fedcba9876543210".repeat(4),
    )
    .with_builder(Builder::DockerBuildx);

    assert_eq!(
        export_archive(&request).expect("real OCI export"),
        Builder::DockerBuildx
    );
    assert!(fs::metadata(output).expect("archive metadata").len() > 1_024);
}
