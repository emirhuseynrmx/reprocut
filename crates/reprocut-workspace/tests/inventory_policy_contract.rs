use std::fs;

use reprocut_workspace::{InventoryPolicy, ProjectInventory};

#[test]
fn generated_directories_are_pruned_before_inventory() {
    let root = tempfile::tempdir().expect("project");
    fs::write(root.path().join("source.py"), "pass").expect("source");
    for directory in ["node_modules", "target", "__pycache__"] {
        fs::create_dir(root.path().join(directory)).expect("generated directory");
        fs::write(root.path().join(directory).join("noise.bin"), [0_u8; 8]).expect("noise");
    }
    let policy = InventoryPolicy::source_only()
        .exclude("node_modules")
        .exclude("target")
        .exclude("__pycache__");

    let inventory = ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
    assert_eq!(inventory.units().len(), 1);
    assert_eq!(inventory.units()[0].path(), "source.py");
}
