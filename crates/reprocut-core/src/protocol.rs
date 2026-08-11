use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Current additive JSON protocol version.
pub const PROTOCOL_VERSION: u16 = 1;

/// User action requested through the machine protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolAction {
    /// Start a new reduction session.
    Minimize,
    /// Resume an existing durable reduction session.
    Resume,
}

/// Complete protocol V1 request; absent optional fields use CLI-safe defaults.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReductionRequestV1 {
    /// Exact protocol generation expected by the caller.
    pub protocol_version: u16,
    /// New-session or resume operation.
    pub action: ProtocolAction,
    /// Source project root; ReproCut never mutates it.
    pub root: PathBuf,
    /// Destination for the verified reduction artifact.
    pub output: PathBuf,
    /// Ecosystem selector: `auto`, `cargo`, `python`, `npm`, or `none`.
    #[serde(default = "default_auto")]
    pub ecosystem: String,
    /// Candidate preparation policy.
    #[serde(default = "default_offline")]
    pub preparation: String,
    /// Failure command followed by its arguments.
    #[serde(default)]
    pub command: Vec<String>,
    /// Per-attempt wall-clock limit in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Combined bounded diagnostic capture budget.
    #[serde(default = "default_capture_bytes")]
    pub max_output_bytes: usize,
    /// Diagnostic channel used to identify the failure.
    #[serde(default = "default_auto")]
    pub oracle_stream: String,
    /// Total observations in flaky-majority mode.
    #[serde(default)]
    pub flaky_runs: Option<u16>,
    /// Required matching observations in flaky-majority mode.
    #[serde(default)]
    pub flaky_required: Option<u16>,
    /// Maximum concurrent frontier evaluations; zero selects the engine default.
    #[serde(default)]
    pub jobs: usize,
    /// Optional durable SQLite session path.
    #[serde(default)]
    pub state: Option<PathBuf>,
    /// Discard incompatible state before a new session.
    #[serde(default)]
    pub restart: bool,
}

impl ReductionRequestV1 {
    /// Refuses requests from incompatible protocol generations.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion {
                found: self.protocol_version,
                supported: PROTOCOL_VERSION,
            });
        }
        if self.action == ProtocolAction::Resume && self.restart {
            return Err(ProtocolError::ResumeRestartConflict);
        }
        Ok(())
    }
}

/// One line-delimited progress or terminal protocol event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEventV1 {
    /// The request was validated and execution is starting.
    Started {
        /// Protocol generation of this event.
        protocol_version: u16,
        /// Requested operation.
        action: ProtocolAction,
        /// Source project root.
        root: PathBuf,
    },
    /// The original failure produced a stable identity.
    BaselineStable {
        /// Protocol generation of this event.
        protocol_version: u16,
        /// SHA-256 failure identity recorded in the evidence artifact.
        fingerprint_sha256: String,
    },
    /// Reduction and final verification completed successfully.
    Completed {
        /// Protocol generation of this event.
        protocol_version: u16,
        /// Verified artifact directory.
        output: PathBuf,
        /// Machine-readable evidence path.
        evidence: PathBuf,
        /// Self-contained HTML report path.
        report: PathBuf,
        /// GitHub-ready issue body path.
        issue: PathBuf,
    },
    /// The request reached a terminal error.
    Failed {
        /// Protocol generation of this event.
        protocol_version: u16,
        /// Human-readable terminal error without a backtrace or secrets.
        message: String,
    },
}

/// Protocol request compatibility error.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProtocolError {
    /// The caller requested an unsupported protocol generation.
    #[error("unsupported protocol version {found}; this binary supports {supported}")]
    UnsupportedVersion {
        /// Version supplied by the caller.
        found: u16,
        /// Version implemented by this binary.
        supported: u16,
    },
    /// Resume cannot be combined with destructive state restart.
    #[error("protocol resume requests cannot set restart=true")]
    ResumeRestartConflict,
}

fn default_auto() -> String {
    "auto".to_owned()
}

fn default_offline() -> String {
    "offline".to_owned()
}

const fn default_timeout_ms() -> u64 {
    5_000
}

const fn default_capture_bytes() -> usize {
    1_048_576
}
