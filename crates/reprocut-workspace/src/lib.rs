//! Disposable filesystem workspaces for ReproCut candidates.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use reprocut_core::ReductionUnit;
use tempfile::TempDir;
use thiserror::Error;
use walkdir::WalkDir;

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
            .filter_entry(|entry| !is_internal_directory(entry.path(), entry.depth()))
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

        for unit in kept {
            let relative = safe_relative(unit.path())?;
            let source_path = inventory.root.join(&relative);
            let destination = root.join(&relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
                    operation: "create candidate parent",
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            fs::copy(&source_path, &destination).map_err(|source| WorkspaceError::Io {
                operation: "copy candidate file",
                path: source_path,
                source,
            })?;
        }

        Ok(Self {
            _temp_dir: temp_dir,
            root,
        })
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
}

fn is_internal_directory(path: &Path, depth: usize) -> bool {
    depth > 0
        && path.is_dir()
        && matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some(".git" | ".reprocut" | "reprocut-output")
        )
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
