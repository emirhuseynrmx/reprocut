use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use reprocut_core::{ContentDigest, ContentHasher};
use reprocut_runner::{ChildEnvironment, CommandSpec, ProcessRunner, RunnerError};
use serde::Deserialize;
use tempfile::TempDir;
use thiserror::Error;

const MAX_PREPARE_SPEC_BYTES: u64 = 1_048_576;
const MAX_PREPARE_COMMANDS: usize = 32;
const MAX_PREPARE_ARGUMENTS: usize = 64;
const MAX_ARGUMENT_BYTES: usize = 4_096;
const ENVIRONMENT_POLICY_VERSION: &str = "python-isolation-v1";
const INTERPRETER_PROBE: &str = "import json,sys;print(json.dumps({'implementation':sys.implementation.name,'version':list(sys.version_info[:3]),'executable':sys.executable},sort_keys=True,separators=(',',':')))";

/// Caller-owned inputs required to isolate every Python candidate offline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonIsolationRequest {
    interpreter: PathBuf,
    wheelhouse: PathBuf,
    extras: Vec<String>,
    prepare_spec: Option<PathBuf>,
}

impl PythonIsolationRequest {
    /// Creates an isolation request with no extras or custom preparation commands.
    pub fn new(interpreter: PathBuf, wheelhouse: PathBuf) -> Self {
        Self {
            interpreter,
            wheelhouse,
            extras: Vec::new(),
            prepare_spec: None,
        }
    }

    /// Validates, canonicalizes, and sorts Python extra names.
    pub fn with_extras(
        mut self,
        extras: impl IntoIterator<Item = String>,
    ) -> Result<Self, PythonPreparationError> {
        self.extras = extras
            .into_iter()
            .map(|extra| normalize_extra(&extra))
            .collect::<Result<Vec<_>, _>>()?;
        self.extras.sort_unstable();
        self.extras.dedup();
        Ok(self)
    }

    /// Adds an optional schema-1 argv-only preparation specification.
    pub fn with_prepare_spec(mut self, prepare_spec: PathBuf) -> Self {
        self.prepare_spec = Some(prepare_spec);
        self
    }

    /// Returns the explicit interpreter path.
    pub fn interpreter(&self) -> &Path {
        &self.interpreter
    }

    /// Returns the caller-owned wheelhouse path captured before execution.
    pub fn wheelhouse(&self) -> &Path {
        &self.wheelhouse
    }

    /// Returns normalized extras in lexical order.
    pub fn extras(&self) -> &[String] {
        &self.extras
    }

    /// Returns the optional argv-only preparation spec.
    pub fn prepare_spec(&self) -> Option<&Path> {
        self.prepare_spec.as_deref()
    }
}

/// A static or per-candidate Python isolation failure.
#[derive(Debug, Error)]
pub enum PythonPreparationError {
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
    /// The configured interpreter could not be probed reliably.
    #[error("Python interpreter probe failed")]
    InterpreterProbe,
    /// Interpreter identity output was malformed or incomplete.
    #[error("Python interpreter returned an invalid identity")]
    InvalidInterpreterIdentity,
    /// A wheelhouse member was not an ordinary safe wheel file.
    #[error("wheelhouse contains an unsafe entry: {path}")]
    UnsafeWheel {
        /// Rejected entry.
        path: PathBuf,
    },
    /// Two wheels collide under case-insensitive filesystems.
    #[error("wheelhouse contains case-insensitive duplicate names: {name}")]
    DuplicateWheel {
        /// Colliding lowercase name.
        name: String,
    },
    /// An extra name did not follow normalized Python project-name syntax.
    #[error("invalid Python extra name: {name}")]
    InvalidExtra {
        /// Rejected name.
        name: String,
    },
    /// The preparation specification was invalid or exceeded its bounds.
    #[error("invalid Python preparation spec: {reason}")]
    InvalidPrepareSpec {
        /// Stable validation reason.
        reason: &'static str,
    },
    /// The requested test command escaped the isolated venv or candidate.
    #[error("Python command is outside the isolated candidate: {program}")]
    UnsafeCommand {
        /// Rejected program.
        program: PathBuf,
    },
    /// The bounded process runner failed.
    #[error(transparent)]
    Runner(#[from] RunnerError),
}

/// Owned, content-addressed Python inputs captured before candidate execution.
#[derive(Debug)]
pub(crate) struct FrozenPythonPreparation {
    interpreter: PathBuf,
    interpreter_identity: Vec<u8>,
    wheelhouse_owner: TempDir,
    wheelhouse_digest: ContentDigest,
    extras: Vec<String>,
    prepare_spec_bytes: Vec<u8>,
    prepare_commands: Vec<Vec<String>>,
    digest: ContentDigest,
}

impl FrozenPythonPreparation {
    /// Captures all caller-owned isolation inputs and probes the explicit interpreter once.
    pub(crate) fn capture(
        request: &PythonIsolationRequest,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Result<Self, PythonPreparationError> {
        let interpreter = canonicalize(&request.interpreter, "canonicalize Python interpreter")?;
        if !interpreter.is_file() {
            return Err(PythonPreparationError::UnsafeCommand {
                program: interpreter,
            });
        }
        let identity = probe_interpreter(&interpreter, timeout, max_output_bytes)?;
        let (wheelhouse_owner, wheelhouse_digest) = freeze_wheelhouse(&request.wheelhouse)?;
        let (prepare_spec_bytes, prepare_commands) = request
            .prepare_spec
            .as_deref()
            .map(read_prepare_spec)
            .transpose()?
            .unwrap_or_default();
        let digest = preparation_digest(
            &interpreter,
            &identity,
            wheelhouse_digest,
            &request.extras,
            &prepare_spec_bytes,
            timeout,
            max_output_bytes,
        );
        Ok(Self {
            interpreter,
            interpreter_identity: identity,
            wheelhouse_owner,
            wheelhouse_digest,
            extras: request.extras.clone(),
            prepare_spec_bytes,
            prepare_commands,
            digest,
        })
    }

    /// Returns the complete static preparation identity.
    pub(crate) const fn digest(&self) -> ContentDigest {
        self.digest
    }

    /// Rejects absolute or parent-relative commands before any candidate starts.
    pub(crate) fn validate_original_program(
        &self,
        program: &Path,
    ) -> Result<(), PythonPreparationError> {
        if program.is_absolute()
            || program
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(PythonPreparationError::UnsafeCommand {
                program: program.to_path_buf(),
            });
        }
        Ok(())
    }

    /// Returns the frozen wheel corpus identity.
    pub(crate) const fn wheelhouse_digest(&self) -> ContentDigest {
        self.wheelhouse_digest
    }

    /// Returns the bounded interpreter identity evidence.
    pub(crate) fn interpreter_identity(&self) -> &[u8] {
        &self.interpreter_identity
    }

    /// Returns the exact prepare-spec bytes participating in identity.
    pub(crate) fn prepare_spec_bytes(&self) -> &[u8] {
        &self.prepare_spec_bytes
    }

    /// Creates a fresh venv, installs only from the frozen wheelhouse, and runs custom setup.
    pub(crate) fn prepare(
        &self,
        candidate_root: &Path,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Result<Option<PreparedPythonCandidate>, PythonPreparationError> {
        let venv = candidate_root
            .parent()
            .ok_or_else(|| PythonPreparationError::UnsafeCommand {
                program: candidate_root.to_path_buf(),
            })?
            .join("python-venv");
        let base_environment = scrubbed_environment();
        let create = CommandSpec::new(
            self.interpreter.clone(),
            vec![
                OsString::from("-I"),
                OsString::from("-m"),
                OsString::from("venv"),
                venv.clone().into_os_string(),
            ],
            candidate_root.to_path_buf(),
            timeout,
            max_output_bytes,
        )
        .with_environment(base_environment);
        if !successful(&ProcessRunner::run(&create)?) {
            return Ok(None);
        }

        let python = venv_python(&venv);
        let scripts = venv_scripts(&venv);
        let environment = candidate_environment(&venv, self.wheelhouse_owner.path(), &scripts);
        let requirement = if self.extras.is_empty() {
            ".".to_owned()
        } else {
            format!(".[{}]", self.extras.join(","))
        };
        let install = CommandSpec::new(
            python.clone(),
            vec![
                OsString::from("-I"),
                OsString::from("-m"),
                OsString::from("pip"),
                OsString::from("--isolated"),
                OsString::from("install"),
                OsString::from("--disable-pip-version-check"),
                OsString::from("--no-input"),
                OsString::from("--no-index"),
                OsString::from("--find-links"),
                self.wheelhouse_owner.path().as_os_str().to_owned(),
                OsString::from(requirement),
            ],
            candidate_root.to_path_buf(),
            timeout,
            max_output_bytes,
        )
        .with_environment(environment.clone());
        if !successful(&ProcessRunner::run(&install)?) {
            return Ok(None);
        }

        for command in &self.prepare_commands {
            let expanded = expand_command(
                command,
                &python,
                candidate_root,
                self.wheelhouse_owner.path(),
            )?;
            let Some((program, arguments)) = expanded.split_first() else {
                return Err(PythonPreparationError::InvalidPrepareSpec {
                    reason: "empty command",
                });
            };
            let command = CommandSpec::new(
                PathBuf::from(program),
                arguments.iter().map(OsString::from).collect(),
                candidate_root.to_path_buf(),
                timeout,
                max_output_bytes,
            )
            .with_environment(environment.clone());
            if !successful(&ProcessRunner::run(&command)?) {
                return Ok(None);
            }
        }

        Ok(Some(PreparedPythonCandidate {
            candidate_root: candidate_root.to_path_buf(),
            python,
            scripts,
            environment,
        }))
    }
}

/// Candidate-local venv and environment, valid only while its workspace exists.
#[derive(Clone, Debug)]
pub(crate) struct PreparedPythonCandidate {
    candidate_root: PathBuf,
    python: PathBuf,
    scripts: PathBuf,
    environment: ChildEnvironment,
}

impl PreparedPythonCandidate {
    /// Resolves the caller command without falling back to host executables.
    pub(crate) fn command_for(
        &self,
        original_program: &Path,
        arguments: &[OsString],
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Result<CommandSpec, PythonPreparationError> {
        let program = self.resolve_program(original_program)?;
        Ok(CommandSpec::new(
            program,
            arguments.to_vec(),
            self.candidate_root.clone(),
            timeout,
            max_output_bytes,
        )
        .with_environment(self.environment.clone()))
    }

    fn resolve_program(&self, program: &Path) -> Result<PathBuf, PythonPreparationError> {
        if program.is_absolute() {
            return Err(PythonPreparationError::UnsafeCommand {
                program: program.to_path_buf(),
            });
        }
        if program.components().count() == 1 {
            let name = program.as_os_str().to_string_lossy();
            if is_python_name(&name) {
                return Ok(self.python.clone());
            }
            let tool = self.scripts.join(program);
            if tool.is_file() {
                return Ok(tool);
            }
            return Err(PythonPreparationError::UnsafeCommand {
                program: program.to_path_buf(),
            });
        }
        if program
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(PythonPreparationError::UnsafeCommand {
                program: program.to_path_buf(),
            });
        }
        let project_tool = self.candidate_root.join(program);
        let canonical = canonicalize(&project_tool, "canonicalize candidate command")?;
        if !canonical.starts_with(&self.candidate_root) || !canonical.is_file() {
            return Err(PythonPreparationError::UnsafeCommand {
                program: program.to_path_buf(),
            });
        }
        Ok(canonical)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareSpec {
    schema: u16,
    commands: Vec<Vec<String>>,
}

fn read_prepare_spec(path: &Path) -> Result<(Vec<u8>, Vec<Vec<String>>), PythonPreparationError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| PythonPreparationError::Io {
        operation: "inspect Python preparation spec",
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PREPARE_SPEC_BYTES {
        return Err(PythonPreparationError::InvalidPrepareSpec {
            reason: "spec must be a regular file no larger than 1 MiB",
        });
    }
    let bytes = fs::read(path).map_err(|source| PythonPreparationError::Io {
        operation: "read Python preparation spec",
        path: path.to_path_buf(),
        source,
    })?;
    let spec: PrepareSpec =
        serde_json::from_slice(&bytes).map_err(|_| PythonPreparationError::InvalidPrepareSpec {
            reason: "spec must be strict schema-1 JSON",
        })?;
    if spec.schema != 1 || spec.commands.len() > MAX_PREPARE_COMMANDS {
        return Err(PythonPreparationError::InvalidPrepareSpec {
            reason: "unsupported schema or too many commands",
        });
    }
    for command in &spec.commands {
        if command.is_empty() || command.len() > MAX_PREPARE_ARGUMENTS {
            return Err(PythonPreparationError::InvalidPrepareSpec {
                reason: "commands must contain between 1 and 64 argv values",
            });
        }
        for argument in command {
            if argument.len() > MAX_ARGUMENT_BYTES || !placeholders_are_valid(argument) {
                return Err(PythonPreparationError::InvalidPrepareSpec {
                    reason: "invalid argument length or placeholder",
                });
            }
        }
    }
    Ok((bytes, spec.commands))
}

fn freeze_wheelhouse(path: &Path) -> Result<(TempDir, ContentDigest), PythonPreparationError> {
    let source = canonicalize(path, "canonicalize wheelhouse")?;
    if !source.is_dir() {
        return Err(PythonPreparationError::UnsafeWheel { path: source });
    }
    let owner = tempfile::Builder::new()
        .prefix("reprocut-wheelhouse-")
        .tempdir()
        .map_err(|source| PythonPreparationError::Io {
            operation: "create frozen wheelhouse",
            path: std::env::temp_dir(),
            source,
        })?;
    let mut entries = fs::read_dir(&source)
        .map_err(|error| PythonPreparationError::Io {
            operation: "read wheelhouse",
            path: source.clone(),
            source: error,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PythonPreparationError::Io {
            operation: "enumerate wheelhouse",
            path: source.clone(),
            source: error,
        })?;
    entries.sort_unstable_by_key(std::fs::DirEntry::file_name);
    let mut names = BTreeSet::new();
    let mut hasher = ContentHasher::new();
    hasher.update(b"REPROCUT-WHEELHOUSE-V1\0");
    for entry in entries {
        let entry_path = entry.path();
        let metadata =
            fs::symlink_metadata(&entry_path).map_err(|error| PythonPreparationError::Io {
                operation: "inspect wheelhouse entry",
                path: entry_path.clone(),
                source: error,
            })?;
        let name = entry.file_name();
        let Some(name_text) = name.to_str() else {
            return Err(PythonPreparationError::UnsafeWheel { path: entry_path });
        };
        if !metadata.file_type().is_file() || !safe_wheel_name(name_text) {
            return Err(PythonPreparationError::UnsafeWheel { path: entry_path });
        }
        let folded = name_text.to_ascii_lowercase();
        if !names.insert(folded.clone()) {
            return Err(PythonPreparationError::DuplicateWheel { name: folded });
        }
        let bytes = fs::read(&entry_path).map_err(|error| PythonPreparationError::Io {
            operation: "read wheel",
            path: entry_path.clone(),
            source: error,
        })?;
        encode_field(&mut hasher, name_text.as_bytes());
        encode_field(&mut hasher, &bytes);
        fs::write(owner.path().join(&name), &bytes).map_err(|error| {
            PythonPreparationError::Io {
                operation: "copy wheel into frozen corpus",
                path: owner.path().join(name),
                source: error,
            }
        })?;
    }
    Ok((owner, hasher.finalize()))
}

fn probe_interpreter(
    interpreter: &Path,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<Vec<u8>, PythonPreparationError> {
    let command = CommandSpec::new(
        interpreter.to_path_buf(),
        vec![
            OsString::from("-I"),
            OsString::from("-c"),
            OsString::from(INTERPRETER_PROBE),
        ],
        std::env::current_dir().map_err(|source| PythonPreparationError::Io {
            operation: "read current directory",
            path: PathBuf::from("."),
            source,
        })?,
        timeout,
        max_output_bytes,
    )
    .with_environment(scrubbed_environment());
    let observation = ProcessRunner::run(&command)?;
    if !successful(&observation) || observation.streams_truncated() {
        return Err(PythonPreparationError::InterpreterProbe);
    }
    let value: serde_json::Value = serde_json::from_slice(observation.stdout())
        .map_err(|_| PythonPreparationError::InvalidInterpreterIdentity)?;
    if value
        .get("implementation")
        .and_then(serde_json::Value::as_str)
        .is_none()
        || value
            .get("version")
            .and_then(serde_json::Value::as_array)
            .is_none()
        || value
            .get("executable")
            .and_then(serde_json::Value::as_str)
            .is_none()
    {
        return Err(PythonPreparationError::InvalidInterpreterIdentity);
    }
    Ok(observation.stdout().to_vec())
}

fn preparation_digest(
    interpreter: &Path,
    identity: &[u8],
    wheelhouse: ContentDigest,
    extras: &[String],
    prepare_spec: &[u8],
    timeout: Duration,
    max_output_bytes: usize,
) -> ContentDigest {
    let mut hasher = ContentHasher::new();
    hasher.update(b"REPROCUT-PYTHON-PREP-V1\0");
    encode_field(
        &mut hasher,
        interpreter.as_os_str().to_string_lossy().as_bytes(),
    );
    encode_field(&mut hasher, identity);
    hasher.update(wheelhouse.as_bytes());
    for extra in extras {
        encode_field(&mut hasher, extra.as_bytes());
    }
    encode_field(&mut hasher, prepare_spec);
    encode_field(&mut hasher, ENVIRONMENT_POLICY_VERSION.as_bytes());
    encode_field(&mut hasher, b"venv|-I|-m|pip|install|--isolated|--no-index");
    hasher.update(&timeout.as_nanos().to_le_bytes());
    hasher.update(
        &u64::try_from(max_output_bytes)
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    hasher.finalize()
}

fn scrubbed_environment() -> ChildEnvironment {
    const REMOVE: &[&str] = &[
        "PYTHONPATH",
        "PYTHONHOME",
        "PYTHONUSERBASE",
        "PYTHONSTARTUP",
        "PIP_INDEX_URL",
        "PIP_EXTRA_INDEX_URL",
        "PIP_TRUSTED_HOST",
        "VIRTUAL_ENV",
    ];
    let mut environment = ChildEnvironment::inherit();
    for name in REMOVE {
        environment = environment.remove(*name);
    }
    environment
        .set("PYTHONNOUSERSITE", "1")
        .set("PIP_NO_INDEX", "1")
        .set("PIP_DISABLE_PIP_VERSION_CHECK", "1")
        .set("PIP_CONFIG_FILE", null_device())
}

fn candidate_environment(venv: &Path, wheelhouse: &Path, scripts: &Path) -> ChildEnvironment {
    scrubbed_environment()
        .set("VIRTUAL_ENV", venv.as_os_str())
        .set("PIP_FIND_LINKS", wheelhouse.as_os_str())
        .prepend_path(scripts)
}

fn expand_command(
    command: &[String],
    python: &Path,
    candidate: &Path,
    wheelhouse: &Path,
) -> Result<Vec<String>, PythonPreparationError> {
    command
        .iter()
        .map(|argument| {
            let expanded = argument
                .replace("{python}", &python.to_string_lossy())
                .replace("{candidate}", &candidate.to_string_lossy())
                .replace("{wheelhouse}", &wheelhouse.to_string_lossy());
            if expanded.contains('{') || expanded.contains('}') {
                Err(PythonPreparationError::InvalidPrepareSpec {
                    reason: "unknown placeholder",
                })
            } else {
                Ok(expanded)
            }
        })
        .collect()
}

fn placeholders_are_valid(argument: &str) -> bool {
    let stripped = argument
        .replace("{python}", "")
        .replace("{candidate}", "")
        .replace("{wheelhouse}", "");
    !stripped.contains('{') && !stripped.contains('}')
}

fn normalize_extra(name: &str) -> Result<String, PythonPreparationError> {
    if name.is_empty()
        || !name.is_ascii()
        || !name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !name
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        || name
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(PythonPreparationError::InvalidExtra {
            name: name.to_owned(),
        });
    }
    let mut normalized = String::with_capacity(name.len());
    let mut separator = false;
    for character in name.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !separator {
                normalized.push('-');
            }
            separator = true;
        } else {
            normalized.push(character.to_ascii_lowercase());
            separator = false;
        }
    }
    Ok(normalized)
}

fn safe_wheel_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name.ends_with(".whl")
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

fn canonicalize(path: &Path, operation: &'static str) -> Result<PathBuf, PythonPreparationError> {
    path.canonicalize()
        .map_err(|source| PythonPreparationError::Io {
            operation,
            path: path.to_path_buf(),
            source,
        })
}

fn encode_field(hasher: &mut ContentHasher, bytes: &[u8]) {
    hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

fn successful(observation: &reprocut_core::ExecutionObservation) -> bool {
    observation.exit_code() == Some(0)
        && observation.signal().is_none()
        && !observation.timed_out()
        && !observation.streams_truncated()
}

fn is_python_name(name: &str) -> bool {
    let lowercase = name.to_ascii_lowercase();
    lowercase == "python"
        || lowercase == "python.exe"
        || lowercase == "python3"
        || lowercase == "python3.exe"
        || lowercase.strip_prefix("python3.").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

#[cfg(windows)]
fn venv_python(venv: &Path) -> PathBuf {
    venv.join("Scripts/python.exe")
}

#[cfg(not(windows))]
fn venv_python(venv: &Path) -> PathBuf {
    venv.join("bin/python")
}

#[cfg(windows)]
fn venv_scripts(venv: &Path) -> PathBuf {
    venv.join("Scripts")
}

#[cfg(not(windows))]
fn venv_scripts(venv: &Path) -> PathBuf {
    venv.join("bin")
}

#[cfg(windows)]
fn null_device() -> &'static OsStr {
    OsStr::new("NUL")
}

#[cfg(not(windows))]
fn null_device() -> &'static OsStr {
    OsStr::new("/dev/null")
}

#[cfg(test)]
mod tests {
    use super::{
        freeze_wheelhouse, normalize_extra, read_prepare_spec, ContentDigest,
        PythonPreparationError,
    };
    use std::fs;

    #[test]
    fn extras_are_canonical_and_invalid_names_fail_closed() {
        assert_eq!(
            normalize_extra("Fast_JSON.parser").expect("extra"),
            "fast-json-parser"
        );
        assert!(matches!(
            normalize_extra("../escape"),
            Err(PythonPreparationError::InvalidExtra { .. })
        ));
    }

    #[test]
    fn wheelhouse_rejects_non_wheels_and_case_collisions() {
        let wheelhouse = tempfile::tempdir().expect("wheelhouse");
        fs::write(wheelhouse.path().join("demo-1-py3-none-any.whl"), b"wheel").expect("wheel");
        let (_, first) = freeze_wheelhouse(wheelhouse.path()).expect("frozen");
        fs::write(wheelhouse.path().join("DEMO-1-PY3-NONE-ANY.WHL"), b"other").expect("collision");
        assert!(matches!(
            freeze_wheelhouse(wheelhouse.path()),
            Err(PythonPreparationError::DuplicateWheel { .. })
                | Err(PythonPreparationError::UnsafeWheel { .. })
        ));
        fs::remove_file(wheelhouse.path().join("DEMO-1-PY3-NONE-ANY.WHL"))
            .expect("remove collision");
        fs::write(wheelhouse.path().join("README.txt"), b"noise").expect("noise");
        assert!(matches!(
            freeze_wheelhouse(wheelhouse.path()),
            Err(PythonPreparationError::UnsafeWheel { .. })
        ));
        assert_ne!(first, ContentDigest::of(b""));
    }

    #[test]
    fn prepare_spec_is_argv_only_and_placeholder_bounded() {
        let root = tempfile::tempdir().expect("spec root");
        let valid = root.path().join("valid.json");
        fs::write(
            &valid,
            br#"{"schema":1,"commands":[["{python}","-c","print('ok')"]]}"#,
        )
        .expect("valid spec");
        assert_eq!(read_prepare_spec(&valid).expect("valid").1.len(), 1);

        let invalid = root.path().join("invalid.json");
        fs::write(
            &invalid,
            br#"{"schema":1,"commands":[["{shell}","echo ok"]]}"#,
        )
        .expect("invalid spec");
        assert!(matches!(
            read_prepare_spec(&invalid),
            Err(PythonPreparationError::InvalidPrepareSpec { .. })
        ));
    }
}
