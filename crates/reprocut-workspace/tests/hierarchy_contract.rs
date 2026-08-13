//! Directory-hierarchy reduction contracts.

use reprocut_core::ReductionUnit;
use reprocut_workspace::{DirectoryHierarchy, HierarchyGroupKind};

#[test]
fn directory_groups_precede_sorted_file_leaves() {
    let units = ["src/a.rs", "src/nested/b.rs", "tests/c.rs", "README.md"]
        .into_iter()
        .enumerate()
        .map(|(id, path)| {
            ReductionUnit::new(
                u32::try_from(id).expect("fixture identifier fits u32"),
                path.to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let hierarchy = DirectoryHierarchy::from_units(&units);
    let groups = hierarchy.groups();

    assert_eq!(groups[0].path(), "src");
    assert_eq!(groups[0].unit_ids(), &[0, 1]);
    assert_eq!(groups[1].path(), "tests");
    assert_eq!(groups[2].path(), "src/nested");
    assert!(groups[..3]
        .iter()
        .all(|group| group.kind() == HierarchyGroupKind::Directory));
    assert!(groups[3..]
        .iter()
        .all(|group| group.kind() == HierarchyGroupKind::File));
}
