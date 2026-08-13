use std::{error::Error, fmt};

use reprocut_core::{ContentDigest, ContentHasher, ARTIFACT_MANIFEST_SCHEMA};
use serde::{Deserialize, Serialize};

/// Current canonical artifact-manifest payload schema.
pub const ARTIFACT_MANIFEST_SCHEMA_VERSION: u16 = ARTIFACT_MANIFEST_SCHEMA;

const MANIFEST_ENVELOPE_PATH: &str = "artifact-manifest.json";

/// Kind of one entry represented by the retained project manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedEntryKind {
    /// A regular file with byte identity and a portable Unix execute mask.
    RegularFile,
    /// An explicitly represented empty directory.
    EmptyDirectory,
    /// A relative, in-root symbolic link.
    Symlink,
}

/// One canonical retained-project entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetainedEntry {
    /// Canonical slash-separated project-relative path.
    pub path: String,
    /// Filesystem entry kind.
    pub kind: RetainedEntryKind,
    /// SHA-256 content identity for regular files.
    pub sha256: Option<String>,
    /// Regular-file byte length; zero for directories and links.
    pub size_bytes: u64,
    /// Owner/group/other execute bits for regular files.
    pub executable_mask: Option<u8>,
    /// Canonical relative target for symbolic links.
    pub symlink_target: Option<String>,
}

impl RetainedEntry {
    /// Creates one content- and metadata-bound regular-file entry.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for an unsafe path, an invalid execute mask, or a byte length
    /// that cannot be represented by the schema.
    pub fn regular_file(
        path: impl Into<String>,
        contents: &[u8],
        executable_mask: u8,
    ) -> Result<Self, ManifestError> {
        let path = checked_path(path.into())?;
        if executable_mask > 0b111 {
            return Err(ManifestError::InvalidExecutableMask(executable_mask));
        }
        Ok(Self {
            path,
            kind: RetainedEntryKind::RegularFile,
            sha256: Some(ContentDigest::of(contents).to_hex()),
            size_bytes: u64::try_from(contents.len()).map_err(|_| ManifestError::LengthOverflow)?,
            executable_mask: Some(executable_mask),
            symlink_target: None,
        })
    }

    /// Creates one explicitly represented empty-directory entry.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] when `path` is not a canonical relative path.
    pub fn empty_directory(path: impl Into<String>) -> Result<Self, ManifestError> {
        Ok(Self {
            path: checked_path(path.into())?,
            kind: RetainedEntryKind::EmptyDirectory,
            sha256: None,
            size_bytes: 0,
            executable_mask: None,
            symlink_target: None,
        })
    }

    /// Creates one relative symbolic-link entry.
    ///
    /// The target is deliberately restricted to the same non-traversing path language as project
    /// entries. Snapshot capture is responsible for resolving it within the root.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] when either path is not canonical and relative.
    pub fn symlink(
        path: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, ManifestError> {
        Ok(Self {
            path: checked_path(path.into())?,
            kind: RetainedEntryKind::Symlink,
            sha256: None,
            size_bytes: 0,
            executable_mask: None,
            symlink_target: Some(checked_path(target.into())?),
        })
    }

    /// Returns the domain-separated identity of path, kind, content, size, and metadata.
    pub fn canonical_digest(&self) -> ContentDigest {
        let mut hasher = ContentHasher::new();
        hasher.update(b"REPROCUT-RETAINED-ENTRY-V1\0");
        encode_text(&mut hasher, &self.path);
        hasher.update(&[match self.kind {
            RetainedEntryKind::RegularFile => 0,
            RetainedEntryKind::EmptyDirectory => 1,
            RetainedEntryKind::Symlink => 2,
        }]);
        encode_optional_text(&mut hasher, self.sha256.as_deref());
        hasher.update(&self.size_bytes.to_le_bytes());
        encode_optional_byte(&mut hasher, self.executable_mask);
        encode_optional_text(&mut hasher, self.symlink_target.as_deref());
        hasher.finalize()
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if checked_path(self.path.clone())? != self.path {
            return Err(ManifestError::UnsafePath(self.path.clone()));
        }
        match self.kind {
            RetainedEntryKind::RegularFile
                if lower_sha256(self.sha256.as_deref().unwrap_or_default())
                    && self.executable_mask.is_some_and(|mask| mask <= 0b111)
                    && self.symlink_target.is_none() =>
            {
                Ok(())
            }
            RetainedEntryKind::EmptyDirectory
                if self.sha256.is_none()
                    && self.size_bytes == 0
                    && self.executable_mask.is_none()
                    && self.symlink_target.is_none() =>
            {
                Ok(())
            }
            RetainedEntryKind::Symlink
                if self.sha256.is_none()
                    && self.size_bytes == 0
                    && self.executable_mask.is_none()
                    && self
                        .symlink_target
                        .as_ref()
                        .is_some_and(|target| checked_path(target.clone()).is_ok()) =>
            {
                Ok(())
            }
            _ => Err(ManifestError::InvalidEntry(self.path.clone())),
        }
    }
}

/// Sorted, byte- and metadata-bound retained snapshot manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetainedManifest {
    schema_version: u16,
    entries: Vec<RetainedEntry>,
    total_bytes: u64,
    manifest_sha256: String,
}

impl RetainedManifest {
    /// Canonicalizes one retained entry set and computes its identity.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for invalid entries or duplicate paths.
    pub fn new(mut entries: Vec<RetainedEntry>) -> Result<Self, ManifestError> {
        entries.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        validate_unique_entries(&entries)?;
        let total_bytes = entries
            .iter()
            .fold(0_u64, |total, entry| total.saturating_add(entry.size_bytes));
        let manifest_sha256 = retained_payload_digest(&entries, total_bytes).to_hex();
        Ok(Self {
            schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION,
            entries,
            total_bytes,
            manifest_sha256,
        })
    }

    /// Returns entries in canonical path order.
    pub fn entries(&self) -> &[RetainedEntry] {
        &self.entries
    }

    /// Returns the saturating sum of regular-file byte lengths.
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Returns the canonical retained-manifest identity.
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    /// Recomputes and validates schema, ordering, metadata, totals, and identity.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for any mismatch.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != ARTIFACT_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchema(self.schema_version));
        }
        validate_unique_entries(&self.entries)?;
        if !self
            .entries
            .windows(2)
            .all(|pair| pair[0].path < pair[1].path)
        {
            return Err(ManifestError::NonCanonicalOrder);
        }
        let total_bytes = self
            .entries
            .iter()
            .fold(0_u64, |total, entry| total.saturating_add(entry.size_bytes));
        if total_bytes != self.total_bytes {
            return Err(ManifestError::TotalBytesMismatch);
        }
        if retained_payload_digest(&self.entries, total_bytes).to_hex() != self.manifest_sha256 {
            return Err(ManifestError::DigestMismatch);
        }
        Ok(())
    }
}

/// One non-envelope member of a complete published artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMember {
    /// Canonical artifact-relative path.
    pub path: String,
    /// SHA-256 identity of the exact member bytes.
    pub sha256: String,
    /// Exact member byte length.
    pub size_bytes: u64,
    /// Owner/group/other execute bits.
    pub executable_mask: u8,
}

impl ArtifactMember {
    /// Creates a member from exact bytes and metadata.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for an unsafe or reserved path, an invalid mask, or overflow.
    pub fn from_bytes(
        path: impl Into<String>,
        contents: &[u8],
        executable_mask: u8,
    ) -> Result<Self, ManifestError> {
        let path = checked_path(path.into())?;
        if path == MANIFEST_ENVELOPE_PATH {
            return Err(ManifestError::ReservedEnvelopeMember);
        }
        if executable_mask > 0b111 {
            return Err(ManifestError::InvalidExecutableMask(executable_mask));
        }
        Ok(Self {
            path,
            sha256: ContentDigest::of(contents).to_hex(),
            size_bytes: u64::try_from(contents.len()).map_err(|_| ManifestError::LengthOverflow)?,
            executable_mask,
        })
    }

    /// Creates a member from an already streamed content digest and exact byte length.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for an unsafe or reserved path or an invalid execute mask.
    pub fn from_digest(
        path: impl Into<String>,
        sha256: ContentDigest,
        size_bytes: u64,
        executable_mask: u8,
    ) -> Result<Self, ManifestError> {
        let path = checked_path(path.into())?;
        if path == MANIFEST_ENVELOPE_PATH {
            return Err(ManifestError::ReservedEnvelopeMember);
        }
        if executable_mask > 0b111 {
            return Err(ManifestError::InvalidExecutableMask(executable_mask));
        }
        Ok(Self {
            path,
            sha256: sha256.to_hex(),
            size_bytes,
            executable_mask,
        })
    }

    fn canonical_digest(&self) -> ContentDigest {
        let mut hasher = ContentHasher::new();
        hasher.update(b"REPROCUT-ARTIFACT-MEMBER-V1\0");
        encode_text(&mut hasher, &self.path);
        encode_text(&mut hasher, &self.sha256);
        hasher.update(&self.size_bytes.to_le_bytes());
        hasher.update(&[self.executable_mask]);
        hasher.finalize()
    }

    fn validate(&self) -> Result<(), ManifestError> {
        checked_path(self.path.clone())?;
        if self.path == MANIFEST_ENVELOPE_PATH {
            return Err(ManifestError::ReservedEnvelopeMember);
        }
        if !lower_sha256(&self.sha256) || self.executable_mask > 0b111 {
            return Err(ManifestError::InvalidEntry(self.path.clone()));
        }
        Ok(())
    }
}

/// Non-circular manifest envelope for a complete artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    schema_version: u16,
    artifact_id: String,
    members: Vec<ArtifactMember>,
}

impl ArtifactManifest {
    /// Canonicalizes members and calculates the root artifact identity.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for invalid, duplicate, or reserved members.
    pub fn new(mut members: Vec<ArtifactMember>) -> Result<Self, ManifestError> {
        members.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        validate_unique_members(&members)?;
        let artifact_id = artifact_payload_digest(&members).to_hex();
        Ok(Self {
            schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION,
            artifact_id,
            members,
        })
    }

    /// Returns the manifest schema generation.
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Returns members in canonical path order.
    pub fn members(&self) -> &[ArtifactMember] {
        &self.members
    }

    /// Returns the stored root artifact identity.
    pub fn artifact_id(&self) -> &str {
        &self.artifact_id
    }

    /// Recomputes the canonical payload identity, excluding the envelope itself.
    pub fn payload_digest(&self) -> String {
        artifact_payload_digest(&self.members).to_hex()
    }

    /// Recomputes and validates schema, members, ordering, and root identity.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] for any mismatch.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != ARTIFACT_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchema(self.schema_version));
        }
        validate_unique_members(&self.members)?;
        if !self
            .members
            .windows(2)
            .all(|pair| pair[0].path < pair[1].path)
        {
            return Err(ManifestError::NonCanonicalOrder);
        }
        if self.payload_digest() != self.artifact_id {
            return Err(ManifestError::DigestMismatch);
        }
        Ok(())
    }
}

/// Canonical manifest construction or verification error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    /// A path is absolute, traversing, drive-qualified, empty, or otherwise non-canonical.
    UnsafePath(String),
    /// Two entries have the same canonical path.
    DuplicatePath(String),
    /// `artifact-manifest.json` cannot bind itself.
    ReservedEnvelopeMember,
    /// An execute mask contains bits outside owner/group/other execute.
    InvalidExecutableMask(u8),
    /// One entry has fields inconsistent with its kind.
    InvalidEntry(String),
    /// A platform length did not fit in the schema.
    LengthOverflow,
    /// Entries were not serialized in canonical lexical order.
    NonCanonicalOrder,
    /// The stored byte total disagrees with entries.
    TotalBytesMismatch,
    /// The stored identity disagrees with the canonical payload.
    DigestMismatch,
    /// A persisted schema is not supported by this binary.
    UnsupportedSchema(u16),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsafePath(path) => write!(formatter, "unsafe manifest path: {path}"),
            Self::DuplicatePath(path) => write!(formatter, "duplicate manifest path: {path}"),
            Self::ReservedEnvelopeMember => {
                formatter.write_str("artifact manifest cannot include its own envelope")
            }
            Self::InvalidExecutableMask(mask) => {
                write!(formatter, "invalid executable mask: {mask:#05b}")
            }
            Self::InvalidEntry(path) => write!(formatter, "invalid manifest entry: {path}"),
            Self::LengthOverflow => formatter.write_str("manifest length exceeds u64"),
            Self::NonCanonicalOrder => formatter.write_str("manifest paths are not canonical"),
            Self::TotalBytesMismatch => formatter.write_str("manifest byte total mismatch"),
            Self::DigestMismatch => formatter.write_str("manifest digest mismatch"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported artifact manifest schema: {version}")
            }
        }
    }
}

impl Error for ManifestError {}

fn retained_payload_digest(entries: &[RetainedEntry], total_bytes: u64) -> ContentDigest {
    let mut hasher = ContentHasher::new();
    hasher.update(b"REPROCUT-RETAINED-MANIFEST-V1\0");
    hasher.update(&ARTIFACT_MANIFEST_SCHEMA_VERSION.to_le_bytes());
    encode_len(&mut hasher, entries.len());
    for entry in entries {
        hasher.update(entry.canonical_digest().as_bytes());
    }
    hasher.update(&total_bytes.to_le_bytes());
    hasher.finalize()
}

fn artifact_payload_digest(members: &[ArtifactMember]) -> ContentDigest {
    let mut hasher = ContentHasher::new();
    hasher.update(b"REPROCUT-ARTIFACT-MANIFEST-V1\0");
    hasher.update(&ARTIFACT_MANIFEST_SCHEMA_VERSION.to_le_bytes());
    encode_len(&mut hasher, members.len());
    for member in members {
        hasher.update(member.canonical_digest().as_bytes());
    }
    hasher.finalize()
}

fn validate_unique_entries(entries: &[RetainedEntry]) -> Result<(), ManifestError> {
    for entry in entries {
        entry.validate()?;
    }
    reject_duplicate_paths(entries.iter().map(|entry| entry.path.as_str()))
}

fn validate_unique_members(members: &[ArtifactMember]) -> Result<(), ManifestError> {
    for member in members {
        member.validate()?;
    }
    reject_duplicate_paths(members.iter().map(|member| member.path.as_str()))
}

fn reject_duplicate_paths<'path>(
    paths: impl IntoIterator<Item = &'path str>,
) -> Result<(), ManifestError> {
    let mut previous: Option<&str> = None;
    for path in paths {
        if previous == Some(path) {
            return Err(ManifestError::DuplicatePath(path.to_owned()));
        }
        previous = Some(path);
    }
    Ok(())
}

fn checked_path(path: String) -> Result<String, ManifestError> {
    let drive_prefix = path
        .as_bytes()
        .get(1)
        .is_some_and(|character| *character == b':');
    let safe = !path.is_empty()
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains(['\\', '\0'])
        && !drive_prefix
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..");
    if safe {
        Ok(path)
    } else {
        Err(ManifestError::UnsafePath(path))
    }
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn encode_len(hasher: &mut ContentHasher, value: usize) {
    hasher.update(&u64::try_from(value).unwrap_or(u64::MAX).to_le_bytes());
}

fn encode_text(hasher: &mut ContentHasher, value: &str) {
    encode_len(hasher, value.len());
    hasher.update(value.as_bytes());
}

fn encode_optional_text(hasher: &mut ContentHasher, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            encode_text(hasher, value);
        }
        None => hasher.update(&[0]),
    }
}

fn encode_optional_byte(hasher: &mut ContentHasher, value: Option<u8>) {
    match value {
        Some(value) => hasher.update(&[1, value]),
        None => hasher.update(&[0]),
    }
}
