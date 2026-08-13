use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{DiagnosticChannel, OracleMode, OracleSpec};

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
    /// Source project root; `ReproCut` never mutates it.
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
    /// Failure recognition mode: `automatic`, `regex`, or `exit_zero`.
    #[serde(default = "default_automatic")]
    pub oracle_mode: String,
    /// Required regular expressions for regex mode.
    #[serde(default)]
    pub failure_patterns: Vec<String>,
    /// Regular expressions that always reject a candidate.
    #[serde(default)]
    pub reject_patterns: Vec<String>,
    /// Explicit Python interpreter used to create candidate-local virtual environments.
    #[serde(default)]
    pub python_executable: Option<PathBuf>,
    /// Offline wheel corpus captured before candidate execution.
    #[serde(default)]
    pub python_wheelhouse: Option<PathBuf>,
    /// Canonical Python extras installed with the candidate.
    #[serde(default)]
    pub python_extras: Vec<String>,
    /// Optional strict schema-1 argv-only preparation specification.
    #[serde(default)]
    pub prepare_spec: Option<PathBuf>,
    /// Total observations in flaky-majority mode.
    #[serde(default)]
    pub flaky_runs: Option<u16>,
    /// Required matching observations in flaky-majority mode.
    #[serde(default)]
    pub flaky_required: Option<u16>,
    /// Maximum concurrent frontier evaluations; zero selects the engine default.
    #[serde(default)]
    pub jobs: usize,
    /// Optional durable `SQLite` session path.
    #[serde(default)]
    pub state: Option<PathBuf>,
    /// Discard incompatible state before a new session.
    #[serde(default)]
    pub restart: bool,
}

impl ReductionRequestV1 {
    /// Refuses requests from incompatible protocol generations.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported protocol version or an inconsistent oracle,
    /// isolation, resume, or restart configuration.
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
        let mode = match self.oracle_mode.as_str() {
            "automatic" => OracleMode::Automatic,
            "regex" => OracleMode::Regex,
            "exit_zero" => OracleMode::ExitZero,
            _ => {
                return Err(ProtocolError::InvalidConfiguration(
                    "unsupported oracle mode",
                ))
            }
        };
        let channel = match self.oracle_stream.as_str() {
            "auto" => DiagnosticChannel::Auto,
            "stderr" => DiagnosticChannel::Stderr,
            "stdout" => DiagnosticChannel::Stdout,
            "combined" => DiagnosticChannel::Combined,
            _ => {
                return Err(ProtocolError::InvalidConfiguration(
                    "unsupported oracle stream",
                ))
            }
        };
        OracleSpec::new(
            mode,
            channel,
            self.failure_patterns.clone(),
            self.reject_patterns.clone(),
        )
        .map_err(|_| ProtocolError::InvalidConfiguration("invalid oracle configuration"))?;
        let isolation_selected = self.preparation == "isolated_python";
        let isolation_complete =
            self.python_executable.is_some() && self.python_wheelhouse.is_some();
        let isolation_fields_present = self.python_executable.is_some()
            || self.python_wheelhouse.is_some()
            || !self.python_extras.is_empty()
            || self.prepare_spec.is_some();
        if isolation_selected != isolation_complete
            || (!isolation_selected && isolation_fields_present)
        {
            return Err(ProtocolError::InvalidConfiguration(
                "isolated_python requires python_executable and python_wheelhouse, and Python isolation fields require isolated_python",
            ));
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
    /// Fields were individually valid JSON but formed an unsafe contract.
    #[error("invalid protocol configuration: {0}")]
    InvalidConfiguration(&'static str),
}

fn default_auto() -> String {
    "auto".to_owned()
}

fn default_offline() -> String {
    "offline".to_owned()
}

fn default_automatic() -> String {
    "automatic".to_owned()
}

const fn default_timeout_ms() -> u64 {
    5_000
}

const fn default_capture_bytes() -> usize {
    1_048_576
}
