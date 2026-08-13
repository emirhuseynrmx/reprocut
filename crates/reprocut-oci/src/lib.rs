//! Minimal-context, fail-closed OCI archive export.

#![forbid(unsafe_code)]

use std::{
    ffi::OsStr,
    fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

use tempfile::TempDir;
use thiserror::Error;
use walkdir::WalkDir;

/// Runtime family used to select an explicit base image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeFamily {
    /// Rust project built and run with the pinned `Cargo` toolchain image.
    Cargo,
    /// Python project run with the pinned `CPython` image.
    Python,
    /// JavaScript project run with the pinned `Node.js` image.
    Npm,
    /// Project requiring only a minimal Debian userspace.
    Generic,
}

impl RuntimeFamily {
    /// Returns a reproducible, intentionally visible runtime base.
    pub const fn base_image(self) -> &'static str {
        match self {
            Self::Cargo => "rust:1.85-slim",
            Self::Python => "python:3.13-slim",
            Self::Npm => "node:22-slim",
            Self::Generic => "debian:bookworm-slim",
        }
    }
}

/// Supported builder frontends that can emit OCI archives directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Builder {
    /// Docker's `Buildx` frontend with direct `type=oci` output.
    DockerBuildx,
    /// A standalone `BuildKit` daemon accessed through `buildctl`.
    BuildKit,
}

impl Builder {
    /// Detects the first builder whose version probe succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`OciError::BuilderUnavailable`] when neither Docker Buildx nor standalone
    /// `BuildKit` can complete its version probe.
    pub fn detect() -> Result<Self, OciError> {
        if command_succeeds("docker", ["buildx", "version"]) {
            return Ok(Self::DockerBuildx);
        }
        if command_succeeds("buildctl", ["--version"]) {
            return Ok(Self::BuildKit);
        }
        Err(OciError::BuilderUnavailable)
    }
}

/// Complete immutable OCI export request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OciRequest {
    artifact_root: PathBuf,
    output: PathBuf,
    runtime: RuntimeFamily,
    command: Vec<String>,
    fingerprint_sha256: String,
    builder: Option<Builder>,
}

impl OciRequest {
    /// Creates an export request from one completed `ReproCut` artifact.
    pub fn new(
        artifact_root: PathBuf,
        output: PathBuf,
        runtime: RuntimeFamily,
        command: Vec<String>,
        fingerprint_sha256: String,
    ) -> Self {
        Self {
            artifact_root,
            output,
            runtime,
            command,
            fingerprint_sha256,
            builder: None,
        }
    }

    /// Pins a builder instead of auto-detecting one.
    #[must_use]
    pub fn with_builder(mut self, builder: Builder) -> Self {
        self.builder = Some(builder);
        self
    }

    /// Returns the requested output archive.
    pub fn output(&self) -> &Path {
        &self.output
    }
}

/// A prepared build context that owns its temporary directory.
#[derive(Debug)]
pub struct PreparedContext {
    _temporary: TempDir,
    root: PathBuf,
}

impl PreparedContext {
    /// Returns the minimal context root.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Prepares a context containing only the verified project and generated Dockerfile.
///
/// # Errors
///
/// Returns [`OciError`] when the entrypoint is empty, the verified project is absent, the source
/// contains a link or special file, metadata cannot be serialized, or filesystem work fails.
pub fn prepare_context(request: &OciRequest) -> Result<PreparedContext, OciError> {
    if request.command.is_empty() {
        return Err(OciError::EmptyCommand);
    }
    let source = request.artifact_root.join("project");
    if !source.is_dir() {
        return Err(OciError::MissingProject(source));
    }
    let temporary = tempfile::Builder::new()
        .prefix("reprocut-oci-")
        .tempdir()
        .map_err(|source| OciError::Io {
            operation: "create OCI context",
            path: std::env::temp_dir(),
            source,
        })?;
    let root = temporary.path().join("context");
    let project = root.join("project");
    fs::create_dir_all(&project).map_err(|source| OciError::Io {
        operation: "create project context",
        path: project.clone(),
        source,
    })?;
    copy_regular_tree(&source, &project)?;
    let dockerfile = render_dockerfile(request)?;
    fs::write(root.join("Dockerfile"), dockerfile).map_err(|source| OciError::Io {
        operation: "write generated Dockerfile",
        path: root.join("Dockerfile"),
        source,
    })?;
    Ok(PreparedContext {
        _temporary: temporary,
        root,
    })
}

/// Builds and validates one real OCI archive, never a context-only placeholder.
///
/// # Errors
///
/// Returns [`OciError`] when the destination already exists, no builder is available, context
/// preparation or builder execution fails, or the emitted tar lacks the required OCI members.
pub fn export_archive(request: &OciRequest) -> Result<Builder, OciError> {
    ensure_output_absent(request.output())?;
    let context = prepare_context(request)?;
    let builder = select_builder(request.builder, Builder::detect)?;
    if let Some(parent) = request
        .output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| OciError::Io {
            operation: "create OCI output parent",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let absolute_output = absolute_output(&request.output)?;
    let status = invoke_builder(builder, context.root(), &absolute_output)?;
    if !status.success() {
        return Err(OciError::BuilderFailed { builder, status });
    }
    validate_oci_archive(&absolute_output)?;
    Ok(builder)
}

fn render_dockerfile(request: &OciRequest) -> Result<Vec<u8>, OciError> {
    let entrypoint = serde_json::to_string(&request.command).map_err(OciError::Serialize)?;
    let label = serde_json::to_string(&request.fingerprint_sha256).map_err(OciError::Serialize)?;
    Ok(format!(
        "FROM {}\nWORKDIR /work\nCOPY project/ /work/\nLABEL org.reprocut.failure-fingerprint={}\nENTRYPOINT {}\n",
        request.runtime.base_image(),
        label,
        entrypoint,
    )
    .into_bytes())
}

fn copy_regular_tree(source: &Path, destination: &Path) -> Result<(), OciError> {
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(OciError::Walk)?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| OciError::UnsupportedEntry(entry.path().to_path_buf()))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_symlink() {
            return Err(OciError::UnsupportedEntry(entry.path().to_path_buf()));
        }
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|source| OciError::Io {
                operation: "create OCI context directory",
                path: target,
                source,
            })?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|source| OciError::Io {
                    operation: "create OCI file parent",
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            fs::copy(entry.path(), &target).map_err(|source| OciError::Io {
                operation: "copy verified project file",
                path: target,
                source,
            })?;
        } else {
            return Err(OciError::UnsupportedEntry(entry.path().to_path_buf()));
        }
    }
    Ok(())
}

fn invoke_builder(builder: Builder, context: &Path, output: &Path) -> Result<ExitStatus, OciError> {
    let mut command = match builder {
        Builder::DockerBuildx => {
            let mut command = Command::new("docker");
            command.args([
                OsStr::new("buildx"),
                OsStr::new("build"),
                OsStr::new("--network=none"),
                OsStr::new("--pull=false"),
                OsStr::new("--output"),
            ]);
            command.arg(format!("type=oci,dest={}", output.display()));
            command.arg(context);
            command
        }
        Builder::BuildKit => {
            let mut command = Command::new("buildctl");
            command.args([
                OsStr::new("build"),
                OsStr::new("--frontend"),
                OsStr::new("dockerfile.v0"),
                OsStr::new("--local"),
            ]);
            command.arg(format!("context={}", context.display()));
            command.args([OsStr::new("--local")]);
            command.arg(format!("dockerfile={}", context.display()));
            command.args([OsStr::new("--opt"), OsStr::new("network=none")]);
            command.args([OsStr::new("--output")]);
            command.arg(format!("type=oci,dest={}", output.display()));
            command
        }
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|source| OciError::SpawnBuilder { builder, source })
}

fn validate_oci_archive(path: &Path) -> Result<(), OciError> {
    let mut file = fs::File::open(path).map_err(|source| OciError::Io {
        operation: "open OCI archive",
        path: path.to_path_buf(),
        source,
    })?;
    let mut header = [0_u8; 512];
    let mut found_layout = false;
    let mut found_index = false;
    loop {
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(source) => {
                return Err(OciError::Io {
                    operation: "read OCI tar header",
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name = tar_text(&header[..100]);
        found_layout |= name == "oci-layout";
        found_index |= name == "index.json";
        let size = parse_tar_octal(&header[124..136])?;
        let padded = size.saturating_add(511) / 512 * 512;
        io::copy(&mut file.by_ref().take(padded), &mut io::sink()).map_err(|source| {
            OciError::Io {
                operation: "skip OCI tar member",
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    if found_layout && found_index {
        Ok(())
    } else {
        Err(OciError::InvalidArchive {
            has_layout: found_layout,
            has_index: found_index,
        })
    }
}

fn tar_text(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn parse_tar_octal(bytes: &[u8]) -> Result<u64, OciError> {
    let text = tar_text(bytes).trim().to_owned();
    let digits = text.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    u64::from_str_radix(digits, 8).map_err(|_| OciError::InvalidTarSize(text))
}

fn command_succeeds<const N: usize>(program: &str, arguments: [&str; N]) -> bool {
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn select_builder<F>(pinned: Option<Builder>, detect: F) -> Result<Builder, OciError>
where
    F: FnOnce() -> Result<Builder, OciError>,
{
    pinned.map_or_else(detect, Ok)
}

fn absolute_output(path: &Path) -> Result<PathBuf, OciError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|source| OciError::Io {
            operation: "resolve OCI output path",
            path: path.to_path_buf(),
            source,
        })
}

fn ensure_output_absent(path: &Path) -> Result<(), OciError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(OciError::OutputExists(path.to_path_buf())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(OciError::Io {
            operation: "inspect OCI output",
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// OCI context, builder, or archive validation failure.
#[derive(Debug, Error)]
pub enum OciError {
    /// The artifact does not contain the final verified `project/` tree.
    #[error("artifact has no verified project directory: {0}")]
    MissingProject(PathBuf),
    /// OCI cannot encode an empty entrypoint contract.
    #[error("OCI entrypoint command is empty")]
    EmptyCommand,
    /// Minimal contexts reject links, devices, sockets, and other non-regular entries.
    #[error("OCI context refuses symlinks and special files: {0}")]
    UnsupportedEntry(PathBuf),
    /// Export is fail-closed and will not overwrite an existing path.
    #[error("OCI output already exists: {0}")]
    OutputExists(PathBuf),
    /// Neither supported OCI-capable frontend passed detection.
    #[error("neither Docker Buildx nor BuildKit is available")]
    BuilderUnavailable,
    /// The selected builder executable could not be started or waited on.
    #[error("spawn {builder:?} failed: {source}")]
    SpawnBuilder {
        /// Builder selected for this invocation.
        builder: Builder,
        /// Operating-system process error.
        #[source]
        source: io::Error,
    },
    /// The builder ran but returned an unsuccessful exit status.
    #[error("{builder:?} failed with {status}")]
    BuilderFailed {
        /// Builder that reported failure.
        builder: Builder,
        /// Exact process exit status.
        status: ExitStatus,
    },
    /// The output tar omitted one or more mandatory OCI image-layout members.
    #[error(
        "builder output is not an OCI archive (oci-layout={has_layout}, index.json={has_index})"
    )]
    InvalidArchive {
        /// Whether the archive contained `oci-layout`.
        has_layout: bool,
        /// Whether the archive contained `index.json`.
        has_index: bool,
    },
    /// A tar member carried a non-octal or overflowing size field.
    #[error("invalid tar size field: {0:?}")]
    InvalidTarSize(String),
    /// An ordinary filesystem or archive-stream operation failed.
    #[error("{operation} failed for {path}: {source}")]
    Io {
        /// Stable operation label.
        operation: &'static str,
        /// Path affected by the operation.
        path: PathBuf,
        /// Underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// Recursive traversal of the verified project failed.
    #[error("walk verified project failed: {0}")]
    Walk(#[from] walkdir::Error),
    /// OCI entrypoint or label JSON serialization failed.
    #[error("serialize OCI metadata failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

#[cfg(test)]
mod archive_tests {
    use super::{select_builder, validate_oci_archive, Builder, OciError};
    use std::{fs, io::Write as _};

    #[test]
    fn archive_validation_requires_layout_and_index_members() {
        let temporary = tempfile::tempdir().expect("archive directory");
        let valid = temporary.path().join("valid.tar");
        write_tar(&valid, &[("oci-layout", b"{}"), ("index.json", b"{}")]);
        validate_oci_archive(&valid).expect("valid OCI member set");

        let invalid = temporary.path().join("invalid.tar");
        write_tar(&invalid, &[("index.json", b"{}")]);
        assert!(matches!(
            validate_oci_archive(&invalid),
            Err(OciError::InvalidArchive {
                has_layout: false,
                has_index: true
            })
        ));
    }

    #[test]
    fn a_pinned_builder_never_runs_environment_detection() {
        let selected = select_builder(Some(Builder::BuildKit), || {
            panic!("pinned builder must bypass environment detection")
        })
        .expect("pinned builder");
        assert_eq!(selected, Builder::BuildKit);
    }

    fn write_tar(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        let mut output = fs::File::create(path).expect("tar fixture");
        for &(name, contents) in entries {
            let mut header = [0_u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            header[100..108].copy_from_slice(b"0000644\0");
            header[108..116].copy_from_slice(b"0000000\0");
            header[116..124].copy_from_slice(b"0000000\0");
            write_octal(&mut header[124..136], contents.len());
            header[136..148].copy_from_slice(b"00000000000\0");
            header[148..156].fill(b' ');
            header[156] = b'0';
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            let checksum = header.iter().map(|byte| u64::from(*byte)).sum::<u64>();
            write_octal(
                &mut header[148..156],
                usize::try_from(checksum).expect("checksum"),
            );
            output.write_all(&header).expect("header");
            output.write_all(contents).expect("contents");
            let padding = (512 - contents.len() % 512) % 512;
            output.write_all(&vec![0_u8; padding]).expect("padding");
        }
        output.write_all(&[0_u8; 1_024]).expect("end marker");
    }

    fn write_octal(field: &mut [u8], value: usize) {
        field.fill(b'0');
        let text = format!("{value:o}");
        let start = field.len().saturating_sub(text.len()).saturating_sub(1);
        field[start..start + text.len()].copy_from_slice(text.as_bytes());
        field[field.len() - 1] = 0;
    }
}
