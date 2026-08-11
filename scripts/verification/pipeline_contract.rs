#[cfg(test)]
mod pipeline_contract {
    use std::fs;

    use super::reprocut_adapters::Ecosystem;
    use super::reprocut_engine::{pipeline::manifest_candidates, PreparationMode};
    use super::reprocut_workspace::{ProjectInventory, ProjectSnapshot};

    #[test]
    fn cargo_manifest_candidates_are_stable_complete_snapshots() {
        let root = tempfile::tempdir().expect("project");
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n[dependencies]\nserde='1'\nregex='1'\n",
        )
        .expect("manifest");
        let inventory = ProjectInventory::scan(root.path()).expect("inventory");
        let snapshot =
            ProjectSnapshot::from_inventory(&inventory, inventory.units()).expect("snapshot");

        let candidates = manifest_candidates(&snapshot, Ecosystem::Cargo, PreparationMode::Offline)
            .expect("candidates");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].key(), "cargo:dependencies.regex");
        assert_eq!(candidates[1].key(), "cargo:dependencies.serde");
        let first = std::str::from_utf8(
            candidates[0]
                .snapshot()
                .file("Cargo.toml")
                .expect("manifest"),
        )
        .expect("UTF-8");
        assert!(!first.contains("regex"));
        assert!(first.contains("serde"));
        assert!(candidates[0].preparation().is_some());
    }

    #[test]
    fn python_dependencies_require_explicit_caller_isolation() {
        let root = tempfile::tempdir().expect("project");
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname='demo'\ndependencies=['requests']\n[project.scripts]\ndemo='demo:main'\n",
        )
        .expect("manifest");
        let inventory = ProjectInventory::scan(root.path()).expect("inventory");
        let snapshot =
            ProjectSnapshot::from_inventory(&inventory, inventory.units()).expect("snapshot");

        let safe = manifest_candidates(&snapshot, Ecosystem::Python, PreparationMode::Offline)
            .expect("safe candidates");
        let isolated = manifest_candidates(
            &snapshot,
            Ecosystem::Python,
            PreparationMode::IsolatedPython,
        )
        .expect("isolated candidates");

        assert_eq!(safe.len(), 1);
        assert_eq!(safe[0].key(), "python:project.scripts.demo");
        assert_eq!(isolated.len(), 2);
    }
}
