//! Reports the engine's own preflight without reducing anything.
//!
//! Every judgement here comes from [`reprocut_engine::ReductionEngine::preflight`], which is the code path
//! a reduction runs before its first cut. This module decides how to print that, and
//! nothing else. It does not re-derive whether a failure is stable, and it does not guess
//! what a diagnostic means.

use std::process::ExitCode;

use reprocut_core::{
    DiagnosticChannel, ExecutionObservation, FailureOracle, OracleError, OracleMode,
};
use reprocut_engine::{EngineError, Preflight, PreflightVerdict, ReductionRequest};
use serde::Serialize;

use crate::{CliError, ReduceArgs};

/// Version of the `--json` document this command emits.
const DOCTOR_SCHEMA_VERSION: u16 = 1;

/// Captured output lines shown for each stream of each run.
///
/// Command output can carry paths, hostnames, tokens, and customer data. Doctor shows a
/// bounded tail so a reader can recognize their own failure, not a transcript.
const TAIL_LINES: usize = 10;

/// Bytes kept from each shown line.
const TAIL_LINE_BYTES: usize = 240;

/// Runs the preflight and reports it. Returns the process exit code.
pub(crate) fn report(arguments: &ReduceArgs, request: &ReductionRequest) -> ExitCode {
    let report = match reprocut_engine::ReductionEngine::preflight(request) {
        Ok(preflight) => DoctorReport::from_preflight(arguments, request, &preflight),
        Err(error) => DoctorReport::from_setup_error(arguments, request, &error),
    };
    if arguments.json {
        match serde_json::to_string_pretty(&report) {
            Ok(document) => println!("{document}"),
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        report.print();
    }
    if report.status == Status::Ready {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    /// Reduction may begin.
    Ready,
    /// Reduction may not begin, for the reported reason.
    NotReady,
}

/// Why the preflight ended as it did. Stable across releases within a schema version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReasonCode {
    /// The failure is stable and an oracle recognizes it.
    StableFailure,
    /// A preparation command did not succeed, so the command never ran.
    PreparationFailed,
    /// The command completed successfully where the oracle mode requires a failure.
    CommandSucceeded,
    /// The observations were taken but no oracle could be stabilized from them.
    NoStableOracle,
    /// The command could not be executed at all.
    CommandNotExecuted,
    /// The project contains no regular files to reduce.
    EmptyProject,
    /// The project could not be read, or isolated preparation could not be captured.
    SetupFailed,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    schema_version: u16,
    status: Status,
    reason_code: ReasonCode,
    detail: Option<String>,
    root: String,
    command: Command,
    oracle: Oracle,
    runs: Runs,
    /// What the engine will treat as the failure, present only when it stabilized one.
    #[serde(skip_serializing_if = "Option::is_none")]
    recognized: Option<Recognized>,
    /// Which oracle rule was not met. `detail` already carries its message for readers.
    #[serde(skip)]
    oracle_error: Option<OracleError>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    observations: Vec<Observation>,
    output_disclosure: Disclosure,
}

/// The failure contract the engine stabilized, in the engine's own normalized terms.
#[derive(Debug, Serialize)]
struct Recognized {
    exit_code: Option<i32>,
    signal: Option<i32>,
    /// Normalized lines every candidate must keep printing.
    anchors: Vec<Anchor>,
    /// Caller-supplied expressions every candidate must keep matching.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required_patterns: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Anchor {
    channel: &'static str,
    text: String,
}

#[derive(Debug, Serialize)]
struct Command {
    /// The program as the caller wrote it.
    requested: String,
    /// The program the runner was given, which isolated preparation may rewrite.
    executed: Option<String>,
    arguments: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Oracle {
    mode: &'static str,
    channel: &'static str,
}

#[derive(Debug, Serialize)]
struct Runs {
    /// How many observations the evaluation policy asked for.
    planned: u16,
    /// How many were taken. Fewer means the preflight ended early.
    completed: usize,
}

#[derive(Debug, Serialize)]
struct Observation {
    run: usize,
    exit_code: Option<i32>,
    signal: Option<i32>,
    timed_out: bool,
    streams_truncated: bool,
    stdout_tail: Vec<String>,
    stderr_tail: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Disclosure {
    lines_per_stream: usize,
    bytes_per_line: usize,
}

impl DoctorReport {
    fn from_preflight(
        arguments: &ReduceArgs,
        request: &ReductionRequest,
        preflight: &Preflight,
    ) -> Self {
        let mut recognized = None;
        let mut oracle_error = None;
        let (status, reason_code, detail) = match preflight.verdict() {
            PreflightVerdict::Ready(oracle) => {
                recognized = Some(Recognized::new(oracle));
                (Status::Ready, ReasonCode::StableFailure, None)
            }
            PreflightVerdict::PreparationRejected => {
                (Status::NotReady, ReasonCode::PreparationFailed, None)
            }
            PreflightVerdict::CommandSucceeded => {
                (Status::NotReady, ReasonCode::CommandSucceeded, None)
            }
            PreflightVerdict::NoStableOracle(error) => {
                oracle_error = Some(*error);
                (
                    Status::NotReady,
                    ReasonCode::NoStableOracle,
                    Some(error.to_string()),
                )
            }
        };
        // A stabilized oracle already says what the failure is, and says it in the terms
        // the reduction will use. Repeating three transcripts underneath it would bury
        // that answer in the output it was distilled from.
        let observations = if status == Status::Ready {
            Vec::new()
        } else {
            preflight
                .observations()
                .iter()
                .enumerate()
                .map(|(index, observation)| Observation::new(index + 1, observation))
                .collect()
        };
        Self {
            schema_version: DOCTOR_SCHEMA_VERSION,
            status,
            reason_code,
            detail,
            root: arguments.root.display().to_string(),
            command: Command {
                requested: preflight.requested_program().display().to_string(),
                executed: preflight
                    .executed_program()
                    .map(|program| program.display().to_string()),
                arguments: argument_strings(request),
            },
            oracle: Oracle {
                mode: mode_name(preflight.mode()),
                channel: channel_name(preflight.channel()),
            },
            runs: Runs {
                planned: preflight.runs_planned(),
                completed: preflight.observations().len(),
            },
            recognized,
            oracle_error,
            observations,
            output_disclosure: Disclosure {
                lines_per_stream: TAIL_LINES,
                bytes_per_line: TAIL_LINE_BYTES,
            },
        }
    }

    /// Reports a request whose command never produced an observation.
    ///
    /// The engine failed before any verdict existed, so there is nothing to show but what
    /// was asked for and what the operating system said about it.
    fn from_setup_error(
        arguments: &ReduceArgs,
        request: &ReductionRequest,
        error: &EngineError,
    ) -> Self {
        let reason_code = match error {
            EngineError::EmptyProject => ReasonCode::EmptyProject,
            EngineError::Runner(_) => ReasonCode::CommandNotExecuted,
            _ => ReasonCode::SetupFailed,
        };
        Self {
            schema_version: DOCTOR_SCHEMA_VERSION,
            status: Status::NotReady,
            reason_code,
            detail: Some(error.to_string()),
            root: arguments.root.display().to_string(),
            command: Command {
                requested: request.program().display().to_string(),
                executed: None,
                arguments: argument_strings(request),
            },
            oracle: Oracle {
                mode: mode_name(request.oracle_spec().mode()),
                channel: channel_name(request.oracle_spec().channel()),
            },
            runs: Runs {
                planned: request.evaluation_policy().runs(),
                completed: 0,
            },
            recognized: None,
            oracle_error: None,
            observations: Vec::new(),
            output_disclosure: Disclosure {
                lines_per_stream: TAIL_LINES,
                bytes_per_line: TAIL_LINE_BYTES,
            },
        }
    }

    fn print(&self) {
        println!("project   {}", self.root);
        println!("command   {}", self.command.line());
        if let Some(executed) = &self.command.executed {
            if executed != &self.command.requested {
                println!("resolved  {executed}");
            }
        }
        println!(
            "oracle    {} mode, {} stream, {} of {} runs",
            self.oracle.mode, self.oracle.channel, self.runs.completed, self.runs.planned
        );
        println!();
        for observation in &self.observations {
            observation.print();
        }
        match &self.detail {
            Some(detail) => println!("{} {detail}", marker(self.status)),
            None => println!("{} {}", marker(self.status), self.headline()),
        }
        if let Some(recognized) = &self.recognized {
            recognized.print();
        }
        println!();
        for line in self.guidance() {
            println!("{line}");
        }
        println!();
        println!("A passing preflight does not guarantee that reduction will succeed.");
    }

    fn headline(&self) -> &'static str {
        match self.reason_code {
            ReasonCode::StableFailure => "the failure is stable and recognizable",
            ReasonCode::PreparationFailed => "candidate preparation did not succeed",
            ReasonCode::CommandSucceeded => "the command completed successfully",
            ReasonCode::NoStableOracle => "no stable failure diagnosis was found",
            ReasonCode::CommandNotExecuted => "the command could not be executed",
            ReasonCode::EmptyProject => "the project contains no files to reduce",
            ReasonCode::SetupFailed => "the project could not be prepared",
        }
    }

    /// Says what the reader can do next, without choosing a failure contract for them.
    fn guidance(&self) -> Vec<String> {
        match self.reason_code {
            ReasonCode::StableFailure => vec!["Reduction can start.".to_owned()],
            ReasonCode::PreparationFailed => vec![
                "Reduction was not started.".to_owned(),
                "The preparation step for this ecosystem failed before the command ran.".to_owned(),
                "Check that dependencies install offline, or pass --prepare none.".to_owned(),
            ],
            ReasonCode::CommandSucceeded => vec![
                "Reduction was not started.".to_owned(),
                format!(
                    "The {} oracle mode requires the command to fail.",
                    self.oracle.mode
                ),
                "The failure may already be fixed, or the command may not be the failing one."
                    .to_owned(),
                "For a command that must keep exiting zero, use --oracle-mode exit-zero."
                    .to_owned(),
            ],
            ReasonCode::NoStableOracle => {
                let mut lines = vec!["Reduction was not started.".to_owned()];
                lines.extend(
                    oracle_guidance(self.oracle_error)
                        .iter()
                        .map(|line| (*line).to_owned()),
                );
                lines
            }
            ReasonCode::CommandNotExecuted => vec![
                "Reduction was not started.".to_owned(),
                "The command could not be started, so nothing was observed. The message".to_owned(),
                "above is what the operating system reported about it.".to_owned(),
            ],
            ReasonCode::EmptyProject | ReasonCode::SetupFailed => {
                vec!["Reduction was not started.".to_owned()]
            }
        }
    }
}

/// Says what the unmet oracle rule means and what the reader can change about it.
///
/// The rules fail for different reasons and the difference is the whole diagnosis. Runs
/// that print nothing in common and runs that print the same unusable line are opposite
/// situations; one message for both would describe neither.
const fn oracle_guidance(error: Option<OracleError>) -> &'static [&'static str] {
    let Some(error) = error else {
        return &[];
    };
    match error {
        OracleError::IncompleteBaseline => &[
            "A run timed out or filled its capture budget, so its evidence is partial.",
            "Raise --timeout-ms, or --max-output-bytes for a very loud command.",
        ],
        OracleError::UnstableExitState => &[
            "The runs above did not end the same way, so there is no single exit state",
            "to preserve. If the command is genuinely intermittent, evaluate it by",
            "supermajority instead: --flaky.",
        ],
        OracleError::UnstableDiagnostic => &[
            "No line was printed identically by every run. Compare the runs above; if the",
            "parts that differ are not what the failure is about, name the stable part",
            "yourself:",
            "  --oracle-mode regex --failure-regex '<expression>'",
        ],
        OracleError::EmptyAnchor => &[
            "Every run printed the same thing, but nothing in it survives normalization as",
            "a failure-bearing line. That is common when the output is an environment",
            "message rather than a failure from your code: read the runs above and check",
            "that the command ran what you meant it to run.",
            "To reduce against that output as it stands, name the line yourself:",
            "  --oracle-mode regex --failure-regex '<expression>'",
        ],
        OracleError::TooFewBaselines => {
            &["An oracle needs at least two independent observations of the failure."]
        }
        OracleError::BaselinePatternMismatch => &[
            "A --failure-regex you supplied does not match every run above. It has to hold",
            "for the unreduced project before it can hold for anything smaller.",
        ],
        OracleError::BaselineUnexpectedReject => &[
            "A --reject-regex you supplied matches the unreduced project, so every",
            "candidate would be rejected including the original.",
        ],
        OracleError::ExitZeroBaselineRequired => &[
            "Exit-zero mode preserves a successful command, but a run above did not exit",
            "zero.",
        ],
        OracleError::InvalidConfiguration
        | OracleError::InvalidPattern
        | OracleError::PatternTooLong
        | OracleError::TooManyPatterns => &["Correct the oracle arguments and run doctor again."],
    }
}

impl Recognized {
    fn new(oracle: &FailureOracle) -> Self {
        let fingerprint = oracle.fingerprint();
        Self {
            exit_code: fingerprint.exit_code(),
            signal: fingerprint.signal(),
            anchors: fingerprint
                .anchors()
                .iter()
                .take(TAIL_LINES)
                .map(|anchor| Anchor {
                    channel: channel_name(anchor.channel()),
                    text: clip(anchor.text()),
                })
                .collect(),
            required_patterns: fingerprint.failure_patterns().to_vec(),
        }
    }

    fn print(&self) {
        let state = match (self.exit_code, self.signal) {
            (_, Some(signal)) => format!("signal {signal}"),
            (Some(code), None) => format!("exit {code}"),
            (None, None) => "the same termination".to_owned(),
        };
        println!("  every candidate must keep ending with {state} and printing:");
        for anchor in &self.anchors {
            println!("    [{}] {}", anchor.channel, anchor.text);
        }
        for pattern in &self.required_patterns {
            println!("    [regex] {pattern}");
        }
    }
}

impl Command {
    fn line(&self) -> String {
        let mut line = self.requested.clone();
        for argument in &self.arguments {
            line.push(' ');
            line.push_str(argument);
        }
        line
    }
}

impl Observation {
    fn new(run: usize, observation: &ExecutionObservation) -> Self {
        Self {
            run,
            exit_code: observation.exit_code(),
            signal: observation.signal(),
            timed_out: observation.timed_out(),
            streams_truncated: observation.streams_truncated(),
            stdout_tail: tail(observation.stdout()),
            stderr_tail: tail(observation.stderr()),
        }
    }

    fn print(&self) {
        let mut state = match (self.exit_code, self.signal) {
            (_, Some(signal)) => format!("signal {signal}"),
            (Some(code), None) => format!("exit {code}"),
            (None, None) => "no exit status".to_owned(),
        };
        if self.timed_out {
            state.push_str(", timed out");
        }
        if self.streams_truncated {
            state.push_str(", output truncated");
        }
        println!("run {}  {state}", self.run);
        for line in self.stderr_tail.iter().chain(&self.stdout_tail) {
            println!("  {line}");
        }
        println!();
    }
}

/// Returns the last shown lines of one captured stream, each clipped to its byte budget.
fn tail(stream: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(stream);
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    lines[lines.len().saturating_sub(TAIL_LINES)..]
        .iter()
        .map(|line| clip(line))
        .collect()
}

/// Clips one line on a character boundary so the result is always printable.
fn clip(line: &str) -> String {
    if line.len() <= TAIL_LINE_BYTES {
        return line.to_owned();
    }
    let mut end = TAIL_LINE_BYTES;
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &line[..end])
}

fn argument_strings(request: &ReductionRequest) -> Vec<String> {
    request
        .arguments()
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

const fn marker(status: Status) -> &'static str {
    match status {
        Status::Ready => "ok:",
        Status::NotReady => "no:",
    }
}

const fn mode_name(mode: OracleMode) -> &'static str {
    match mode {
        OracleMode::Automatic => "automatic",
        OracleMode::Regex => "regex",
        OracleMode::ExitZero => "exit-zero",
    }
}

const fn channel_name(channel: DiagnosticChannel) -> &'static str {
    match channel {
        DiagnosticChannel::Auto => "auto",
        DiagnosticChannel::Stderr => "stderr",
        DiagnosticChannel::Stdout => "stdout",
        DiagnosticChannel::Combined => "combined",
    }
}

/// Builds the request and reports its preflight.
pub(crate) fn run(mut arguments: ReduceArgs) -> Result<ExitCode, CliError> {
    let request = crate::build_request(&mut arguments, false, crate::RequestPurpose::Inspect)?;
    Ok(report(&arguments, &request))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::{clip, tail, TAIL_LINES, TAIL_LINE_BYTES};

    #[test]
    fn tail_keeps_only_the_last_lines_and_drops_blank_ones() {
        let mut stream = String::new();
        for index in 0..40 {
            let _ = writeln!(stream, "line {index}\n");
        }
        let lines = tail(stream.as_bytes());
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines.last().map(String::as_str), Some("line 39"));
    }

    #[test]
    fn tail_of_an_empty_stream_is_empty() {
        assert!(tail(b"").is_empty());
        assert!(tail(b"\n  \n").is_empty());
    }

    #[test]
    fn clip_never_splits_a_character() {
        let line = "é".repeat(TAIL_LINE_BYTES);
        let clipped = clip(&line);
        assert!(clipped.ends_with("..."));
        assert!(clipped.len() <= TAIL_LINE_BYTES + 3);
    }
}
