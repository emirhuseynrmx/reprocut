#[cfg(test)]
mod adapter_contract {
    use std::{ffi::OsStr, fs};

    use crate::reprocut_adapters::{Adapter, AdapterError, Ecosystem, EcosystemSelection};
    use crate::reprocut_workspace::ProjectInventory;

    #[test]
    fn ambiguity_explicit_selection_and_inventory_pruning_are_deterministic() {
        let root = tempfile::tempdir().expect("project");
        fs::write(root.path().join("Cargo.toml"), "").expect("cargo marker");
        fs::write(root.path().join("pyproject.toml"), "").expect("python marker");
        assert!(matches!(
            Adapter::detect(root.path(), EcosystemSelection::Auto),
            Err(AdapterError::Ambiguous(found))
                if found == [Ecosystem::Cargo, Ecosystem::Python]
        ));

        fs::create_dir(root.path().join("__pycache__")).expect("cache");
        fs::write(root.path().join("__pycache__/noise.pyc"), [0_u8; 4]).expect("noise");
        fs::write(root.path().join("source.py"), "pass").expect("source");
        let adapter = Adapter::detect(root.path(), EcosystemSelection::Explicit(Ecosystem::Python))
            .expect("python adapter");
        assert_eq!(
            adapter.command().expect("command").program(),
            OsStr::new("python")
        );
        let inventory = ProjectInventory::scan_with_policy(root.path(), adapter.inventory_policy())
            .expect("inventory");
        assert!(inventory
            .units()
            .iter()
            .all(|unit| !unit.path().contains("__pycache__")));
    }

    #[test]
    fn npm_jest_detection_has_token_boundaries() {
        let root = tempfile::tempdir().expect("project");
        fs::write(
            root.path().join("package.json"),
            r#"{"scripts":{"test":"node prep.js && jest"}}"#,
        )
        .expect("package");
        let adapter = Adapter::detect(root.path(), EcosystemSelection::Auto).expect("npm");
        assert_eq!(
            adapter.command().expect("command").arguments(),
            ["test", "--", "--runInBand"]
        );
    }
}
