//! Bounded child-process execution for `ReproCut`.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use command_group::{CommandGroup, GroupChild};
use reprocut_core::{ContainmentMechanism, ExecutionObservation, TerminationReason};
use thiserror::Error;

/// A bounded command execution request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    program: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    timeout: Duration,
    max_output_bytes: usize,
    environment: ChildEnvironment,
}

impl CommandSpec {
    /// Creates a complete, immutable command request.
    pub fn new(
        program: PathBuf,
        arguments: Vec<OsString>,
        working_directory: PathBuf,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Self {
        Self {
            program,
            arguments,
            working_directory,
            timeout,
            max_output_bytes,
            environment: ChildEnvironment::inherit(),
        }
    }

    /// Replaces the default inherited child-environment policy.
    #[must_use]
    pub fn with_environment(mut self, environment: ChildEnvironment) -> Self {
        self.environment = environment;
        self
    }

    /// Returns the executable path.
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Returns the exact argument vector.
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the child working directory.
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    /// Returns the execution deadline.
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the per-stream byte budget.
    pub const fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }

    /// Returns the immutable environment mutation contract.
    pub const fn environment(&self) -> &ChildEnvironment {
        &self.environment
    }
}

/// Deterministic environment mutations applied directly to a child command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildEnvironment {
    clear: bool,
    set: BTreeMap<OsString, OsString>,
    remove: BTreeSet<OsString>,
    path_prepend: Vec<PathBuf>,
}

impl ChildEnvironment {
    /// Starts from the parent process environment.
    pub const fn inherit() -> Self {
        Self {
            clear: false,
            set: BTreeMap::new(),
            remove: BTreeSet::new(),
            path_prepend: Vec::new(),
        }
    }

    /// Starts from an empty child environment.
    pub const fn cleared() -> Self {
        Self {
            clear: true,
            set: BTreeMap::new(),
            remove: BTreeSet::new(),
            path_prepend: Vec::new(),
        }
    }

    /// Sets one exact variable after inherited removals are applied.
    #[must_use]
    pub fn set(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        let name = name.into();
        self.remove.remove(&name);
        self.set.insert(name, value.into());
        self
    }

    /// Removes one exact variable and any earlier explicit setting for it.
    #[must_use]
    pub fn remove(mut self, name: impl Into<OsString>) -> Self {
        let name = name.into();
        self.set.remove(&name);
        self.remove.insert(name);
        self
    }

    /// Prepends one directory to PATH while retaining its policy-selected tail.
    #[must_use]
    pub fn prepend_path(mut self, directory: impl Into<PathBuf>) -> Self {
        self.path_prepend.push(directory.into());
        self
    }

    /// Reports whether the parent environment is discarded.
    pub const fn clears_parent(&self) -> bool {
        self.clear
    }

    /// Returns explicit settings in lexical platform order.
    pub const fn settings(&self) -> &BTreeMap<OsString, OsString> {
        &self.set
    }

    /// Returns explicit removals in lexical platform order.
    pub const fn removals(&self) -> &BTreeSet<OsString> {
        &self.remove
    }

    /// Returns PATH prefixes in caller order.
    pub fn path_prefixes(&self) -> &[PathBuf] {
        &self.path_prepend
    }
}

impl Default for ChildEnvironment {
    fn default() -> Self {
        Self::inherit()
    }
}

/// A process-spawn, wait, or capture failure.
#[derive(Debug, Error)]
pub enum RunnerError {
    /// An operating-system operation failed.
    #[error("{operation} failed: {source}")]
    Io {
        /// Failed operation.
        operation: &'static str,
        /// Operating-system error.
        #[source]
        source: io::Error,
    },
    /// A stream reader thread panicked.
    #[error("{stream} capture thread panicked")]
    CaptureThread {
        /// Stream being captured.
        stream: &'static str,
    },
    /// PATH entries could not be encoded for the current platform.
    #[error("invalid child environment: {source}")]
    InvalidEnvironment {
        /// Invalid PATH composition.
        #[source]
        source: std::env::JoinPathsError,
    },
}

/// Executes child processes with bounded evidence capture.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessRunner;

impl ProcessRunner {
    /// Runs a command, drains both pipes, and always reaps its contained process group.
    ///
    /// # Errors
    ///
    /// Returns [`RunnerError`] when the child cannot be spawned, waited on, terminated, or read;
    /// when a capture thread panics; or when the requested child environment is invalid.
    pub fn run(spec: &CommandSpec) -> Result<ExecutionObservation, RunnerError> {
        let mut command = Command::new(spec.program());
        command
            .args(spec.arguments())
            .current_dir(spec.working_directory())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        apply_environment(&mut command, spec.environment())?;
        let mut child = ContainedChild::spawn(&mut command).map_err(|source| RunnerError::Io {
            operation: "spawn child",
            source,
        })?;
        let stdout = child.inner().stdout.take().ok_or_else(|| RunnerError::Io {
            operation: "capture child stdout",
            source: io::Error::new(io::ErrorKind::BrokenPipe, "stdout pipe was not created"),
        })?;
        let stderr = child.inner().stderr.take().ok_or_else(|| RunnerError::Io {
            operation: "capture child stderr",
            source: io::Error::new(io::ErrorKind::BrokenPipe, "stderr pipe was not created"),
        })?;
        let stdout_limit = spec.max_output_bytes();
        let stderr_limit = spec.max_output_bytes();
        let stdout_reader = thread::spawn(move || read_bounded(stdout, stdout_limit));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, stderr_limit));

        let (status, timed_out) = if let Some(status) = wait_until(&mut child, spec.timeout())
            .map_err(|source| RunnerError::Io {
                operation: "wait for child",
                source,
            })? {
            (status, false)
        } else {
            let status = child.terminate().map_err(|source| RunnerError::Io {
                operation: "terminate timed-out process group",
                source,
            })?;
            (status, true)
        };

        let (stdout, stdout_truncated) = join_capture(stdout_reader, "stdout")?;
        let (stderr, stderr_truncated) = join_capture(stderr_reader, "stderr")?;

        Ok(ExecutionObservation::new_contained(
            termination_reason(status, timed_out),
            stdout,
            stderr,
            stdout_truncated || stderr_truncated,
            containment_mechanism(),
        ))
    }
}

fn apply_environment(
    command: &mut Command,
    environment: &ChildEnvironment,
) -> Result<(), RunnerError> {
    if environment.clears_parent() {
        command.env_clear();
    }
    for name in environment.removals() {
        command.env_remove(name);
    }
    command.envs(environment.settings());

    if environment.path_prefixes().is_empty() {
        return Ok(());
    }
    let path_name = OsStr::new("PATH");
    let explicit_path = environment.settings().get(path_name);
    let inherited_path = (!environment.clears_parent()
        && !environment.removals().contains(path_name))
    .then(|| std::env::var_os(path_name))
    .flatten();
    let tail = explicit_path.cloned().or(inherited_path);
    let mut entries = environment.path_prefixes().to_vec();
    if let Some(tail) = tail {
        entries.extend(std::env::split_paths(&tail));
    }
    let joined = std::env::join_paths(entries)
        .map_err(|source| RunnerError::InvalidEnvironment { source })?;
    command.env(path_name, joined);
    Ok(())
}

/// Returns the OS-level primitive used to own descendant processes.
pub const fn containment_mechanism() -> ContainmentMechanism {
    #[cfg(unix)]
    {
        ContainmentMechanism::PosixProcessGroup
    }
    #[cfg(windows)]
    {
        ContainmentMechanism::WindowsJobObject
    }
    #[cfg(not(any(unix, windows)))]
    {
        ContainmentMechanism::DirectChild
    }
}

struct ContainedChild {
    child: GroupChild,
    finished: bool,
}

impl ContainedChild {
    fn spawn(command: &mut Command) -> io::Result<Self> {
        Ok(Self {
            child: spawn_group(command)?,
            finished: false,
        })
    }

    fn inner(&mut self) -> &mut std::process::Child {
        self.child.inner()
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        let status = self.child.try_wait()?;
        self.finished |= status.is_some();
        Ok(status)
    }

    fn terminate(&mut self) -> io::Result<std::process::ExitStatus> {
        match self.child.kill() {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::InvalidInput => {}
            Err(source) => return Err(source),
        }
        let status = self.child.wait()?;
        self.finished = true;
        Ok(status)
    }
}

impl Drop for ContainedChild {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.finished = true;
    }
}

#[cfg(windows)]
fn spawn_group(command: &mut Command) -> io::Result<GroupChild> {
    command.group().kill_on_drop(true).spawn()
}

#[cfg(not(windows))]
fn spawn_group(command: &mut Command) -> io::Result<GroupChild> {
    command.group_spawn()
}

fn wait_until(
    child: &mut ContainedChild,
    timeout: Duration,
) -> io::Result<Option<std::process::ExitStatus>> {
    const POLL_INTERVAL: Duration = Duration::from_millis(5);
    let started = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            return Ok(None);
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(elapsed)));
    }
}

fn read_bounded<R: Read>(mut reader: R, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::with_capacity(limit.min(8 * 1_024));
    let mut chunk = [0_u8; 8 * 1_024];
    let mut truncated = false;

    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let retained = limit.saturating_sub(captured.len()).min(read);
        captured.extend_from_slice(&chunk[..retained]);
        truncated |= retained < read;
    }

    Ok((captured, truncated))
}

fn join_capture(
    handle: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
    stream: &'static str,
) -> Result<(Vec<u8>, bool), RunnerError> {
    handle
        .join()
        .map_err(|_| RunnerError::CaptureThread { stream })?
        .map_err(|source| RunnerError::Io {
            operation: "read child stream",
            source,
        })
}

#[cfg(unix)]
fn exit_signal(status: std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
const fn exit_signal(_status: std::process::ExitStatus) -> Option<i32> {
    None
}

fn termination_reason(status: std::process::ExitStatus, timed_out: bool) -> TerminationReason {
    if timed_out {
        TerminationReason::TimedOut
    } else if let Some(signal) = exit_signal(status) {
        TerminationReason::UnixSignal(signal)
    } else if let Some(code) = status.code() {
        TerminationReason::ExitCode(code)
    } else {
        TerminationReason::RunnerFailure
    }
}
