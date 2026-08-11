#![forbid(unsafe_code)]

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use reprocut_adapters::{Adapter, AdapterError, Ecosystem, EcosystemSelection};
use reprocut_core::{
    DiagnosticAnchor, DiagnosticChannel, EvaluationPolicy, PolicyError, TerminationReason,
};
use reprocut_engine::{
    EngineError, PreparationMode, ReductionEngine, ReductionOutcome, ReductionRequest, SessionMode,
};
use reprocut_report::{render_report, ReportModel};
use reprocut_workspace::WorkspaceError;
use serde::Serialize;
use tempfile::TempDir;
use thiserror::Error;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_CAPTURE_BYTES: usize = 1_048_576;
const DEFAULT_FLAKY_RUNS: u16 = 11;
const DEFAULT_FLAKY_REQUIRED: u16 = 9;

#[derive(Debug, Parser)]
#[command(
    name = "reprocut",
    version,
    about = "Shrink a failing project without changing its failure",
    after_help = "Start with: reprocut minimize --root ./failing-project"
)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    /// Auto-detect, prove, minimize, and re-verify one failing project.
    Minimize(ReduceArgs),
    /// Prove, minimize, and re-verify one failing command.
    Reduce(ReduceArgs),
    /// Continue an exactly compatible interrupted reduction.
    Resume(ReduceArgs),
}

#[derive(Debug, Args)]
struct ReduceArgs {
    /// Project directory to minimize.
    #[arg(long, default_value = ".")]
    root: PathBuf,

    /// New directory that receives the reduced reproduction.
    #[arg(short, long, default_value = "reprocut-output")]
    output: PathBuf,

    /// Project adapter used for commands, exclusions, manifests, and syntax.
    #[arg(long, value_enum, default_value_t = EcosystemArg::Auto)]
    ecosystem: EcosystemArg,

    /// Candidate preparation authority; isolated-python trusts your command environment.
    #[arg(long, value_enum, default_value_t = PrepareArg::Offline)]
    prepare: PrepareArg,

    /// Deadline for each candidate execution.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
    timeout_ms: u64,

    /// Maximum captured bytes for each child output stream.
    #[arg(long, default_value_t = DEFAULT_CAPTURE_BYTES)]
    max_output_bytes: usize,

    /// Process stream used to identify the stabilized failure.
    #[arg(long, value_enum, default_value_t = OracleStreamArg::Auto)]
    oracle_stream: OracleStreamArg,

    /// Evaluate each failure using a validated repeated-run supermajority.
    #[arg(long)]
    flaky: bool,

    /// Maximum observations used in flaky mode (odd, 5..=101).
    #[arg(long, requires = "flaky")]
    flaky_runs: Option<u16>,

    /// Preserved observations required in flaky mode (at least two thirds).
    #[arg(long, requires = "flaky")]
    flaky_required: Option<u16>,

    /// Emit one machine-readable JSON value on standard output.
    #[arg(long)]
    json: bool,

    /// Maximum simultaneous candidate commands; zero uses detected hardware parallelism.
    #[arg(long, default_value_t = 0)]
    jobs: usize,

    /// SQLite journal path (defaults to ROOT/.reprocut/state.sqlite3).
    #[arg(long)]
    state: Option<PathBuf>,

    /// Start a new session without deleting prior journal history.
    #[arg(long)]
    restart: bool,

    /// Optional failing command after `--`; otherwise the adapter supplies one.
    #[arg(last = true, num_args = 0.., value_name = "COMMAND")]
    command: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum EcosystemArg {
    Auto,
    Cargo,
    Python,
    Npm,
    None,
}

impl EcosystemArg {
    const fn selection(self) -> EcosystemSelection {
        match self {
            Self::Auto => EcosystemSelection::Auto,
            Self::Cargo => EcosystemSelection::Explicit(Ecosystem::Cargo),
            Self::Python => EcosystemSelection::Explicit(Ecosystem::Python),
            Self::Npm => EcosystemSelection::Explicit(Ecosystem::Npm),
            Self::None => EcosystemSelection::Explicit(Ecosystem::None),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cargo => "cargo",
            Self::Python => "python",
            Self::Npm => "npm",
            Self::None => "none",
        }
    }
}

impl From<Ecosystem> for EcosystemArg {
    fn from(value: Ecosystem) -> Self {
        match value {
            Ecosystem::Cargo => Self::Cargo,
            Ecosystem::Python => Self::Python,
            Ecosystem::Npm => Self::Npm,
            Ecosystem::None => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum PrepareArg {
    None,
    Offline,
    LifecycleScripts,
    IsolatedPython,
}

impl From<PrepareArg> for PreparationMode {
    fn from(value: PrepareArg) -> Self {
        match value {
            PrepareArg::None => Self::None,
            PrepareArg::Offline => Self::Offline,
            PrepareArg::LifecycleScripts => Self::LifecycleScripts,
            PrepareArg::IsolatedPython => Self::IsolatedPython,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OracleStreamArg {
    Auto,
    Stderr,
    Stdout,
    Combined,
}

impl From<OracleStreamArg> for DiagnosticChannel {
    fn from(value: OracleStreamArg) -> Self {
        match value {
            OracleStreamArg::Auto => Self::Auto,
            OracleStreamArg::Stderr => Self::Stderr,
            OracleStreamArg::Stdout => Self::Stdout,
            OracleStreamArg::Combined => Self::Combined,
        }
    }
}

#[derive(Debug, Error)]
enum CliError {
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("output path already exists: {0}")]
    OutputExists(PathBuf),
    #[error("output path has no usable parent: {0}")]
    InvalidOutput(PathBuf),
    #[error("{operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("serialize reduction state: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("{0}")]
    InvalidArguments(&'static str),
}

#[derive(Clone, Debug, Serialize)]
struct ReductionSummary {
    schema_version: u8,
    source_root: String,
    output: String,
    command: Vec<String>,
    ecosystem: &'static str,
    prepare: PrepareArg,
    original_files: usize,
    retained_files: usize,
    attempts: u64,
    file_attempts: u64,
    structured_attempts: u64,
    baseline_runs: u16,
    final_verifications: u16,
    inconclusive_attempts: u64,
    cache_hits: u64,
    jobs: usize,
    state: Option<String>,
    resumed: bool,
    oracle_stream: DiagnosticChannel,
    evaluation_policy: PolicySummary,
    kept_files: Vec<String>,
    accepted_structured_edits: Vec<String>,
    fingerprint: FingerprintSummary,
}

#[derive(Clone, Debug, Serialize)]
struct FingerprintSummary {
    exit_code: Option<i32>,
    signal: Option<i32>,
    termination: TerminationReason,
    anchor: String,
    anchors: Vec<DiagnosticAnchor>,
    normalization_schema: u16,
}

#[derive(Clone, Debug, Serialize)]
struct PolicySummary {
    mode: &'static str,
    runs: u16,
    required: u16,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> Result<(), CliError> {
    match cli.action {
        Action::Minimize(arguments) => reduce_project(arguments, false),
        Action::Reduce(arguments) => reduce_project(arguments, false),
        Action::Resume(arguments) => reduce_project(arguments, true),
    }
}

fn reduce_project(mut arguments: ReduceArgs, resume: bool) -> Result<(), CliError> {
    if resume && arguments.restart {
        return Err(CliError::InvalidArguments(
            "resume and --restart are mutually exclusive",
        ));
    }
    let evaluation_policy = evaluation_policy(&arguments)?;
    ensure_output_absent(&arguments.output)?;
    let adapter = Adapter::detect(&arguments.root, arguments.ecosystem.selection())?;
    arguments.ecosystem = adapter.ecosystem().into();
    if arguments.command.is_empty() {
        let command = adapter.command().ok_or(CliError::InvalidArguments(
            "the selected ecosystem has no default command; pass one after --",
        ))?;
        let mut resolved = Vec::with_capacity(command.arguments().len().saturating_add(1));
        resolved.push(command.program().to_string_lossy().into_owned());
        resolved.extend(
            command
                .arguments()
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned()),
        );
        arguments.command = resolved;
    }
    let (program, child_arguments) = split_command(&arguments.command);
    let request = ReductionRequest::new(
        arguments.root.clone(),
        PathBuf::from(program),
        child_arguments.iter().map(OsString::from).collect(),
        Duration::from_millis(arguments.timeout_ms),
        arguments.max_output_bytes,
    )
    .with_evaluation(arguments.oracle_stream.into(), evaluation_policy)
    .with_runtime(arguments.jobs, session_mode(&arguments, resume))
    .with_inventory_policy(adapter.inventory_policy().clone())
    .with_ecosystem(adapter.ecosystem(), arguments.prepare.into());

    eprintln!("reprocut: proving a stable baseline and searching safe cuts...");
    let outcome = ReductionEngine::run(&request)?;
    eprintln!(
        "reprocut: stable baseline preserved; {} → {} files",
        outcome.original_files(),
        outcome.snapshot().files().len()
    );

    let summary = build_summary(&arguments, &outcome);
    let json = serde_json::to_vec_pretty(&summary)?;
    publish_artifact(&arguments, &outcome, &summary, &json)?;

    if arguments.json {
        println!("{}", String::from_utf8_lossy(&json));
    } else {
        println!(
            "Reduced {} files to {}. Open {}/report.html",
            outcome.original_files(),
            outcome.snapshot().files().len(),
            arguments.output.display()
        );
    }
    Ok(())
}

fn split_command(command: &[String]) -> (&str, &[String]) {
    let (program, arguments) = command
        .split_first()
        .expect("clap requires at least one command element");
    (program, arguments)
}

fn build_summary(arguments: &ReduceArgs, outcome: &ReductionOutcome) -> ReductionSummary {
    let fingerprint = outcome.fingerprint();
    ReductionSummary {
        schema_version: 1,
        source_root: arguments.root.display().to_string(),
        output: arguments.output.display().to_string(),
        command: arguments.command.clone(),
        ecosystem: arguments.ecosystem.name(),
        prepare: arguments.prepare,
        original_files: outcome.original_files(),
        retained_files: outcome.snapshot().files().len(),
        attempts: outcome
            .reduction()
            .attempts()
            .saturating_add(outcome.structured_attempts()),
        file_attempts: outcome.reduction().attempts(),
        structured_attempts: outcome.structured_attempts(),
        baseline_runs: outcome.baseline_runs(),
        final_verifications: outcome.final_verifications(),
        inconclusive_attempts: outcome.inconclusive_attempts(),
        cache_hits: outcome.cache_hits(),
        jobs: arguments.jobs,
        state: outcome.state_path().map(|path| path.display().to_string()),
        resumed: outcome.resumed(),
        oracle_stream: arguments.oracle_stream.into(),
        evaluation_policy: policy_summary(arguments),
        kept_files: outcome
            .snapshot()
            .files()
            .iter()
            .map(|file| file.path().to_owned())
            .collect(),
        accepted_structured_edits: outcome.accepted_structured_edits().to_vec(),
        fingerprint: FingerprintSummary {
            exit_code: fingerprint.exit_code(),
            signal: fingerprint.signal(),
            termination: fingerprint.termination(),
            anchor: fingerprint.anchor().to_owned(),
            anchors: fingerprint.anchors().to_vec(),
            normalization_schema: fingerprint.normalization_schema(),
        },
    }
}

fn session_mode(arguments: &ReduceArgs, resume: bool) -> SessionMode {
    let path = arguments
        .state
        .clone()
        .unwrap_or_else(|| arguments.root.join(".reprocut/state.sqlite3"));
    if resume {
        SessionMode::Resume(path)
    } else if arguments.restart {
        SessionMode::Restart(path)
    } else {
        SessionMode::Create(path)
    }
}

fn evaluation_policy(arguments: &ReduceArgs) -> Result<EvaluationPolicy, PolicyError> {
    if arguments.flaky {
        EvaluationPolicy::flaky(
            arguments.flaky_runs.unwrap_or(DEFAULT_FLAKY_RUNS),
            arguments.flaky_required.unwrap_or(DEFAULT_FLAKY_REQUIRED),
        )
    } else {
        Ok(EvaluationPolicy::strict())
    }
}

fn policy_summary(arguments: &ReduceArgs) -> PolicySummary {
    if arguments.flaky {
        PolicySummary {
            mode: "flaky",
            runs: arguments.flaky_runs.unwrap_or(DEFAULT_FLAKY_RUNS),
            required: arguments.flaky_required.unwrap_or(DEFAULT_FLAKY_REQUIRED),
        }
    } else {
        PolicySummary {
            mode: "strict",
            runs: 3,
            required: 3,
        }
    }
}

fn publish_artifact(
    arguments: &ReduceArgs,
    outcome: &ReductionOutcome,
    summary: &ReductionSummary,
    json: &[u8],
) -> Result<(), CliError> {
    let parent = usable_parent(&arguments.output)?;
    fs::create_dir_all(parent)
        .map_err(|source| io_error("create output parent", parent, source))?;
    let staging = tempfile::Builder::new()
        .prefix(".reprocut-publish-")
        .tempdir_in(parent)
        .map_err(|source| io_error("create staging directory", parent, source))?;
    let artifact = staging.path().join("artifact");
    let project = artifact.join("project");

    outcome.snapshot().copy_to(&project)?;

    let report = render_report(&report_model(arguments, outcome, summary));
    write_file(&artifact.join("report.html"), report.as_bytes())?;
    write_file(&artifact.join("reduction.json"), json)?;
    write_reproduction_scripts(&artifact, &arguments.command)?;

    ensure_output_absent(&arguments.output)?;
    publish_staging(staging, &artifact, &arguments.output)
}

fn report_model(
    arguments: &ReduceArgs,
    outcome: &ReductionOutcome,
    summary: &ReductionSummary,
) -> ReportModel {
    let mut accepted_sizes =
        Vec::with_capacity(outcome.reduction().accepted_sizes().len().saturating_add(1));
    accepted_sizes.push(outcome.original_files());
    accepted_sizes.extend_from_slice(outcome.reduction().accepted_sizes());

    ReportModel {
        command: display_command(&arguments.command),
        original_files: outcome.original_files(),
        retained_files: outcome.snapshot().files().len(),
        attempts: outcome
            .reduction()
            .attempts()
            .saturating_add(outcome.structured_attempts()),
        inconclusive_attempts: outcome.inconclusive_attempts(),
        cache_hits: outcome.cache_hits(),
        accepted_sizes,
        fingerprint: format_fingerprint(summary),
        kept_files: summary.kept_files.clone(),
    }
}

fn format_fingerprint(summary: &ReductionSummary) -> String {
    let termination = match summary.fingerprint.termination {
        TerminationReason::ExitCode(exit) => format!("exit {exit}"),
        TerminationReason::UnixSignal(signal) => format!("signal {signal}"),
        TerminationReason::TimedOut => "timed out".to_owned(),
        TerminationReason::RunnerFailure => "runner failure".to_owned(),
    };
    format!("{termination} · {}", summary.fingerprint.anchor)
}

fn write_reproduction_scripts(artifact: &Path, command: &[String]) -> Result<(), CliError> {
    let shell = format!(
        "#!/usr/bin/env sh\nset -eu\ncd -- \"$(dirname -- \"$0\")/project\"\nexec {}\n",
        command
            .iter()
            .map(|argument| quote_shell(argument))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let shell_path = artifact.join("reproduce.sh");
    write_file(&shell_path, shell.as_bytes())?;
    make_executable(&shell_path)?;

    let powershell = format!(
        "$ErrorActionPreference = 'Stop'\nSet-Location (Join-Path $PSScriptRoot 'project')\n& {}\nexit $LASTEXITCODE\n",
        command
            .iter()
            .map(|argument| quote_powershell(argument))
            .collect::<Vec<_>>()
            .join(" ")
    );
    write_file(&artifact.join("reproduce.ps1"), powershell.as_bytes())
}

fn quote_shell(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', "'\"'\"'"))
}

fn quote_powershell(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', "''"))
}

fn display_command(command: &[String]) -> String {
    command
        .iter()
        .map(|argument| {
            if argument
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-._/:\\".contains(character))
            {
                argument.clone()
            } else {
                format!("\"{}\"", argument.replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn ensure_output_absent(output: &Path) -> Result<(), CliError> {
    match fs::symlink_metadata(output) {
        Ok(_) => Err(CliError::OutputExists(output.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error("inspect output path", output, source)),
    }
}

fn usable_parent(output: &Path) -> Result<&Path, CliError> {
    if output.file_name().is_none() {
        return Err(CliError::InvalidOutput(output.to_path_buf()));
    }
    Ok(output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new(".")))
}

fn publish_staging(staging: TempDir, artifact: &Path, output: &Path) -> Result<(), CliError> {
    fs::rename(artifact, output)
        .map_err(|source| io_error("publish output atomically", output, source))?;
    drop(staging);
    Ok(())
}

fn write_file(path: &Path, contents: &[u8]) -> Result<(), CliError> {
    fs::write(path, contents).map_err(|source| io_error("write artifact", path, source))
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> CliError {
    CliError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)
        .map_err(|source| io_error("read script permissions", path, source))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|source| io_error("make reproduction script executable", path, source))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), CliError> {
    Ok(())
}
