use std::{
    collections::BTreeSet,
    error::Error,
    fmt, fs, io,
    io::Read as _,
    path::{Path, PathBuf},
};

use reprocut_core::ContentHasher;

use crate::{
    render_issue, render_report, render_reproduction_scripts, write_attempts_jsonl,
    ArtifactManifest, ArtifactMember, ManifestError, ReductionEvidence, ReportModel,
    RetainedEntryKind,
};

const EVIDENCE_PATH: &str = "reduction.json";
const MANIFEST_PATH: &str = "artifact-manifest.json";
const MAX_CONTROL_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 1024 * 1024 * 1024;

/// A structurally verified, content-addressed ReproCut artifact.
///
/// Values can only be constructed by [`verify_artifact`]. Keeping the fields private prevents
/// publication APIs from accidentally accepting an unchecked directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedArtifact {
    root: PathBuf,
    artifact_id: String,
}

impl VerifiedArtifact {
    /// Returns the checked artifact directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the canonical identity of every non-envelope artifact member.
    pub fn artifact_id(&self) -> &str {
        &self.artifact_id
    }
}

/// Artifact construction or verification failure.
#[derive(Debug)]
pub enum VerificationError {
    /// The requested artifact root was absent, symbolic, or not a directory.
    InvalidRoot(PathBuf),
    /// A filesystem operation failed.
    Io {
        /// Operation being attempted.
        operation: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Operating-system failure.
        source: io::Error,
    },
    /// An artifact member is too large for bounded structural verification.
    OversizedMember(PathBuf),
    /// A symbolic link or other unsupported filesystem entry was present.
    UnsupportedEntry(PathBuf),
    /// The artifact manifest was absent, malformed, or internally inconsistent.
    InvalidManifest(String),
    /// Reduction evidence was malformed or violated schema invariants.
    InvalidEvidence(String),
    /// Actual and declared artifact member paths differ.
    MemberSetMismatch,
    /// One declared member's bytes, size, or executable metadata changed.
    MemberMismatch(String),
    /// The retained project and retained evidence manifest disagree.
    RetainedProjectMismatch(String),
    /// The append-only attempt ledger differs from evidence.
    AttemptLedgerMismatch,
    /// The HTML report is not the deterministic rendering of evidence.
    ReportMismatch,
    /// The issue body is not the deterministic rendering of evidence.
    IssueMismatch,
    /// A reproduction launcher is not the deterministic rendering of recorded argv.
    ReproducerMismatch(&'static str),
    /// `reduction.json` is not the canonical serialization of its evidence value.
    NonCanonicalEvidence,
}

impl fmt::Display for VerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot(path) => {
                write!(formatter, "invalid artifact root: {}", path.display())
            }
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "{operation} failed for {}: {source}",
                path.display()
            ),
            Self::OversizedMember(path) => {
                write!(
                    formatter,
                    "artifact member exceeds verification limit: {}",
                    path.display()
                )
            }
            Self::UnsupportedEntry(path) => {
                write!(formatter, "unsupported artifact entry: {}", path.display())
            }
            Self::InvalidManifest(reason) => {
                write!(formatter, "invalid artifact manifest: {reason}")
            }
            Self::InvalidEvidence(reason) => {
                write!(formatter, "invalid reduction evidence: {reason}")
            }
            Self::MemberSetMismatch => {
                formatter.write_str("artifact member set does not match manifest")
            }
            Self::MemberMismatch(path) => write!(formatter, "artifact member changed: {path}"),
            Self::RetainedProjectMismatch(path) => {
                write!(formatter, "retained project changed: {path}")
            }
            Self::AttemptLedgerMismatch => {
                formatter.write_str("attempt ledger disagrees with evidence")
            }
            Self::ReportMismatch => formatter.write_str("HTML report disagrees with evidence"),
            Self::IssueMismatch => formatter.write_str("issue body disagrees with evidence"),
            Self::ReproducerMismatch(path) => {
                write!(
                    formatter,
                    "reproduction launcher disagrees with evidence: {path}"
                )
            }
            Self::NonCanonicalEvidence => {
                formatter.write_str("reduction evidence is not canonically serialized")
            }
        }
    }
}

impl Error for VerificationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ManifestError> for VerificationError {
    fn from(error: ManifestError) -> Self {
        Self::InvalidManifest(error.to_string())
    }
}

/// Builds the non-circular manifest for an already staged artifact directory.
///
/// `artifact-manifest.json` is deliberately excluded so the envelope never hashes itself.
///
/// # Errors
///
/// Returns [`VerificationError`] for unsafe roots, unsupported entries, oversized members,
/// filesystem failures, or an invalid canonical member set.
pub fn build_artifact_manifest(root: &Path) -> Result<ArtifactManifest, VerificationError> {
    validate_root(root)?;
    let members = collect_members(root, false)?;
    ArtifactManifest::new(members).map_err(Into::into)
}

/// Independently checks every declared artifact byte and all derivable evidence contracts.
///
/// # Errors
///
/// Returns a specific [`VerificationError`] when the manifest, member set, retained project,
/// evidence, ledger, report, issue, or reproduction scripts disagree.
pub fn verify_artifact(root: &Path) -> Result<VerifiedArtifact, VerificationError> {
    validate_root(root)?;
    let manifest_bytes = read_control_file(&root.join(MANIFEST_PATH))?;
    let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| VerificationError::InvalidManifest(error.to_string()))?;
    manifest.validate()?;
    if serde_json::to_vec_pretty(&manifest)
        .map_err(|error| VerificationError::InvalidManifest(error.to_string()))?
        != manifest_bytes
    {
        return Err(VerificationError::InvalidManifest(
            "envelope is not canonically serialized".to_owned(),
        ));
    }

    let actual_members = collect_members(root, true)?;
    if actual_members.len() != manifest.members().len()
        || actual_members
            .iter()
            .map(|member| member.path.as_str())
            .ne(manifest.members().iter().map(|member| member.path.as_str()))
    {
        return Err(VerificationError::MemberSetMismatch);
    }
    for (actual, declared) in actual_members.iter().zip(manifest.members()) {
        if actual != declared {
            return Err(VerificationError::MemberMismatch(declared.path.clone()));
        }
    }

    let evidence_bytes = read_control_file(&root.join(EVIDENCE_PATH))?;
    let evidence: ReductionEvidence = serde_json::from_slice(&evidence_bytes)
        .map_err(|error| VerificationError::InvalidEvidence(error.to_string()))?;
    evidence
        .validate()
        .map_err(|reason| VerificationError::InvalidEvidence(reason.to_owned()))?;
    if serde_json::to_vec_pretty(&evidence)
        .map_err(|error| VerificationError::InvalidEvidence(error.to_string()))?
        != evidence_bytes
    {
        return Err(VerificationError::NonCanonicalEvidence);
    }

    verify_expected_member_set(&evidence, manifest.members())?;
    verify_retained_project(&evidence, manifest.members())?;
    verify_derived_files(root, &evidence)?;
    if collect_members(root, true)? != actual_members {
        return Err(VerificationError::MemberSetMismatch);
    }
    if read_control_file(&root.join(MANIFEST_PATH))? != manifest_bytes {
        return Err(VerificationError::InvalidManifest(
            "envelope changed during verification".to_owned(),
        ));
    }

    Ok(VerifiedArtifact {
        root: root.to_path_buf(),
        artifact_id: manifest.artifact_id().to_owned(),
    })
}

fn validate_root(root: &Path) -> Result<(), VerificationError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|source| io_error("inspect artifact root", root, source))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(VerificationError::InvalidRoot(root.to_path_buf()));
    }
    Ok(())
}

fn collect_members(
    root: &Path,
    require_manifest_envelope: bool,
) -> Result<Vec<ArtifactMember>, VerificationError> {
    let mut pending = vec![root.to_path_buf()];
    let mut members = Vec::new();
    let mut saw_envelope = false;
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|source| io_error("read artifact directory", &directory, source))?;
        for entry in entries {
            let entry =
                entry.map_err(|source| io_error("read artifact entry", &directory, source))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|source| io_error("inspect artifact entry", &path, source))?;
            if metadata.file_type().is_symlink() {
                return Err(VerificationError::UnsupportedEntry(path));
            }
            if metadata.is_dir() {
                let empty = fs::read_dir(&path)
                    .map_err(|source| io_error("inspect artifact directory", &path, source))?
                    .next()
                    .is_none();
                if empty && relative_path(root, &path)? != "project" {
                    return Err(VerificationError::UnsupportedEntry(path));
                }
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                return Err(VerificationError::UnsupportedEntry(path));
            }
            let relative = relative_path(root, &path)?;
            if relative == MANIFEST_PATH {
                saw_envelope = true;
                continue;
            }
            if metadata.len() > MAX_MEMBER_BYTES {
                return Err(VerificationError::OversizedMember(path));
            }
            members.push(stream_member(relative, &path, &metadata)?);
        }
    }
    if require_manifest_envelope && !saw_envelope {
        return Err(VerificationError::InvalidManifest(
            "artifact-manifest.json is missing".to_owned(),
        ));
    }
    members.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    Ok(members)
}

fn stream_member(
    relative: String,
    path: &Path,
    before: &fs::Metadata,
) -> Result<ArtifactMember, VerificationError> {
    let mut file =
        fs::File::open(path).map_err(|source| io_error("open artifact member", path, source))?;
    let mut hasher = ContentHasher::new();
    let mut size_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| io_error("read artifact member", path, source))?;
        if read == 0 {
            break;
        }
        size_bytes = size_bytes
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or_else(|| VerificationError::OversizedMember(path.to_path_buf()))?;
        if size_bytes > MAX_MEMBER_BYTES {
            return Err(VerificationError::OversizedMember(path.to_path_buf()));
        }
        hasher.update(&buffer[..read]);
    }
    let after = file
        .metadata()
        .map_err(|source| io_error("reinspect artifact member", path, source))?;
    if !after.is_file()
        || before.len() != size_bytes
        || after.len() != size_bytes
        || executable_mask(before) != executable_mask(&after)
        || before.modified().ok() != after.modified().ok()
    {
        return Err(VerificationError::MemberMismatch(relative));
    }
    ArtifactMember::from_digest(
        relative,
        hasher.finalize(),
        size_bytes,
        executable_mask(&after),
    )
    .map_err(Into::into)
}

fn verify_expected_member_set(
    evidence: &ReductionEvidence,
    members: &[ArtifactMember],
) -> Result<(), VerificationError> {
    let mut expected = BTreeSet::from([
        "attempts.jsonl".to_owned(),
        "issue.md".to_owned(),
        "reduction.json".to_owned(),
        "report.html".to_owned(),
        "reproduce.ps1".to_owned(),
        "reproduce.sh".to_owned(),
    ]);
    expected.extend(
        evidence
            .retained_manifest
            .entries()
            .iter()
            .map(|entry| format!("project/{}", entry.path)),
    );
    let declared = members
        .iter()
        .map(|member| member.path.clone())
        .collect::<BTreeSet<_>>();
    if declared != expected {
        return Err(VerificationError::MemberSetMismatch);
    }
    Ok(())
}

fn verify_retained_project(
    evidence: &ReductionEvidence,
    members: &[ArtifactMember],
) -> Result<(), VerificationError> {
    for retained in evidence.retained_manifest.entries() {
        if retained.kind != RetainedEntryKind::RegularFile {
            return Err(VerificationError::RetainedProjectMismatch(
                retained.path.clone(),
            ));
        }
        let artifact_path = format!("project/{}", retained.path);
        let member = members
            .iter()
            .find(|member| member.path == artifact_path)
            .ok_or_else(|| VerificationError::RetainedProjectMismatch(retained.path.clone()))?;
        if retained.sha256.as_deref() != Some(member.sha256.as_str())
            || retained.size_bytes != member.size_bytes
            || retained.executable_mask != Some(member.executable_mask)
        {
            return Err(VerificationError::RetainedProjectMismatch(
                retained.path.clone(),
            ));
        }
    }
    Ok(())
}

fn verify_derived_files(
    root: &Path,
    evidence: &ReductionEvidence,
) -> Result<(), VerificationError> {
    let mut expected_attempts = Vec::new();
    write_attempts_jsonl(&evidence.attempts, &mut expected_attempts)
        .map_err(|error| VerificationError::InvalidEvidence(error.to_string()))?;
    if read_control_file(&root.join("attempts.jsonl"))? != expected_attempts {
        return Err(VerificationError::AttemptLedgerMismatch);
    }
    if evidence
        .attempts
        .windows(2)
        .any(|pair| pair[0].event_id >= pair[1].event_id)
        || evidence.attempts.iter().any(|attempt| {
            attempt.event_id == 0
                || attempt.candidate_sha256.len() != 64
                || !attempt
                    .candidate_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                || attempt.observed_runs == 0
                || attempt.inconclusive_runs > attempt.observed_runs
                || !matches!(
                    attempt.verdict.as_str(),
                    "preserved" | "rejected" | "inconclusive"
                )
        })
    {
        return Err(VerificationError::AttemptLedgerMismatch);
    }
    if read_control_file(&root.join("report.html"))?
        != render_report(&ReportModel::from(evidence)).as_bytes()
    {
        return Err(VerificationError::ReportMismatch);
    }
    if read_control_file(&root.join("issue.md"))? != render_issue(evidence).as_bytes() {
        return Err(VerificationError::IssueMismatch);
    }
    let scripts = render_reproduction_scripts(&evidence.command);
    if read_control_file(&root.join("reproduce.sh"))? != scripts.shell.as_bytes() {
        return Err(VerificationError::ReproducerMismatch("reproduce.sh"));
    }
    if read_control_file(&root.join("reproduce.ps1"))? != scripts.powershell.as_bytes() {
        return Err(VerificationError::ReproducerMismatch("reproduce.ps1"));
    }
    Ok(())
}

fn read_control_file(path: &Path) -> Result<Vec<u8>, VerificationError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect artifact control file", path, source))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(VerificationError::UnsupportedEntry(path.to_path_buf()));
    }
    if metadata.len() > MAX_CONTROL_FILE_BYTES {
        return Err(VerificationError::OversizedMember(path.to_path_buf()));
    }
    fs::read(path).map_err(|source| io_error("read artifact control file", path, source))
}

fn relative_path(root: &Path, path: &Path) -> Result<String, VerificationError> {
    path.strip_prefix(root)
        .map_err(|_| VerificationError::UnsupportedEntry(path.to_path_buf()))?
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or_else(|| VerificationError::UnsupportedEntry(path.to_path_buf()))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"))
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> VerificationError {
    VerificationError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(unix)]
fn executable_mask(metadata: &fs::Metadata) -> u8 {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = metadata.permissions().mode();
    (u8::from(mode & 0o100 != 0) << 2)
        | (u8::from(mode & 0o010 != 0) << 1)
        | u8::from(mode & 0o001 != 0)
}

#[cfg(not(unix))]
const fn executable_mask(_: &fs::Metadata) -> u8 {
    0
}
