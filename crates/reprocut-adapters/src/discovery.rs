use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use reprocut_workspace::InventoryPolicy;
use serde_json::Value;
use thiserror::Error;

/// Supported project command and structured-reducer families.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Ecosystem {
    /// Cargo workspace or package.
    Cargo,
    /// Pytest-compatible Python project.
    Python,
    /// npm package with a test script.
    Npm,
    /// Explicit command only; no manifest reducer.
    None,
}

/// User override or deterministic automatic selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EcosystemSelection {
    /// Require exactly one detected ecosystem.
    Auto,
    /// Select a specific adapter even when other markers coexist.
    Explicit(Ecosystem),
}

/// Program and arguments selected without invoking a shell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterCommand {
    program: OsString,
    arguments: Vec<OsString>,
}

impl AdapterCommand {
    /// Creates a shell-free process specification.
    pub fn new(program: impl Into<OsString>, arguments: Vec<OsString>) -> Self {
        Self {
            program: program.into(),
            arguments,
        }
    }

    /// Returns the executable name.
    pub fn program(&self) -> &std::ffi::OsStr {
        &self.program
    }

    /// Returns the exact child argument vector.
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}

/// One selected ecosystem and its fail-closed operational policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Adapter {
    ecosystem: Ecosystem,
    command: Option<AdapterCommand>,
    inventory_policy: InventoryPolicy,
}

impl Adapter {
    /// Detects markers without executing project code or following links.
    pub fn detect(root: &Path, selection: EcosystemSelection) -> Result<Self, AdapterError> {
        if !root.is_dir() {
            return Err(AdapterError::InvalidRoot(root.to_path_buf()));
        }
        let ecosystem = match selection {
            EcosystemSelection::Explicit(ecosystem) => ecosystem,
            EcosystemSelection::Auto => detect_unique(root)?,
        };
        Self::for_ecosystem(root, ecosystem)
    }

    /// Returns the selected family.
    pub const fn ecosystem(&self) -> Ecosystem {
        self.ecosystem
    }

    /// Returns a zero-configuration command when the adapter defines one.
    pub const fn command(&self) -> Option<&AdapterCommand> {
        self.command.as_ref()
    }

    /// Returns exclusions applied before source inventory allocation.
    pub const fn inventory_policy(&self) -> &InventoryPolicy {
        &self.inventory_policy
    }

    fn for_ecosystem(root: &Path, ecosystem: Ecosystem) -> Result<Self, AdapterError> {
        let (command, inventory_policy) = match ecosystem {
            Ecosystem::Cargo => (
                Some(AdapterCommand::new("cargo", vec![OsString::from("test")])),
                InventoryPolicy::source_only().exclude("target"),
            ),
            Ecosystem::Python => (
                Some(AdapterCommand::new(
                    "python",
                    vec![OsString::from("-m"), OsString::from("pytest")],
                )),
                InventoryPolicy::source_only()
                    .exclude("__pycache__")
                    .exclude(".pytest_cache")
                    .exclude(".mypy_cache")
                    .exclude(".ruff_cache")
                    .exclude(".tox")
                    .exclude(".venv")
                    .exclude("venv"),
            ),
            Ecosystem::Npm => (
                Some(npm_command(root)?),
                InventoryPolicy::source_only()
                    .exclude("node_modules")
                    .exclude("coverage")
                    .exclude("dist")
                    .exclude("build")
                    .exclude(".next")
                    .exclude(".turbo"),
            ),
            Ecosystem::None => (None, InventoryPolicy::source_only()),
        };
        Ok(Self {
            ecosystem,
            command,
            inventory_policy,
        })
    }
}

/// Deterministic discovery or manifest-read failure.
#[derive(Debug, Error)]
pub enum AdapterError {
    /// Root must exist and be a directory.
    #[error("adapter root is not a directory: {0}")]
    InvalidRoot(PathBuf),
    /// Auto mode found no supported marker.
    #[error("no supported ecosystem marker found; pass an explicit command or --ecosystem")]
    NotDetected,
    /// Auto mode refuses to guess between multiple project families.
    #[error("ambiguous ecosystems detected: {0:?}")]
    Ambiguous(Vec<Ecosystem>),
    /// A required manifest could not be read.
    #[error("read adapter manifest {path}: {source}")]
    ReadManifest {
        /// Manifest path.
        path: PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// package.json was not valid JSON.
    #[error("parse package.json: {0}")]
    PackageJson(#[from] serde_json::Error),
    /// npm requires a string-valued scripts.test entry.
    #[error("package.json has no string-valued scripts.test command")]
    MissingNpmTest,
}

fn detect_unique(root: &Path) -> Result<Ecosystem, AdapterError> {
    let mut detected = Vec::with_capacity(3);
    if root.join("Cargo.toml").is_file() {
        detected.push(Ecosystem::Cargo);
    }
    if has_python_marker(root) {
        detected.push(Ecosystem::Python);
    }
    if root.join("package.json").is_file() {
        detected.push(Ecosystem::Npm);
    }
    match detected.as_slice() {
        [] => Err(AdapterError::NotDetected),
        [ecosystem] => Ok(*ecosystem),
        _ => Err(AdapterError::Ambiguous(detected)),
    }
}

fn has_python_marker(root: &Path) -> bool {
    ["pyproject.toml", "pytest.ini", "tox.ini", "setup.cfg"]
        .into_iter()
        .any(|marker| root.join(marker).is_file())
        || fs::read_dir(root).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry.path().is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "py" || extension == "pyi")
            })
        })
}

fn npm_command(root: &Path) -> Result<AdapterCommand, AdapterError> {
    let path = root.join("package.json");
    let bytes = fs::read(&path).map_err(|source| AdapterError::ReadManifest { path, source })?;
    let document: Value = serde_json::from_slice(&bytes)?;
    let test = document
        .pointer("/scripts/test")
        .and_then(Value::as_str)
        .ok_or(AdapterError::MissingNpmTest)?;
    let mut arguments = vec![OsString::from("test")];
    if contains_jest_command(test)
        && !test
            .split_ascii_whitespace()
            .any(|part| part == "--runInBand")
    {
        arguments.extend([OsString::from("--"), OsString::from("--runInBand")]);
    }
    Ok(AdapterCommand::new("npm", arguments))
}

fn contains_jest_command(command: &str) -> bool {
    command
        .split(|character: char| {
            character.is_ascii_whitespace() || matches!(character, '&' | '|' | ';' | '(' | ')')
        })
        .any(|token| {
            token == "jest"
                || token.ends_with("/jest")
                || token.ends_with("\\jest")
                || token.ends_with("/jest.js")
                || token.ends_with("\\jest.js")
        })
}
