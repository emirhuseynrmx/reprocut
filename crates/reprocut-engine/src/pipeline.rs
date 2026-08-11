use reprocut_adapters::{
    CargoManifest, Ecosystem, ManifestCapability, ManifestError, NpmManifest, PreparationPlan,
    PythonManifest,
};
use reprocut_core::ProjectPath;
use reprocut_syntax::{
    deletion_transforms, hoist_transforms, SyntaxError, SyntaxLanguage, SyntaxStrategy,
};
use reprocut_workspace::{ProjectSnapshot, WorkspaceError};
use thiserror::Error;

use super::PreparationMode;

const CARGO_LOCKS: &[&str] = &["Cargo.lock"];
const NPM_LOCKS: &[&str] = &["package-lock.json", "npm-shrinkwrap.json"];
const NO_CAPTURE: &[&str] = &[];

#[derive(Clone, Debug)]
pub(crate) struct StructuredCandidate {
    key: String,
    snapshot: ProjectSnapshot,
    preparation: Option<PreparationPlan>,
    capture_paths: &'static [&'static str],
}

impl StructuredCandidate {
    /// Returns the canonical human-readable edit identity.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// Returns immutable candidate project contents.
    pub(crate) const fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    /// Returns candidate-specific preparation after global preparation.
    pub(crate) const fn preparation(&self) -> Option<&PreparationPlan> {
        self.preparation.as_ref()
    }

    /// Returns newly generated regular files that may enter the result.
    pub(crate) const fn capture_paths(&self) -> &'static [&'static str] {
        self.capture_paths
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SyntaxPhase {
    /// Delete allowlisted complete named nodes.
    Delete,
    /// Replace wrappers with grammar-valid named children.
    Hoist,
}

/// Generates one-entry manifest cuts in stable semantic-key order.
pub(crate) fn manifest_candidates(
    snapshot: &ProjectSnapshot,
    ecosystem: Ecosystem,
    preparation_mode: PreparationMode,
) -> Result<Vec<StructuredCandidate>, PipelineError> {
    match ecosystem {
        Ecosystem::Cargo => cargo_candidates(snapshot, preparation_mode),
        Ecosystem::Python => python_candidates(snapshot, preparation_mode),
        Ecosystem::Npm => npm_candidates(snapshot, preparation_mode),
        Ecosystem::None => Ok(Vec::new()),
    }
}

/// Generates grammar-validated syntax candidates in stable path/range order.
pub(crate) fn syntax_candidates(
    snapshot: &ProjectSnapshot,
    phase: SyntaxPhase,
) -> Result<Vec<StructuredCandidate>, PipelineError> {
    let mut candidates = Vec::new();
    for file in snapshot.files() {
        let Some(language) = SyntaxLanguage::from_path(std::path::Path::new(file.path())) else {
            continue;
        };
        let transforms = match phase {
            SyntaxPhase::Delete => deletion_transforms(language, file.contents()),
            SyntaxPhase::Hoist => hoist_transforms(language, file.contents()),
        };
        let transforms = match transforms {
            Ok(transforms) => transforms,
            Err(SyntaxError::InvalidSyntax | SyntaxError::InvalidUtf8) => continue,
            Err(error) => return Err(error.into()),
        };
        let path = ProjectPath::new(file.path().to_owned())?;
        for transform in transforms {
            let candidate =
                snapshot.with_transformation(&reprocut_core::Transformation::new(vec![
                    transform.operation(path.clone()),
                ])?)?;
            candidates.push(StructuredCandidate {
                key: format!(
                    "syntax:{}:{}:{}:{}",
                    file.path(),
                    strategy_name(transform.strategy()),
                    transform.range().start(),
                    transform.range().end()
                ),
                snapshot: candidate,
                preparation: None,
                capture_paths: NO_CAPTURE,
            });
        }
    }
    canonicalize(&mut candidates);
    Ok(candidates)
}

fn cargo_candidates(
    snapshot: &ProjectSnapshot,
    preparation_mode: PreparationMode,
) -> Result<Vec<StructuredCandidate>, PipelineError> {
    if preparation_mode == PreparationMode::None {
        return Ok(Vec::new());
    }
    let Some(source) = snapshot.file("Cargo.toml") else {
        return Ok(Vec::new());
    };
    let source = std::str::from_utf8(source).map_err(|_| PipelineError::NonUtf8Manifest)?;
    let manifest = CargoManifest::parse(source)?;
    let mut candidates = Vec::new();
    for entry in manifest.entries() {
        let mut edited = manifest.clone();
        edited.remove(&entry)?;
        candidates.push(StructuredCandidate {
            key: entry.stable_key().to_owned(),
            snapshot: snapshot.with_file_contents("Cargo.toml", edited.render().into_bytes())?,
            preparation: Some(CargoManifest::preparation()),
            capture_paths: CARGO_LOCKS,
        });
    }
    canonicalize(&mut candidates);
    Ok(candidates)
}

fn python_candidates(
    snapshot: &ProjectSnapshot,
    _preparation_mode: PreparationMode,
) -> Result<Vec<StructuredCandidate>, PipelineError> {
    let Some(source) = snapshot.file("pyproject.toml") else {
        return Ok(Vec::new());
    };
    let source = std::str::from_utf8(source).map_err(|_| PipelineError::NonUtf8Manifest)?;
    let manifest = PythonManifest::parse(source)?;
    let mut candidates = Vec::new();
    for entry in manifest.entries() {
        if entry.capability() == ManifestCapability::RequiresIsolatedPython {
            continue;
        }
        let mut edited = manifest.clone();
        edited.remove(&entry)?;
        candidates.push(StructuredCandidate {
            key: entry.stable_key().to_owned(),
            snapshot: snapshot
                .with_file_contents("pyproject.toml", edited.render().into_bytes())?,
            preparation: None,
            capture_paths: NO_CAPTURE,
        });
    }
    canonicalize(&mut candidates);
    Ok(candidates)
}

fn npm_candidates(
    snapshot: &ProjectSnapshot,
    preparation_mode: PreparationMode,
) -> Result<Vec<StructuredCandidate>, PipelineError> {
    let Some(source) = snapshot.file("package.json") else {
        return Ok(Vec::new());
    };
    let source = std::str::from_utf8(source).map_err(|_| PipelineError::NonUtf8Manifest)?;
    let manifest = NpmManifest::parse(source)?;
    let mut candidates = Vec::new();
    for entry in manifest.entries() {
        let permitted = match entry.capability() {
            ManifestCapability::OfflineValidated => preparation_mode != PreparationMode::None,
            ManifestCapability::RequiresLifecycleOptIn => {
                preparation_mode == PreparationMode::LifecycleScripts
            }
            ManifestCapability::RequiresIsolatedPython => false,
        };
        if !permitted {
            continue;
        }
        let mut edited = manifest.clone();
        edited.remove(&entry)?;
        candidates.push(StructuredCandidate {
            key: entry.stable_key().to_owned(),
            snapshot: snapshot.with_file_contents("package.json", edited.render()?.into_bytes())?,
            preparation: None,
            capture_paths: NPM_LOCKS,
        });
    }
    canonicalize(&mut candidates);
    Ok(candidates)
}

fn canonicalize(candidates: &mut Vec<StructuredCandidate>) {
    candidates.sort_unstable_by(|left, right| {
        left.key
            .cmp(&right.key)
            .then_with(|| left.snapshot.digest().cmp(&right.snapshot.digest()))
    });
    candidates.dedup_by(|left, right| left.snapshot.digest() == right.snapshot.digest());
}

const fn strategy_name(strategy: SyntaxStrategy) -> &'static str {
    match strategy {
        SyntaxStrategy::DeleteNode => "delete",
        SyntaxStrategy::HoistChild => "hoist",
    }
}

#[derive(Debug, Error)]
pub(crate) enum PipelineError {
    /// Manifest reducers accept UTF-8 text only.
    #[error("manifest source is not valid UTF-8")]
    NonUtf8Manifest,
    /// Manifest parser or stable-entry failure.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// Grammar configuration or transform failure.
    #[error(transparent)]
    Syntax(#[from] SyntaxError),
    /// Immutable snapshot construction failure.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    /// Canonical operation validation failure.
    #[error(transparent)]
    Transformation(#[from] reprocut_core::TransformationError),
}
