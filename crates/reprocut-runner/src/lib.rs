//! Bounded child-process execution for ReproCut.

use std::{
    ffi::OsString,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use reprocut_core::ExecutionObservation;
use thiserror::Error;

/// A bounded command execution request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    program: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    timeout: Duration,
    max_output_bytes: usize,
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
        }
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
}

/// Executes child processes with bounded evidence capture.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessRunner;

impl ProcessRunner {
    /// Runs a command, drains both pipes, and always reaps the direct child.
    pub fn run(spec: &CommandSpec) -> Result<ExecutionObservation, RunnerError> {
        let mut child = Command::new(spec.program())
            .args(spec.arguments())
            .current_dir(spec.working_directory())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| RunnerError::Io {
                operation: "spawn child",
                source,
            })?;
        let stdout = child.stdout.take().ok_or_else(|| RunnerError::Io {
            operation: "capture child stdout",
            source: io::Error::new(io::ErrorKind::BrokenPipe, "stdout pipe was not created"),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| RunnerError::Io {
            operation: "capture child stderr",
            source: io::Error::new(io::ErrorKind::BrokenPipe, "stderr pipe was not created"),
        })?;
        let stdout_limit = spec.max_output_bytes();
        let stderr_limit = spec.max_output_bytes();
        let stdout_reader = thread::spawn(move || read_bounded(stdout, stdout_limit));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, stderr_limit));

        let (status, timed_out) = match wait_until(&mut child, spec.timeout()).map_err(|source| {
            RunnerError::Io {
                operation: "wait for child",
                source,
            }
        })? {
            Some(status) => (status, false),
            None => {
                if let Err(source) = child.kill() {
                    if source.kind() != io::ErrorKind::InvalidInput {
                        return Err(RunnerError::Io {
                            operation: "kill timed-out child",
                            source,
                        });
                    }
                }
                let status = child.wait().map_err(|source| RunnerError::Io {
                    operation: "reap timed-out child",
                    source,
                })?;
                (status, true)
            }
        };

        let (stdout, stdout_truncated) = join_capture(stdout_reader, "stdout")?;
        let (stderr, stderr_truncated) = join_capture(stderr_reader, "stderr")?;

        Ok(ExecutionObservation::new(
            status.code(),
            exit_signal(&status),
            stdout,
            stderr,
            timed_out,
            stdout_truncated || stderr_truncated,
        ))
    }
}

fn wait_until(
    child: &mut std::process::Child,
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
fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
const fn exit_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}
