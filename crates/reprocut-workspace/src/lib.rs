//! Disposable filesystem workspaces for ReproCut candidates.

mod hierarchy;

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Component, Path, PathBuf},
};

use reprocut_core::{Operation, ReductionUnit, Transformation};
use tempfile::TempDir;
use thiserror::Error;
use walkdir::WalkDir;

pub use hierarchy::{DirectoryHierarchy, HierarchyGroup, HierarchyGroupKind};

/// Exact directory-basename exclusions applied at every depth before traversal descends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryPolicy {
    excluded_directory_names: BTreeSet<String>,
}

impl InventoryPolicy {
    /// Creates the source-control and ReproCut safety baseline.
    pub fn source_only() -> Self {
        Self {
            excluded_directory_names: [".git", ".reprocut", "reprocut-output"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Adds an exact directory basename exclusion matched at any nested depth.
    pub fn exclude(mut self, name: impl Into<String>) -> Self {
        self.excluded_directory_names.insert(name.into());
        self
    }

    /// Reports whether traversal may enter this directory basename.
    pub fn excludes(&self, name: &str) -> bool {
        self.excluded_directory_names.contains(name)
    }
}

impl Default for InventoryPolicy {
    fn default() -> Self {
        Self::source_only()
    }
}

/// A workspace inventory or materialization failure.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    /// The supplied source root was not a directory.
    #[error("project root is not a directory: {path}")]
    InvalidRoot {
        /// Invalid root path.
        path: PathBuf,
    },
    /// A filesystem operation failed.
    #[error("{operation} failed for {path}: {source}")]
    Io {
        /// Operation being attempted.
        operation: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Operating-system error.
        #[source]
        source: io::Error,
    },
    /// Directory traversal failed.
    #[error("project traversal failed: {0}")]
    Walk(#[from] walkdir::Error),
    /// A path was absolute, parent-relative, or otherwise unsafe to mutate.
    #[error("unsafe project-relative path: {path}")]
    UnsafeRelativePath {
        /// Rejected path text.
        path: String,
    },
    /// The inventory cannot be represented by stable 32-bit identifiers.
    #[error("project contains more than {0} files")]
    TooManyFiles(usize),
    /// A transformation referenced a file that is not present in the snapshot.
    #[error("transformation target is not a regular file: {path}")]
    MissingTransformationTarget {
        /// Missing project-relative target.
        path: String,
    },
    /// A byte operation exceeded its immutable source file.
    #[error("transformation range is outside source bytes: {path}")]
    RangeOutOfBounds {
        /// Project-relative target with invalid offsets.
        path: String,
    },
}

/// A sorted, immutable view of removable project files.
#[derive(Clone, Debug)]
pub struct ProjectInventory {
    root: PathBuf,
    units: Vec<ReductionUnit>,
}

impl ProjectInventory {
    /// Scans regular files without following symbolic links.
    pub fn scan(root: &Path) -> Result<Self, WorkspaceError> {
        Self::scan_with_policy(root, &InventoryPolicy::default())
    }

    /// Scans regular files while pruning generated/cache directories at traversal time.
    pub fn scan_with_policy(root: &Path, policy: &InventoryPolicy) -> Result<Self, WorkspaceError> {
        if !root.is_dir() {
            return Err(WorkspaceError::InvalidRoot {
                path: root.to_path_buf(),
            });
        }
        let root = root.canonicalize().map_err(|source| WorkspaceError::Io {
            operation: "canonicalize project root",
            path: root.to_path_buf(),
            source,
        })?;
        let mut paths = Vec::new();

        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !is_excluded_directory(entry.path(), entry.depth(), policy))
        {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&root)
                .expect("walkdir entries remain beneath their root");
            paths.push(display_relative(relative)?);
        }

        paths.sort_unstable();
        let units = paths
            .into_iter()
            .enumerate()
            .map(|(id, path)| {
                let stable_id =
                    u32::try_from(id).map_err(|_| WorkspaceError::TooManyFiles(id + 1))?;
                Ok(ReductionUnit::new(stable_id, path))
            })
            .collect::<Result<Vec<_>, WorkspaceError>>()?;

        Ok(Self { root, units })
    }

    /// Returns the canonical source root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns sorted file units.
    pub fn units(&self) -> &[ReductionUnit] {
        &self.units
    }

    /// Copies exactly the selected units below a destination directory.
    pub fn copy_units_to(
        &self,
        units: &[&ReductionUnit],
        destination_root: &Path,
    ) -> Result<(), WorkspaceError> {
        fs::create_dir_all(destination_root).map_err(|source| WorkspaceError::Io {
            operation: "create publication root",
            path: destination_root.to_path_buf(),
            source,
        })?;

        for unit in units {
            let relative = safe_relative(unit.path())?;
            let source_path = self.root.join(&relative);
            let destination = destination_root.join(&relative);
            copy_regular_file(&source_path, &destination)?;
        }
        Ok(())
    }
}

/// An automatically cleaned candidate project directory.
#[derive(Debug)]
pub struct CandidateWorkspace {
    _temp_dir: TempDir,
    root: PathBuf,
}

impl CandidateWorkspace {
    /// Copies exactly the retained regular files into a disposable project.
    pub fn materialize(
        inventory: &ProjectInventory,
        kept: &[&ReductionUnit],
    ) -> Result<Self, WorkspaceError> {
        let temp_dir = tempfile::Builder::new()
            .prefix("reprocut-candidate-")
            .tempdir()
            .map_err(|source| WorkspaceError::Io {
                operation: "create candidate directory",
                path: std::env::temp_dir(),
                source,
            })?;
        let root = temp_dir.path().join("project");
        fs::create_dir(&root).map_err(|source| WorkspaceError::Io {
            operation: "create candidate root",
            path: root.clone(),
            source,
        })?;

        inventory.copy_units_to(kept, &root)?;

        Ok(Self {
            _temp_dir: temp_dir,
            root,
        })
    }

    /// Copies the full snapshot and applies one canonical transformation.
    pub fn materialize_transformation(
        inventory: &ProjectInventory,
        transformation: &Transformation,
    ) -> Result<Self, WorkspaceError> {
        let all = inventory.units().iter().collect::<Vec<_>>();
        let candidate = Self::materialize(inventory, &all)?;
        candidate.apply(transformation)?;
        Ok(candidate)
    }

    /// Returns the disposable project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Removes selected files only from this candidate.
    pub fn remove_units(&self, removed: &[&ReductionUnit]) -> Result<(), WorkspaceError> {
        for unit in removed {
            let relative = safe_relative(unit.path())?;
            let target = self.root.join(relative);
            if target.is_file() {
                fs::remove_file(&target).map_err(|source| WorkspaceError::Io {
                    operation: "remove candidate file",
                    path: target,
                    source,
                })?;
            }
        }
        Ok(())
    }

    fn apply(&self, transformation: &Transformation) -> Result<(), WorkspaceError> {
        let operations = transformation.operations();
        let mut start = 0_usize;
        while start < operations.len() {
            let path = operations[start].path();
            let mut end = start + 1;
            while end < operations.len() && operations[end].path() == path {
                end += 1;
            }
            let relative = safe_relative(path.as_str())?;
            let target = self.root.join(relative);
            if !target.is_file() {
                return Err(WorkspaceError::MissingTransformationTarget {
                    path: path.as_str().to_owned(),
                });
            }
            match &operations[start] {
                Operation::DeleteFile { .. } => {
                    fs::remove_file(&target).map_err(|source| WorkspaceError::Io {
                        operation: "delete transformed file",
                        path: target,
                        source,
                    })?;
                }
                Operation::ReplaceRange { .. } => {
                    apply_replacements(&target, &operations[start..end], path.as_str())?;
                }
            }
            start = end;
        }
        Ok(())
    }
}

fn apply_replacements(
    target: &Path,
    operations: &[Operation],
    display_path: &str,
) -> Result<(), WorkspaceError> {
    let source = fs::read(target).map_err(|source| WorkspaceError::Io {
        operation: "read transformation target",
        path: target.to_path_buf(),
        source,
    })?;
    let mut final_length = source.len();
    for operation in operations {
        let Operation::ReplaceRange {
            range, replacement, ..
        } = operation
        else {
            unreachable!("canonical validation prevents mixed delete and replace operations");
        };
        let start =
            usize::try_from(range.start()).map_err(|_| WorkspaceError::RangeOutOfBounds {
                path: display_path.to_owned(),
            })?;
        let end = usize::try_from(range.end()).map_err(|_| WorkspaceError::RangeOutOfBounds {
            path: display_path.to_owned(),
        })?;
        if end > source.len() {
            return Err(WorkspaceError::RangeOutOfBounds {
                path: display_path.to_owned(),
            });
        }
        final_length = final_length
            .checked_sub(end - start)
            .and_then(|length| length.checked_add(replacement.len()))
            .ok_or_else(|| WorkspaceError::RangeOutOfBounds {
                path: display_path.to_owned(),
            })?;
    }
    let mut transformed = Vec::with_capacity(final_length);
    let mut cursor = 0_usize;
    for operation in operations {
        let Operation::ReplaceRange {
            range, replacement, ..
        } = operation
        else {
            unreachable!("canonical validation prevents mixed delete and replace operations");
        };
        let start =
            usize::try_from(range.start()).map_err(|_| WorkspaceError::RangeOutOfBounds {
                path: display_path.to_owned(),
            })?;
        let end = usize::try_from(range.end()).map_err(|_| WorkspaceError::RangeOutOfBounds {
            path: display_path.to_owned(),
        })?;
        transformed.extend_from_slice(&source[cursor..start]);
        transformed.extend_from_slice(replacement);
        cursor = end;
    }
    transformed.extend_from_slice(&source[cursor..]);
    debug_assert_eq!(transformed.len(), final_length);
    fs::write(target, transformed).map_err(|source| WorkspaceError::Io {
        operation: "write transformation target",
        path: target.to_path_buf(),
        source,
    })
}

fn copy_regular_file(source_path: &Path, destination: &Path) -> Result<(), WorkspaceError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
            operation: "create destination parent",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::copy(source_path, destination).map_err(|source| WorkspaceError::Io {
        operation: "copy regular file",
        path: source_path.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn is_excluded_directory(path: &Path, depth: usize, policy: &InventoryPolicy) -> bool {
    depth > 0
        && path.is_dir()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| policy.excludes(name))
}

fn display_relative(relative: &Path) -> Result<String, WorkspaceError> {
    let mut display = String::new();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err(WorkspaceError::UnsafeRelativePath {
                path: relative.display().to_string(),
            });
        };
        let part = part
            .to_str()
            .ok_or_else(|| WorkspaceError::UnsafeRelativePath {
                path: relative.display().to_string(),
            })?;
        if !display.is_empty() {
            display.push('/');
        }
        display.push_str(part);
    }
    if display.is_empty() {
        return Err(WorkspaceError::UnsafeRelativePath {
            path: relative.display().to_string(),
        });
    }
    Ok(display)
}

fn safe_relative(path: &str) -> Result<PathBuf, WorkspaceError> {
    if path.is_empty() || path.contains('\\') {
        return Err(WorkspaceError::UnsafeRelativePath {
            path: path.to_owned(),
        });
    }
    let candidate = Path::new(path);
    if candidate
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(WorkspaceError::UnsafeRelativePath {
            path: path.to_owned(),
        });
    }
    Ok(candidate.to_path_buf())
}
