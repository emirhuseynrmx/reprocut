use std::ffi::OsString;

use serde_json::Value as JsonValue;
use thiserror::Error;
use toml_edit::{DocumentMut, Item};

use crate::AdapterCommand;

/// Semantic family shown in reports and structured frontier ranks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestCategory {
    /// Direct, development, build, optional, or peer dependency.
    Dependency,
    /// Cargo feature or Python optional group.
    Feature,
    /// Workspace member or pattern.
    Workspace,
    /// Explicit binary, example, test, or benchmark target.
    Target,
    /// Project command or script.
    Script,
}

/// Preparation guarantee associated with a candidate manifest edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestCapability {
    /// Candidate can be checked by a deterministic offline preparation command.
    OfflineValidated,
    /// Dependency pruning requires a caller-provided isolated Python environment.
    RequiresIsolatedPython,
    /// npm lifecycle execution requires explicit user authorization.
    RequiresLifecycleOptIn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EntryLocator {
    CargoTable {
        prefix: Option<String>,
        table: String,
        key: String,
    },
    CargoArray {
        table: String,
        key: String,
        index: usize,
    },
    CargoArrayOfTables {
        table: String,
        index: usize,
    },
    PythonArray {
        group: Option<String>,
        index: usize,
    },
    PythonScript {
        key: String,
    },
    NpmTable {
        table: String,
        key: String,
    },
    NpmArray {
        key: String,
        index: usize,
    },
}

/// One stable, removable manifest element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    stable_key: String,
    category: ManifestCategory,
    capability: ManifestCapability,
    locator: EntryLocator,
}

impl ManifestEntry {
    /// Returns an ecosystem-qualified deterministic key.
    pub fn stable_key(&self) -> &str {
        &self.stable_key
    }

    /// Returns the semantic manifest family.
    pub const fn category(&self) -> ManifestCategory {
        self.category
    }

    /// Returns the preparation authority required for this edit.
    pub const fn capability(&self) -> ManifestCapability {
        self.capability
    }
}

/// A shell-free, network-disabled candidate preparation plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparationPlan {
    commands: Vec<AdapterCommand>,
    network_allowed: bool,
    lifecycle_scripts_allowed: bool,
}

impl PreparationPlan {
    /// Returns commands in required execution order.
    pub fn commands(&self) -> &[AdapterCommand] {
        &self.commands
    }

    /// Always false for built-in 0.1 preparation plans.
    pub const fn network_allowed(&self) -> bool {
        self.network_allowed
    }

    /// Reports whether package lifecycle scripts may execute.
    pub const fn lifecycle_scripts_allowed(&self) -> bool {
        self.lifecycle_scripts_allowed
    }
}

/// Parsed Cargo.toml with formatting-preserving edits.
#[derive(Clone, Debug)]
pub struct CargoManifest {
    document: DocumentMut,
}

impl CargoManifest {
    /// Parses one Cargo manifest without resolving or executing it.
    pub fn parse(source: &str) -> Result<Self, ManifestError> {
        Ok(Self {
            document: source.parse()?,
        })
    }

    /// Enumerates dependencies, features, members, and explicit targets.
    pub fn entries(&self) -> Vec<ManifestEntry> {
        let mut entries = Vec::new();
        for table in ["dependencies", "dev-dependencies", "build-dependencies"] {
            enumerate_cargo_table(&self.document, None, table, &mut entries);
        }
        if let Some(workspace) = self.document.get("workspace").and_then(Item::as_table_like) {
            if let Some(dependencies) = workspace.get("dependencies").and_then(Item::as_table_like)
            {
                for (key, _) in dependencies.iter() {
                    entries.push(entry(
                        format!("cargo:workspace.dependencies.{key}"),
                        ManifestCategory::Dependency,
                        ManifestCapability::OfflineValidated,
                        EntryLocator::CargoTable {
                            prefix: Some("workspace".to_owned()),
                            table: "dependencies".to_owned(),
                            key: key.to_owned(),
                        },
                    ));
                }
            }
            if let Some(members) = workspace.get("members").and_then(Item::as_array) {
                for (index, value) in members.iter().enumerate() {
                    entries.push(entry(
                        format!("cargo:workspace.members[{index}]:{value}"),
                        ManifestCategory::Workspace,
                        ManifestCapability::OfflineValidated,
                        EntryLocator::CargoArray {
                            table: "workspace".to_owned(),
                            key: "members".to_owned(),
                            index,
                        },
                    ));
                }
            }
        }
        if let Some(features) = self.document.get("features").and_then(Item::as_table_like) {
            for (key, _) in features.iter() {
                entries.push(entry(
                    format!("cargo:features.{key}"),
                    ManifestCategory::Feature,
                    ManifestCapability::OfflineValidated,
                    EntryLocator::CargoTable {
                        prefix: None,
                        table: "features".to_owned(),
                        key: key.to_owned(),
                    },
                ));
            }
        }
        for table in ["bin", "example", "test", "bench"] {
            if let Some(targets) = self.document.get(table).and_then(Item::as_array_of_tables) {
                for (index, target) in targets.iter().enumerate() {
                    let name = target
                        .get("name")
                        .and_then(Item::as_str)
                        .unwrap_or("unnamed");
                    entries.push(entry(
                        format!("cargo:{table}[{index}]:{name}"),
                        ManifestCategory::Target,
                        ManifestCapability::OfflineValidated,
                        EntryLocator::CargoArrayOfTables {
                            table: table.to_owned(),
                            index,
                        },
                    ));
                }
            }
        }
        entries.sort_unstable_by(|left, right| left.stable_key.cmp(&right.stable_key));
        entries
    }

    /// Removes exactly one entry previously enumerated from this document.
    pub fn remove(&mut self, entry: &ManifestEntry) -> Result<(), ManifestError> {
        match &entry.locator {
            EntryLocator::CargoTable { prefix, table, key } => {
                let owner = if let Some(prefix) = prefix {
                    self.document
                        .get_mut(prefix)
                        .and_then(Item::as_table_like_mut)
                        .ok_or(ManifestError::StaleEntry)?
                } else {
                    self.document.as_table_mut()
                };
                owner
                    .get_mut(table)
                    .and_then(Item::as_table_like_mut)
                    .and_then(|values| values.remove(key))
                    .ok_or(ManifestError::StaleEntry)?;
            }
            EntryLocator::CargoArray { table, key, index } => {
                let values = self
                    .document
                    .get_mut(table)
                    .and_then(Item::as_table_like_mut)
                    .and_then(|owner| owner.get_mut(key))
                    .and_then(Item::as_array_mut)
                    .ok_or(ManifestError::StaleEntry)?;
                remove_toml_array(values, *index)?;
            }
            EntryLocator::CargoArrayOfTables { table, index } => {
                let values = self
                    .document
                    .get_mut(table)
                    .and_then(Item::as_array_of_tables_mut)
                    .ok_or(ManifestError::StaleEntry)?;
                if *index >= values.len() {
                    return Err(ManifestError::StaleEntry);
                }
                values.remove(*index);
            }
            _ => return Err(ManifestError::WrongManifestKind),
        }
        Ok(())
    }

    /// Returns formatting-preserving TOML text.
    pub fn render(&self) -> String {
        self.document.to_string()
    }

    /// Returns offline lock regeneration followed by locked metadata validation.
    pub fn preparation() -> PreparationPlan {
        PreparationPlan {
            commands: vec![
                AdapterCommand::new(
                    "cargo",
                    ["generate-lockfile", "--offline"]
                        .into_iter()
                        .map(OsString::from)
                        .collect(),
                ),
                AdapterCommand::new(
                    "cargo",
                    ["metadata", "--locked", "--offline", "--format-version", "1"]
                        .into_iter()
                        .map(OsString::from)
                        .collect(),
                ),
            ],
            network_allowed: false,
            lifecycle_scripts_allowed: false,
        }
    }
}

/// Parsed pyproject.toml with capability-aware edits.
#[derive(Clone, Debug)]
pub struct PythonManifest {
    document: DocumentMut,
}

impl PythonManifest {
    /// Parses pyproject.toml without importing the project.
    pub fn parse(source: &str) -> Result<Self, ManifestError> {
        Ok(Self {
            document: source.parse()?,
        })
    }

    /// Enumerates PEP 621 dependencies, optional groups, and scripts.
    pub fn entries(&self) -> Vec<ManifestEntry> {
        let mut entries = Vec::new();
        let Some(project) = self.document.get("project").and_then(Item::as_table_like) else {
            return entries;
        };
        if let Some(dependencies) = project.get("dependencies").and_then(Item::as_array) {
            for (index, value) in dependencies.iter().enumerate() {
                entries.push(entry(
                    format!("python:project.dependencies[{index}]:{value}"),
                    ManifestCategory::Dependency,
                    ManifestCapability::RequiresIsolatedPython,
                    EntryLocator::PythonArray { group: None, index },
                ));
            }
        }
        if let Some(groups) = project
            .get("optional-dependencies")
            .and_then(Item::as_table_like)
        {
            for (group, item) in groups.iter() {
                if let Some(values) = item.as_array() {
                    for (index, value) in values.iter().enumerate() {
                        entries.push(entry(
                            format!(
                                "python:project.optional-dependencies.{group}[{index}]:{value}"
                            ),
                            ManifestCategory::Dependency,
                            ManifestCapability::RequiresIsolatedPython,
                            EntryLocator::PythonArray {
                                group: Some(group.to_owned()),
                                index,
                            },
                        ));
                    }
                }
            }
        }
        if let Some(scripts) = project.get("scripts").and_then(Item::as_table_like) {
            for (key, _) in scripts.iter() {
                entries.push(entry(
                    format!("python:project.scripts.{key}"),
                    ManifestCategory::Script,
                    ManifestCapability::OfflineValidated,
                    EntryLocator::PythonScript {
                        key: key.to_owned(),
                    },
                ));
            }
        }
        entries.sort_unstable_by(|left, right| left.stable_key.cmp(&right.stable_key));
        entries
    }

    /// Removes one entry; callers must provide isolation for dependency edits.
    pub fn remove(&mut self, entry: &ManifestEntry) -> Result<(), ManifestError> {
        let project = self
            .document
            .get_mut("project")
            .and_then(Item::as_table_like_mut)
            .ok_or(ManifestError::StaleEntry)?;
        match &entry.locator {
            EntryLocator::PythonArray { group, index } => {
                let values = if let Some(group) = group {
                    project
                        .get_mut("optional-dependencies")
                        .and_then(Item::as_table_like_mut)
                        .and_then(|groups| groups.get_mut(group))
                        .and_then(Item::as_array_mut)
                } else {
                    project.get_mut("dependencies").and_then(Item::as_array_mut)
                }
                .ok_or(ManifestError::StaleEntry)?;
                remove_toml_array(values, *index)?;
            }
            EntryLocator::PythonScript { key } => {
                project
                    .get_mut("scripts")
                    .and_then(Item::as_table_like_mut)
                    .and_then(|scripts| scripts.remove(key))
                    .ok_or(ManifestError::StaleEntry)?;
            }
            _ => return Err(ManifestError::WrongManifestKind),
        }
        Ok(())
    }

    /// Returns formatting-preserving TOML text.
    pub fn render(&self) -> String {
        self.document.to_string()
    }
}

/// Parsed package.json with deterministic pretty rendering.
#[derive(Clone, Debug)]
pub struct NpmManifest {
    document: JsonValue,
}

impl NpmManifest {
    /// Parses JSON and requires an object root.
    pub fn parse(source: &str) -> Result<Self, ManifestError> {
        let document: JsonValue = serde_json::from_str(source)?;
        if !document.is_object() {
            return Err(ManifestError::NonObjectPackage);
        }
        Ok(Self { document })
    }

    /// Enumerates dependency maps, scripts, and array-valued workspaces.
    pub fn entries(&self) -> Vec<ManifestEntry> {
        let mut entries = Vec::new();
        for table in [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ] {
            if let Some(values) = self.document.get(table).and_then(JsonValue::as_object) {
                for key in values.keys() {
                    entries.push(entry(
                        format!("npm:{table}.{key}"),
                        ManifestCategory::Dependency,
                        ManifestCapability::OfflineValidated,
                        EntryLocator::NpmTable {
                            table: table.to_owned(),
                            key: key.to_owned(),
                        },
                    ));
                }
            }
        }
        if let Some(scripts) = self.document.get("scripts").and_then(JsonValue::as_object) {
            for key in scripts.keys().filter(|key| key.as_str() != "test") {
                entries.push(entry(
                    format!("npm:scripts.{key}"),
                    ManifestCategory::Script,
                    if is_lifecycle_script(key) {
                        ManifestCapability::RequiresLifecycleOptIn
                    } else {
                        ManifestCapability::OfflineValidated
                    },
                    EntryLocator::NpmTable {
                        table: "scripts".to_owned(),
                        key: key.to_owned(),
                    },
                ));
            }
        }
        if let Some(workspaces) = self
            .document
            .get("workspaces")
            .and_then(JsonValue::as_array)
        {
            for (index, value) in workspaces.iter().enumerate() {
                entries.push(entry(
                    format!("npm:workspaces[{index}]:{value}"),
                    ManifestCategory::Workspace,
                    ManifestCapability::OfflineValidated,
                    EntryLocator::NpmArray {
                        key: "workspaces".to_owned(),
                        index,
                    },
                ));
            }
        }
        entries.sort_unstable_by(|left, right| left.stable_key.cmp(&right.stable_key));
        entries
    }

    /// Removes exactly one JSON entry.
    pub fn remove(&mut self, entry: &ManifestEntry) -> Result<(), ManifestError> {
        match &entry.locator {
            EntryLocator::NpmTable { table, key } => {
                self.document
                    .get_mut(table)
                    .and_then(JsonValue::as_object_mut)
                    .and_then(|values| values.remove(key))
                    .ok_or(ManifestError::StaleEntry)?;
            }
            EntryLocator::NpmArray { key, index } => {
                let values = self
                    .document
                    .get_mut(key)
                    .and_then(JsonValue::as_array_mut)
                    .ok_or(ManifestError::StaleEntry)?;
                if *index >= values.len() {
                    return Err(ManifestError::StaleEntry);
                }
                values.remove(*index);
            }
            _ => return Err(ManifestError::WrongManifestKind),
        }
        Ok(())
    }

    /// Returns canonical pretty JSON with a trailing newline.
    pub fn render(&self) -> Result<String, ManifestError> {
        let mut output = serde_json::to_string_pretty(&self.document)?;
        output.push('\n');
        Ok(output)
    }

    /// Returns offline npm preparation with lifecycle scripts disabled by default.
    pub fn preparation(allow_lifecycle_scripts: bool) -> PreparationPlan {
        let mut arguments = vec![OsString::from("ci"), OsString::from("--offline")];
        if !allow_lifecycle_scripts {
            arguments.push(OsString::from("--ignore-scripts"));
        }
        PreparationPlan {
            commands: vec![AdapterCommand::new("npm", arguments)],
            network_allowed: false,
            lifecycle_scripts_allowed: allow_lifecycle_scripts,
        }
    }
}

/// Manifest parse, stale-key, or kind mismatch.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// Invalid TOML.
    #[error("parse TOML manifest: {0}")]
    Toml(#[from] toml_edit::TomlError),
    /// Invalid JSON.
    #[error("parse JSON manifest: {0}")]
    Json(#[from] serde_json::Error),
    /// package.json must be a JSON object.
    #[error("package.json root must be an object")]
    NonObjectPackage,
    /// Entry belongs to a different manifest family.
    #[error("manifest entry belongs to another ecosystem")]
    WrongManifestKind,
    /// Entry was already removed or the document changed after enumeration.
    #[error("manifest entry is stale")]
    StaleEntry,
}

fn enumerate_cargo_table(
    document: &DocumentMut,
    prefix: Option<&str>,
    table: &str,
    entries: &mut Vec<ManifestEntry>,
) {
    let values = match prefix {
        Some(prefix) => document
            .get(prefix)
            .and_then(Item::as_table_like)
            .and_then(|owner| owner.get(table)),
        None => document.get(table),
    }
    .and_then(Item::as_table_like);
    if let Some(values) = values {
        for (key, _) in values.iter() {
            entries.push(entry(
                prefix.map_or_else(
                    || format!("cargo:{table}.{key}"),
                    |prefix| format!("cargo:{prefix}.{table}.{key}"),
                ),
                ManifestCategory::Dependency,
                ManifestCapability::OfflineValidated,
                EntryLocator::CargoTable {
                    prefix: prefix.map(str::to_owned),
                    table: table.to_owned(),
                    key: key.to_owned(),
                },
            ));
        }
    }
}

fn remove_toml_array(values: &mut toml_edit::Array, index: usize) -> Result<(), ManifestError> {
    if index >= values.len() {
        return Err(ManifestError::StaleEntry);
    }
    values.remove(index);
    Ok(())
}

fn entry(
    stable_key: String,
    category: ManifestCategory,
    capability: ManifestCapability,
    locator: EntryLocator,
) -> ManifestEntry {
    ManifestEntry {
        stable_key,
        category,
        capability,
        locator,
    }
}

fn is_lifecycle_script(key: &str) -> bool {
    matches!(
        key,
        "preinstall" | "install" | "postinstall" | "prepublish" | "prepare" | "postpublish"
    )
}
