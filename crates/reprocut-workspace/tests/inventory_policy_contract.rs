use std::fs;

use reprocut_workspace::{InventoryPolicy, ProjectInventory};

#[test]
fn generated_directories_are_pruned_before_inventory() {
    let root = tempfile::tempdir().expect("project");
    fs::create_dir_all(root.path().join("apps/web/src")).expect("normal source directory");
    fs::write(root.path().join("apps/web/src/source.py"), "pass").expect("nested source");
    fs::create_dir_all(root.path().join("apps/web/node_modules/pkg"))
        .expect("nested generated directory");
    fs::write(
        root.path().join("apps/web/node_modules/pkg/noise.js"),
        "generated",
    )
    .expect("nested generated file");
    fs::create_dir_all(root.path().join("packages/core/target/debug"))
        .expect("nested target directory");
    fs::write(
        root.path().join("packages/core/target/debug/noise.bin"),
        [0_u8; 8],
    )
    .expect("target noise");
    fs::create_dir_all(root.path().join("apps/api/__pycache__"))
        .expect("empty generated directory");
    let policy = InventoryPolicy::source_only()
        .exclude("node_modules")
        .exclude("target")
        .exclude("__pycache__");

    let inventory = ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
    assert_eq!(inventory.units().len(), 1);
    assert_eq!(inventory.units()[0].path(), "apps/web/src/source.py");
}

#[test]
fn source_only_has_only_universal_safety_defaults() {
    let policy = InventoryPolicy::source_only();
    for excluded in [".git", ".reprocut", "reprocut-output"] {
        assert!(policy.excludes(excluded));
    }
    for adapter_specific in ["node_modules", "target", "__pycache__", "src"] {
        assert!(!policy.excludes(adapter_specific));
    }
}
