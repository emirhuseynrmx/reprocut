use std::collections::BTreeMap;

use reprocut_core::ReductionUnit;

/// The origin of one ordered hierarchy group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HierarchyGroupKind {
    /// Every regular file below one directory prefix.
    Directory,
    /// One final leaf file.
    File,
}

/// A deterministic group of inventory identifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HierarchyGroup {
    kind: HierarchyGroupKind,
    path: String,
    unit_ids: Vec<u32>,
}

impl HierarchyGroup {
    /// Returns whether the group came from a directory or a leaf file.
    pub const fn kind(&self) -> HierarchyGroupKind {
        self.kind
    }

    /// Returns the slash-separated group path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns sorted stable inventory identifiers.
    pub fn unit_ids(&self) -> &[u32] {
        &self.unit_ids
    }
}

/// A path trie flattened into deterministic directory-first groups.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryHierarchy {
    groups: Vec<HierarchyGroup>,
}

impl DirectoryHierarchy {
    /// Builds directory prefixes without allocating per-node pointer graphs.
    pub fn from_units(units: &[ReductionUnit]) -> Self {
        let mut directories = BTreeMap::<String, Vec<u32>>::new();
        for unit in units {
            let components = unit.path().split('/').collect::<Vec<_>>();
            let mut prefix = String::new();
            for component in components.iter().take(components.len().saturating_sub(1)) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(component);
                directories
                    .entry(prefix.clone())
                    .or_default()
                    .push(unit.id());
            }
        }

        let mut groups = directories
            .into_iter()
            .map(|(path, mut unit_ids)| {
                unit_ids.sort_unstable();
                unit_ids.dedup();
                HierarchyGroup {
                    kind: HierarchyGroupKind::Directory,
                    path,
                    unit_ids,
                }
            })
            .collect::<Vec<_>>();
        groups.sort_by(|left, right| {
            path_depth(&left.path)
                .cmp(&path_depth(&right.path))
                .then_with(|| left.path.cmp(&right.path))
        });
        groups.extend(units.iter().map(|unit| HierarchyGroup {
            kind: HierarchyGroupKind::File,
            path: unit.path().to_owned(),
            unit_ids: vec![unit.id()],
        }));
        Self { groups }
    }

    /// Returns parent directories first and singleton leaves last.
    pub fn groups(&self) -> &[HierarchyGroup] {
        &self.groups
    }

    /// Returns only directory identifiers, ready for hierarchical ddmin.
    pub fn directory_unit_ids(&self) -> Vec<Vec<u32>> {
        self.groups
            .iter()
            .take_while(|group| group.kind == HierarchyGroupKind::Directory)
            .map(|group| group.unit_ids.clone())
            .collect()
    }
}

fn path_depth(path: &str) -> usize {
    path.bytes().filter(|byte| *byte == b'/').count()
}
