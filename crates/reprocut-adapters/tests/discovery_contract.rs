use std::{ffi::OsStr, fs};

use reprocut_adapters::{Adapter, AdapterError, Ecosystem, EcosystemSelection};

#[test]
fn unique_markers_select_shell_free_default_commands() {
    let cases = [
        (
            "Cargo.toml",
            "[package]\nname='x'\nversion='0.1.0'",
            Ecosystem::Cargo,
            "cargo",
        ),
        (
            "pyproject.toml",
            "[project]\nname='x'",
            Ecosystem::Python,
            "python",
        ),
        (
            "package.json",
            r#"{"scripts":{"test":"jest"}}"#,
            Ecosystem::Npm,
            "npm",
        ),
    ];
    for (marker, contents, ecosystem, program) in cases {
        let root = tempfile::tempdir().expect("project");
        fs::write(root.path().join(marker), contents).expect("marker");
        let adapter = Adapter::detect(root.path(), EcosystemSelection::Auto).expect("detect");
        assert_eq!(adapter.ecosystem(), ecosystem);
        assert_eq!(
            adapter.command().expect("command").program(),
            OsStr::new(program)
        );
    }
}

#[test]
fn ambiguity_is_reported_in_stable_ecosystem_order() {
    let root = tempfile::tempdir().expect("project");
    fs::write(root.path().join("Cargo.toml"), "").expect("cargo marker");
    fs::write(root.path().join("pyproject.toml"), "").expect("python marker");
    let error = Adapter::detect(root.path(), EcosystemSelection::Auto).expect_err("ambiguous");
    assert!(matches!(
        error,
        AdapterError::Ambiguous(found) if found == [Ecosystem::Cargo, Ecosystem::Python]
    ));
}

#[test]
fn explicit_selection_resolves_polyglot_roots_without_guessing() {
    let root = tempfile::tempdir().expect("project");
    fs::write(root.path().join("Cargo.toml"), "").expect("cargo marker");
    fs::write(root.path().join("pyproject.toml"), "").expect("python marker");
    let adapter = Adapter::detect(root.path(), EcosystemSelection::Explicit(Ecosystem::Python))
        .expect("explicit selection");
    assert_eq!(adapter.ecosystem(), Ecosystem::Python);
}

#[test]
fn npm_adds_run_in_band_only_for_an_exact_jest_command_token() {
    let root = tempfile::tempdir().expect("project");
    fs::write(
        root.path().join("package.json"),
        r#"{"scripts":{"test":"node pre.js && jest"}}"#,
    )
    .expect("package");
    let adapter = Adapter::detect(root.path(), EcosystemSelection::Auto).expect("npm");
    assert_eq!(
        adapter.command().expect("command").arguments(),
        ["test", "--", "--runInBand"]
    );

    fs::write(
        root.path().join("package.json"),
        r#"{"scripts":{"test":"node jest-helper.js"}}"#,
    )
    .expect("package");
    let adapter = Adapter::detect(root.path(), EcosystemSelection::Auto).expect("npm");
    assert_eq!(adapter.command().expect("command").arguments(), ["test"]);
}
