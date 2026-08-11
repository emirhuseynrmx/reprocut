use reprocut_adapters::{CargoManifest, ManifestCapability, NpmManifest, PythonManifest};

#[test]
fn cargo_entries_remove_one_key_and_keep_formatting_context() {
    let source = r#"[package]
name = "fixture"
version = "0.1.0"

[dependencies]
serde = "1"
regex = "1"

[features]
extra = ["regex"]

[workspace]
members = ["child"]

[[bin]]
name = "fixture-cli"
path = "src/main.rs"
"#;
    let mut manifest = CargoManifest::parse(source).expect("cargo manifest");
    let entries = manifest.entries();
    let regex = entries
        .iter()
        .find(|entry| entry.stable_key() == "cargo:dependencies.regex")
        .expect("regex entry");
    manifest.remove(regex).expect("remove dependency");
    let rendered = manifest.render();
    assert!(!rendered.contains("regex = \"1\""));
    assert!(rendered.contains("serde = \"1\""));
    assert_eq!(CargoManifest::preparation().commands().len(), 2);
    assert!(!CargoManifest::preparation().network_allowed());
}

#[test]
fn python_dependency_edits_are_labeled_as_isolation_required() {
    let source = r#"[project]
name = "fixture"
dependencies = ["requests", "numpy"]

[project.optional-dependencies]
test = ["pytest"]

[project.scripts]
fixture = "fixture:main"
"#;
    let mut manifest = PythonManifest::parse(source).expect("pyproject");
    let dependency = manifest
        .entries()
        .into_iter()
        .find(|entry| entry.stable_key().contains("requests"))
        .expect("dependency");
    assert_eq!(
        dependency.capability(),
        ManifestCapability::RequiresIsolatedPython
    );
    manifest.remove(&dependency).expect("remove dependency");
    assert!(!manifest.render().contains("requests"));
}

#[test]
fn npm_keeps_test_script_and_disables_lifecycle_by_default() {
    let source = r#"{
  "scripts": {"test": "jest", "prepare": "node build.js", "lint": "eslint ."},
  "dependencies": {"left-pad": "1.3.0"},
  "workspaces": ["packages/*"]
}"#;
    let mut manifest = NpmManifest::parse(source).expect("package");
    let entries = manifest.entries();
    assert!(entries
        .iter()
        .all(|entry| entry.stable_key() != "npm:scripts.test"));
    let dependency = entries
        .iter()
        .find(|entry| entry.stable_key() == "npm:dependencies.left-pad")
        .expect("dependency");
    manifest.remove(dependency).expect("remove dependency");
    let rendered = manifest.render().expect("render");
    assert!(!rendered.contains("left-pad"));
    let preparation = NpmManifest::preparation(false);
    assert!(!preparation.network_allowed());
    assert!(!preparation.lifecycle_scripts_allowed());
    assert!(preparation.commands()[0]
        .arguments()
        .iter()
        .any(|argument| argument == "--ignore-scripts"));
}
