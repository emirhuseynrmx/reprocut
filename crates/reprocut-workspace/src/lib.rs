//! Disposable filesystem workspaces for ReproCut candidates.

mod hierarchy;

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use reprocut_core::{ContentDigest, Operation, ReductionUnit, Transformation};
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

    /// Returns excluded basenames in stable lexical order for session contracts.
    pub fn excluded_directory_names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.excluded_directory_names.iter().map(String::as_str)
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

/// One immutable file in a content-addressed reduced-project snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotFile {
    path: String,
    contents: Arc<[u8]>,
    digest: ContentDigest,
}

impl SnapshotFile {
    /// Returns the normalized project-relative path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns immutable file bytes.
    pub fn contents(&self) -> &[u8] {
        &self.contents
    }

    /// Returns the file-content digest cached when the snapshot was built.
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Sorted immutable project contents with copy-on-write file sharing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSnapshot {
    files: Vec<SnapshotFile>,
    digest: ContentDigest,
}

impl ProjectSnapshot {
    /// Reads selected immutable inventory units into one sorted snapshot.
    pub fn from_inventory<'unit>(
        inventory: &ProjectInventory,
        units: impl IntoIterator<Item = &'unit ReductionUnit>,
    ) -> Result<Self, WorkspaceError> {
        let mut files = units
            .into_iter()
            .map(|unit| {
                let relative = safe_relative(unit.path())?;
                let path = inventory.root.join(relative);
                let contents = fs::read(&path).map_err(|source| WorkspaceError::Io {
                    operation: "read snapshot file",
                    path,
                    source,
                })?;
                Ok(snapshot_file(unit.path().to_owned(), contents))
            })
            .collect::<Result<Vec<_>, WorkspaceError>>()?;
        files.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        files.dedup_by(|left, right| left.path == right.path);
        Ok(Self::from_files(files))
    }

    /// Returns sorted immutable files.
    pub fn files(&self) -> &[SnapshotFile] {
        &self.files
    }

    /// Returns one file by normalized relative path.
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.file_index(path)
            .ok()
            .map(|index| self.files[index].contents())
    }

    /// Returns a digest over paths and cached content digests.
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    /// Returns a new snapshot with one canonical transformation applied.
    pub fn with_transformation(
        &self,
        transformation: &Transformation,
    ) -> Result<Self, WorkspaceError> {
        let mut files = self.files.clone();
        let operations = transformation.operations();
        let mut start = 0_usize;
        while start < operations.len() {
            let target = operations[start].path().as_str();
            let index = files
                .binary_search_by(|file| file.path.as_str().cmp(target))
                .map_err(|_| WorkspaceError::MissingTransformationTarget {
                    path: target.to_owned(),
                })?;
            let mut end = start + 1;
            while end < operations.len() && operations[end].path() == operations[start].path() {
                end += 1;
            }
            match &operations[start] {
                Operation::DeleteFile { .. } => {
                    files.remove(index);
                }
                Operation::ReplaceRange { .. } => {
                    let contents = apply_replacements_to_bytes(
                        files[index].contents(),
                        &operations[start..end],
                        target,
                    )?;
                    files[index] = snapshot_file(target.to_owned(), contents);
                }
            }
            start = end;
        }
        Ok(Self::from_files(files))
    }

    /// Returns a new snapshot replacing all bytes of one existing file.
    pub fn with_file_contents(
        &self,
        path: &str,
        contents: Vec<u8>,
    ) -> Result<Self, WorkspaceError> {
        let index = self.file_index(path)?;
        let mut files = self.files.clone();
        files[index] = snapshot_file(path.to_owned(), contents);
        Ok(Self::from_files(files))
    }

    /// Writes exactly this snapshot below an existing or new destination.
    pub fn copy_to(&self, destination_root: &Path) -> Result<(), WorkspaceError> {
        fs::create_dir_all(destination_root).map_err(|source| WorkspaceError::Io {
            operation: "create snapshot destination",
            path: destination_root.to_path_buf(),
            source,
        })?;
        for file in &self.files {
            let relative = safe_relative(&file.path)?;
            write_regular_file(&destination_root.join(relative), file.contents())?;
        }
        Ok(())
    }

    fn file_index(&self, path: &str) -> Result<usize, WorkspaceError> {
        self.files
            .binary_search_by(|file| file.path.as_str().cmp(path))
            .map_err(|_| WorkspaceError::MissingTransformationTarget {
                path: path.to_owned(),
            })
    }

    fn from_files(files: Vec<SnapshotFile>) -> Self {
        let digest = snapshot_digest(&files);
        Self { files, digest }
    }
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

    /// Materializes an immutable reduced-project snapshot.
    pub fn materialize_snapshot(snapshot: &ProjectSnapshot) -> Result<Self, WorkspaceError> {
        let temp_dir = tempfile::Builder::new()
            .prefix("reprocut-candidate-")
            .tempdir()
            .map_err(|source| WorkspaceError::Io {
                operation: "create candidate directory",
                path: std::env::temp_dir(),
                source,
            })?;
        let root = temp_dir.path().join("project");
        snapshot.copy_to(&root)?;
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
    let transformed = apply_replacements_to_bytes(&source, operations, display_path)?;
    fs::write(target, transformed).map_err(|source| WorkspaceError::Io {
        operation: "write transformation target",
        path: target.to_path_buf(),
        source,
    })
}

fn apply_replacements_to_bytes(
    source: &[u8],
    operations: &[Operation],
    display_path: &str,
) -> Result<Vec<u8>, WorkspaceError> {
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
    Ok(transformed)
}

fn snapshot_file(path: String, contents: Vec<u8>) -> SnapshotFile {
    let digest = ContentDigest::of(&contents);
    SnapshotFile {
        path,
        contents: Arc::from(contents),
        digest,
    }
}

fn snapshot_digest(files: &[SnapshotFile]) -> ContentDigest {
    let mut encoded = Vec::with_capacity(files.len().saturating_mul(48).saturating_add(24));
    encoded.extend_from_slice(b"REPROCUT-SNAPSHOT\0");
    encoded.extend_from_slice(&1_u16.to_le_bytes());
    encoded.extend_from_slice(&u64::try_from(files.len()).unwrap_or(u64::MAX).to_le_bytes());
    for file in files {
        encoded.extend_from_slice(
            &u64::try_from(file.path.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        encoded.extend_from_slice(file.path.as_bytes());
        encoded.extend_from_slice(file.digest.as_bytes());
    }
    ContentDigest::of(&encoded)
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

fn write_regular_file(destination: &Path, contents: &[u8]) -> Result<(), WorkspaceError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
            operation: "create destination parent",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(destination, contents).map_err(|source| WorkspaceError::Io {
        operation: "write snapshot file",
        path: destination.to_path_buf(),
        source,
    })
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
