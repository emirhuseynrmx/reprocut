use std::{cmp::Ordering, fmt};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// A validated, platform-independent path below a project root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectPath(String);

impl ProjectPath {
    /// Validates a slash-separated project-relative path.
    pub fn new(path: impl Into<String>) -> Result<Self, TransformationError> {
        let path = path.into();
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
        if !safe {
            return Err(TransformationError::UnsafeProjectPath);
        }
        Ok(Self(path))
    }

    /// Returns the canonical slash-separated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A half-open non-empty byte interval.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    /// Validates `start..end` as a non-empty interval.
    pub const fn new(start: u64, end: u64) -> Result<Self, TransformationError> {
        if start >= end {
            return Err(TransformationError::InvalidByteRange);
        }
        Ok(Self { start, end })
    }

    /// Returns the inclusive start offset.
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Returns the exclusive end offset.
    pub const fn end(self) -> u64 {
        self.end
    }

    const fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// One immutable change to a source snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Remove one regular file.
    DeleteFile {
        /// Safe project-relative target.
        path: ProjectPath,
    },
    /// Replace a byte interval without interpreting source encoding.
    ReplaceRange {
        /// Safe project-relative target.
        path: ProjectPath,
        /// Half-open source interval.
        range: ByteRange,
        /// Exact replacement bytes.
        replacement: Vec<u8>,
    },
}

impl Operation {
    /// Creates a whole-file delete operation.
    pub fn delete(path: ProjectPath) -> Self {
        Self::DeleteFile { path }
    }

    /// Creates a byte-range replacement operation.
    pub fn replace(path: ProjectPath, range: ByteRange, replacement: Vec<u8>) -> Self {
        Self::ReplaceRange {
            path,
            range,
            replacement,
        }
    }

    /// Returns the operation target.
    pub const fn path(&self) -> &ProjectPath {
        match self {
            Self::DeleteFile { path } | Self::ReplaceRange { path, .. } => path,
        }
    }

    /// Returns a replacement interval, or `None` for a whole-file deletion.
    pub const fn range(&self) -> Option<ByteRange> {
        match self {
            Self::DeleteFile { .. } => None,
            Self::ReplaceRange { range, .. } => Some(*range),
        }
    }

    /// Returns replacement bytes, or `None` for a whole-file deletion.
    pub fn replacement(&self) -> Option<&[u8]> {
        match self {
            Self::DeleteFile { .. } => None,
            Self::ReplaceRange { replacement, .. } => Some(replacement),
        }
    }
}

/// A canonical SHA-256 identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ContentDigest([u8; 32]);

impl ContentDigest {
    /// Restores an identity already validated as exactly 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Hashes a stable byte encoding.
    pub fn of(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut value = [0_u8; 32];
        value.copy_from_slice(&digest);
        Self(value)
    }

    /// Returns the raw 32-byte identity.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns lowercase hexadecimal without an allocation in the hashing path.
    pub fn to_hex(self) -> String {
        use fmt::Write as _;

        let mut output = String::with_capacity(64);
        for byte in self.0 {
            write!(&mut output, "{byte:02x}").expect("writing to String is infallible");
        }
        output
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// An ordered, conflict-free, content-addressed operation set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transformation {
    operations: Vec<Operation>,
    stable_encoding: Vec<u8>,
    digest: ContentDigest,
}

impl Transformation {
    /// Canonicalizes operations and rejects order-dependent conflicts.
    pub fn new(mut operations: Vec<Operation>) -> Result<Self, TransformationError> {
        operations.sort_by(canonical_operation_order);
        operations.dedup();
        validate_conflicts(&operations)?;
        let stable_encoding = encode_operations(&operations)?;
        let digest = ContentDigest::of(&stable_encoding);
        Ok(Self {
            operations,
            stable_encoding,
            digest,
        })
    }

    /// Returns operations in canonical order.
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Returns the schema-versioned canonical bytes used for identity.
    pub fn stable_encoding(&self) -> &[u8] {
        &self.stable_encoding
    }

    /// Returns the transformation's SHA-256 identity.
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Subset/complement class used in deterministic frontier order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontierClass {
    /// A path-trie directory group.
    Directory,
    /// A direct subset candidate.
    Subset,
    /// The complement of a direct subset.
    Complement,
    /// A final one-operation sweep.
    Singleton,
    /// A manifest or syntax-aware operation.
    Structured,
}

/// Total order for a candidate inside one search frontier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CandidateRank {
    phase: u16,
    granularity: u32,
    class: FrontierClass,
    start: u32,
    transformation: ContentDigest,
}

impl CandidateRank {
    /// Creates an explicitly ordered candidate rank.
    pub const fn new(
        phase: u16,
        granularity: u32,
        class: FrontierClass,
        start: u32,
        transformation: ContentDigest,
    ) -> Self {
        Self {
            phase,
            granularity,
            class,
            start,
            transformation,
        }
    }

    /// Returns the candidate transformation identity.
    pub const fn transformation(self) -> ContentDigest {
        self.transformation
    }
}

/// Invalid path, range, or operation-set shape.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransformationError {
    /// Project paths must be normalized and relative.
    #[error("unsafe project-relative path")]
    UnsafeProjectPath,
    /// Byte ranges must satisfy `start < end`.
    #[error("byte range must be non-empty and ordered")]
    InvalidByteRange,
    /// Two replacements target intersecting source bytes.
    #[error("replacement byte ranges overlap")]
    OverlappingRanges,
    /// Whole-file deletion cannot coexist with a range operation for that file.
    #[error("whole-file deletion conflicts with a range operation")]
    DeleteReplaceConflict,
    /// Stable encoding cannot represent an in-memory length.
    #[error("transformation is too large to encode")]
    EncodingTooLarge,
}

fn canonical_operation_order(left: &Operation, right: &Operation) -> Ordering {
    left.path()
        .cmp(right.path())
        .then_with(|| operation_tag(left).cmp(&operation_tag(right)))
        .then_with(|| left.range().cmp(&right.range()))
        .then_with(|| left.replacement().cmp(&right.replacement()))
}

const fn operation_tag(operation: &Operation) -> u8 {
    match operation {
        Operation::DeleteFile { .. } => 0,
        Operation::ReplaceRange { .. } => 1,
    }
}

fn validate_conflicts(operations: &[Operation]) -> Result<(), TransformationError> {
    for (index, operation) in operations.iter().enumerate() {
        let mut previous_range = operation.range();
        for next in &operations[index + 1..] {
            if next.path() != operation.path() {
                break;
            }
            match (operation.range(), next.range()) {
                (None, _) | (_, None) => return Err(TransformationError::DeleteReplaceConflict),
                (Some(_), Some(range)) => {
                    if previous_range.is_some_and(|previous| previous.overlaps(range)) {
                        return Err(TransformationError::OverlappingRanges);
                    }
                    previous_range = Some(range);
                }
            }
        }
    }
    Ok(())
}

fn encode_operations(operations: &[Operation]) -> Result<Vec<u8>, TransformationError> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(b"REPROCUT-TRANSFORM\0");
    encoded.extend_from_slice(&1_u16.to_le_bytes());
    encode_len(&mut encoded, operations.len())?;
    for operation in operations {
        encoded.push(operation_tag(operation));
        encode_bytes(&mut encoded, operation.path().as_str().as_bytes())?;
        if let Operation::ReplaceRange {
            range, replacement, ..
        } = operation
        {
            encoded.extend_from_slice(&range.start().to_le_bytes());
            encoded.extend_from_slice(&range.end().to_le_bytes());
            encode_bytes(&mut encoded, replacement)?;
        }
    }
    Ok(encoded)
}

fn encode_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), TransformationError> {
    encode_len(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn encode_len(output: &mut Vec<u8>, length: usize) -> Result<(), TransformationError> {
    let length = u64::try_from(length).map_err(|_| TransformationError::EncodingTooLarge)?;
    output.extend_from_slice(&length.to_le_bytes());
    Ok(())
}
