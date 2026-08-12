#[cfg(test)]
mod snapshot_integrity_contract {
    use std::fs;

    use crate::reprocut_core::{ByteRange, Operation, ProjectPath, Transformation};
    use crate::reprocut_workspace::{
        CandidateWorkspace, InventoryPolicy, ProjectInventory, ProjectSnapshot,
    };

    #[test]
    fn a_subset_uses_only_the_once_captured_bytes() {
        let root = tempfile::tempdir().expect("root");
        fs::create_dir(root.path().join("src")).expect("src");
        fs::write(root.path().join("src/lib.rs"), b"original bytes").expect("source");
        let policy = InventoryPolicy::source_only();
        let inventory =
            ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
        let frozen = ProjectSnapshot::capture(&inventory, &policy).expect("capture");

        fs::write(root.path().join("src/lib.rs"), b"changed live bytes").expect("mutate live");
        let subset = frozen.subset(inventory.units()).expect("subset");

        assert_eq!(subset.file("src/lib.rs"), Some(&b"original bytes"[..]));
        assert_eq!(frozen.measurements().files(), 1);
        assert_eq!(frozen.measurements().bytes(), 14);
    }

    #[test]
    fn replacement_preserves_the_snapshot_executable_mask() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("tool.sh"), b"#!/bin/sh\nexit 1\n").expect("tool");
        set_mask(root.path().join("tool.sh"), 0b101);
        let policy = InventoryPolicy::source_only();
        let inventory =
            ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
        let snapshot = ProjectSnapshot::capture(&inventory, &policy).expect("capture");
        let operation = Operation::replace(
            ProjectPath::new("tool.sh").expect("path"),
            ByteRange::new(10, 16).expect("range"),
            b"exit 2".to_vec(),
        );
        let reduced = snapshot
            .with_transformation(&Transformation::new(vec![operation]).expect("transformation"))
            .expect("reduced");
        let candidate = CandidateWorkspace::materialize_snapshot(&reduced).expect("candidate");

        assert_eq!(reduced.files()[0].executable_mask(), platform_mask(0b101));
        assert_eq!(
            read_mask(candidate.root().join("tool.sh")),
            platform_mask(0b101)
        );
    }

    #[test]
    fn execute_mask_participates_in_snapshot_identity() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("tool.sh"), b"same bytes").expect("tool");
        let policy = InventoryPolicy::source_only();
        let inventory =
            ProjectInventory::scan_with_policy(root.path(), &policy).expect("inventory");
        set_mask(root.path().join("tool.sh"), 0b100);
        let owner = ProjectSnapshot::capture(&inventory, &policy).expect("owner");
        set_mask(root.path().join("tool.sh"), 0b001);
        let other = ProjectSnapshot::capture(&inventory, &policy).expect("other");

        #[cfg(unix)]
        assert_ne!(owner.digest(), other.digest());
        #[cfg(not(unix))]
        assert_eq!(owner.digest(), other.digest());
    }

    #[cfg(unix)]
    fn set_mask(path: std::path::PathBuf, mask: u8) {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        let execute = (u32::from(mask & 0b100) >> 2) * 0o100
            | (u32::from(mask & 0b010) >> 1) * 0o010
            | u32::from(mask & 0b001);
        permissions.set_mode((permissions.mode() & !0o111) | execute);
        fs::set_permissions(path, permissions).expect("permissions");
    }

    #[cfg(not(unix))]
    fn set_mask(_: std::path::PathBuf, _: u8) {}

    #[cfg(unix)]
    fn read_mask(path: std::path::PathBuf) -> u8 {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path).expect("metadata").permissions().mode();
        u8::from(mode & 0o100 != 0) << 2
            | u8::from(mode & 0o010 != 0) << 1
            | u8::from(mode & 0o001 != 0)
    }

    #[cfg(not(unix))]
    fn read_mask(_: std::path::PathBuf) -> u8 {
        0
    }

    const fn platform_mask(mask: u8) -> u8 {
        #[cfg(unix)]
        {
            mask
        }
        #[cfg(not(unix))]
        {
            let _ = mask;
            0
        }
    }
}
