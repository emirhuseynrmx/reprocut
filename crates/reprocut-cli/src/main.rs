//! `ReproCut` command-line entry point.

#![forbid(unsafe_code)]

use std::{
    ffi::OsString,
    fs, io,
    io::Write as _,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use reprocut_adapters::{Adapter, AdapterError, Ecosystem, EcosystemSelection};
use reprocut_core::{
    CandidateVerdict, ContainmentMechanism, ContentDigest, DiagnosticChannel, EvaluationPolicy,
    OracleError, OracleMode, OracleSpec, PolicyError, ProgressEventV1, ProtocolAction,
    ProtocolError, ReductionRequestV1, TerminationReason, PROTOCOL_VERSION,
};
use reprocut_engine::{
    Completion, EngineError, PreparationMode, PythonIsolationRequest, PythonPreparationError,
    ReductionEngine, ReductionOutcome, ReductionRequest, SessionMode,
};
use reprocut_oci::{export_archive, Builder, OciError, OciRequest, RuntimeFamily};
use reprocut_report::{
    build_artifact_manifest, render_issue, render_report, render_reproduction_scripts,
    verify_artifact, write_attempts_jsonl, AttemptSummary, ChannelAnchor, EvaluationPolicyEvidence,
    FailureEvidence, FinalObservationEvidence, ManifestError, MaterialMeasurement, MeasurementSet,
    PreparationEvidence, ReductionEvidence, ReportModel, RetainedEntry, RetainedManifest,
    RetentionEvidence, SearchEvidence, VerificationError, EVIDENCE_SCHEMA_VERSION,
};
use reprocut_workspace::{ProjectInventory, ProjectSnapshot, WorkspaceError};
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
    /// Independently verify one completed artifact's complete byte identity.
    Verify(VerifyArgs),
    /// Export a completed artifact into a distribution format.
    Export(ExportArgs),
    /// Run the versioned JSONL integration protocol.
    Protocol(ProtocolArgs),
    /// Prepare a redacted, local-only static gallery submission.
    Gallery(GalleryArgs),
    /// Write a shell completion script to standard output.
    Completions(CompletionsArgs),
}

#[derive(Debug, Args)]
struct VerifyArgs {
    /// Completed `ReproCut` artifact directory.
    #[arg(value_name = "OUTPUT")]
    output: PathBuf,

    /// Emit one machine-readable result.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct CompletionsArgs {
    /// Target shell.
    #[arg(value_enum)]
    shell: CompletionShell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
}

impl From<CompletionShell> for Shell {
    fn from(value: CompletionShell) -> Self {
        match value {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Elvish => Self::Elvish,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::PowerShell => Self::PowerShell,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}

#[derive(Debug, Args)]
struct GalleryArgs {
    #[command(subcommand)]
    action: GalleryCommand,
}

#[derive(Debug, Subcommand)]
enum GalleryCommand {
    /// Create a reviewable submission directory without uploading anything.
    Prepare(GalleryPrepareArgs),
}

#[derive(Debug, Args)]
struct GalleryPrepareArgs {
    /// Completed `ReproCut` artifact containing `reduction.json`.
    #[arg(long, value_name = "ARTIFACT")]
    from: PathBuf,

    /// New local submission directory; existing paths are never overwritten.
    #[arg(short, long, default_value = "reprocut-gallery-submission")]
    output: PathBuf,

    /// Public gallery title (1..=100 printable characters).
    #[arg(long)]
    title: String,

    /// SPDX license expression covering submitted metadata and optional source.
    #[arg(long, value_name = "SPDX")]
    license: String,

    /// Explicitly copy the verified minimal project into the submission.
    #[arg(long)]
    include_source: bool,
}

#[derive(Debug, Args)]
struct ProtocolArgs {
    #[command(subcommand)]
    action: ProtocolCommand,
}

#[derive(Debug, Subcommand)]
enum ProtocolCommand {
    /// Execute one JSON request and stream tagged JSONL events.
    Run(ProtocolRunArgs),
}

#[derive(Debug, Args)]
struct ProtocolRunArgs {
    /// JSON file containing a `ReductionRequestV1`.
    #[arg(long)]
    request: PathBuf,
}

#[derive(Debug, Args)]
struct ExportArgs {
    #[command(subcommand)]
    format: ExportFormat,
}

#[derive(Debug, Subcommand)]
enum ExportFormat {
    /// Build and validate a real OCI image archive.
    Oci(OciArgs),
}

#[derive(Debug, Args)]
struct OciArgs {
    /// Completed `ReproCut` artifact containing `project/` and `reduction.json`.
    #[arg(long, value_name = "ARTIFACT")]
    from: PathBuf,

    /// New OCI archive path; existing files are never overwritten.
    #[arg(short, long, default_value = "reprocut.oci.tar")]
    output: PathBuf,

    /// OCI builder frontend; auto probes `Docker Buildx` then `BuildKit`.
    #[arg(long, value_enum, default_value_t = BuilderArg::Auto)]
    builder: BuilderArg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum BuilderArg {
    Auto,
    DockerBuildx,
    BuildKit,
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

    /// Candidate preparation authority; isolated-python builds a fresh offline venv per candidate.
    #[arg(long, value_enum, default_value_t = PrepareArg::Offline)]
    prepare: PrepareArg,

    /// Deadline for each candidate execution.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
    timeout_ms: u64,

    /// Wall-time budget for the search; publishes the best verified result when it elapses.
    #[arg(long)]
    max_duration_secs: Option<u64>,

    /// Maximum captured bytes for each child output stream.
    #[arg(long, default_value_t = DEFAULT_CAPTURE_BYTES)]
    max_output_bytes: usize,

    /// Process stream used to identify the stabilized failure.
    #[arg(long, value_enum, default_value_t = OracleStreamArg::Auto)]
    oracle_stream: OracleStreamArg,

    /// Failure recognition contract.
    #[arg(long, value_enum, default_value_t = OracleModeArg::Automatic)]
    oracle_mode: OracleModeArg,

    /// Required regex in regex mode; repeat for an AND contract.
    #[arg(long = "failure-regex")]
    failure_patterns: Vec<String>,

    /// Regex that rejects a candidate in automatic or regex mode.
    #[arg(long = "reject-regex")]
    reject_patterns: Vec<String>,

    /// Explicit Python interpreter for isolated-python preparation.
    #[arg(long)]
    python_executable: Option<PathBuf>,

    /// Offline wheel directory captured before reduction begins.
    #[arg(long)]
    python_wheelhouse: Option<PathBuf>,

    /// Python extra to install; repeatable, normalized, and deduplicated.
    #[arg(long = "python-extra")]
    python_extras: Vec<String>,

    /// Strict schema-1 JSON containing argv-only preparation commands.
    #[arg(long)]
    prepare_spec: Option<PathBuf>,

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

    /// `SQLite` journal path (defaults to `ROOT/.reprocut/state.sqlite3`).
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum OracleModeArg {
    Automatic,
    Regex,
    ExitZero,
}

impl From<OracleModeArg> for OracleMode {
    fn from(value: OracleModeArg) -> Self {
        match value {
            OracleModeArg::Automatic => Self::Automatic,
            OracleModeArg::Regex => Self::Regex,
            OracleModeArg::ExitZero => Self::ExitZero,
        }
    }
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
    #[error(transparent)]
    Oci(#[from] OciError),
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
    #[error(transparent)]
    Oracle(#[from] OracleError),
    #[error(transparent)]
    PythonPreparation(#[from] PythonPreparationError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Verification(#[from] VerificationError),
    #[error("gallery submission is invalid: {0}")]
    Gallery(String),
    #[error("{0}")]
    InvalidArguments(&'static str),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let machine_protocol = matches!(&cli.action, Action::Protocol(_));
    match execute(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if !machine_protocol {
                eprintln!("error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> Result<(), CliError> {
    match cli.action {
        Action::Minimize(arguments) | Action::Reduce(arguments) => reduce_project(arguments, false),
        Action::Resume(arguments) => reduce_project(arguments, true),
        Action::Verify(arguments) => verify_completed_artifact(&arguments),
        Action::Export(arguments) => export_artifact(arguments),
        Action::Protocol(arguments) => run_protocol(arguments),
        Action::Gallery(arguments) => run_gallery(arguments),
        Action::Completions(arguments) => {
            emit_completions(&arguments);
            Ok(())
        }
    }
}

fn verify_completed_artifact(arguments: &VerifyArgs) -> Result<(), CliError> {
    let verified = verify_artifact(&arguments.output)?;
    if arguments.json {
        println!(
            "{}",
            serde_json::json!({
                "artifact_id": verified.artifact_id(),
                "artifact_manifest_schema": reprocut_report::ARTIFACT_MANIFEST_SCHEMA_VERSION,
                "verified": true,
            })
        );
    } else {
        println!("verified artifact sha256:{}", verified.artifact_id());
    }
    Ok(())
}

fn emit_completions(arguments: &CompletionsArgs) {
    let mut command = Cli::command();
    generate(
        Shell::from(arguments.shell),
        &mut command,
        "reprocut",
        &mut io::stdout().lock(),
    );
}

fn run_protocol(arguments: ProtocolArgs) -> Result<(), CliError> {
    let result = match arguments.action {
        ProtocolCommand::Run(arguments) => run_protocol_request(&arguments.request),
    };
    if let Err(error) = &result {
        let _ = emit_event(&ProgressEventV1::Failed {
            protocol_version: PROTOCOL_VERSION,
            message: error.to_string(),
        });
    }
    result
}

fn run_protocol_request(path: &Path) -> Result<(), CliError> {
    let bytes = fs::read(path).map_err(|source| io_error("read protocol request", path, source))?;
    let request: ReductionRequestV1 = serde_json::from_slice(&bytes)?;
    request.validate()?;
    emit_event(&ProgressEventV1::Started {
        protocol_version: PROTOCOL_VERSION,
        action: request.action,
        root: request.root.clone(),
    })?;
    let resume = request.action == ProtocolAction::Resume;
    let arguments = protocol_reduce_args(request)?;
    let completed = execute_reduction(arguments, resume, false)?;
    emit_event(&ProgressEventV1::BaselineStable {
        protocol_version: PROTOCOL_VERSION,
        fingerprint_sha256: completed.evidence.failure.fingerprint_sha256.clone(),
    })?;
    emit_event(&ProgressEventV1::Completed {
        protocol_version: PROTOCOL_VERSION,
        evidence: completed.arguments.output.join("reduction.json"),
        report: completed.arguments.output.join("report.html"),
        issue: completed.arguments.output.join("issue.md"),
        output: completed.arguments.output,
    })
}

fn protocol_reduce_args(request: ReductionRequestV1) -> Result<ReduceArgs, CliError> {
    let ecosystem = match request.ecosystem.as_str() {
        "auto" => EcosystemArg::Auto,
        "cargo" => EcosystemArg::Cargo,
        "python" => EcosystemArg::Python,
        "npm" => EcosystemArg::Npm,
        "none" => EcosystemArg::None,
        _ => return Err(CliError::InvalidArguments("unsupported protocol ecosystem")),
    };
    let prepare = match request.preparation.as_str() {
        "none" => PrepareArg::None,
        "offline" => PrepareArg::Offline,
        "lifecycle_scripts" => PrepareArg::LifecycleScripts,
        "isolated_python" => PrepareArg::IsolatedPython,
        _ => {
            return Err(CliError::InvalidArguments(
                "unsupported protocol preparation",
            ))
        }
    };
    let oracle_stream = match request.oracle_stream.as_str() {
        "auto" => OracleStreamArg::Auto,
        "stderr" => OracleStreamArg::Stderr,
        "stdout" => OracleStreamArg::Stdout,
        "combined" => OracleStreamArg::Combined,
        _ => {
            return Err(CliError::InvalidArguments(
                "unsupported protocol oracle stream",
            ))
        }
    };
    let oracle_mode = match request.oracle_mode.as_str() {
        "automatic" => OracleModeArg::Automatic,
        "regex" => OracleModeArg::Regex,
        "exit_zero" => OracleModeArg::ExitZero,
        _ => {
            return Err(CliError::InvalidArguments(
                "unsupported protocol oracle mode",
            ))
        }
    };
    Ok(ReduceArgs {
        root: request.root,
        output: request.output,
        ecosystem,
        prepare,
        timeout_ms: request.timeout_ms,
        // The protocol has no budget field yet; a client that wants one sets it
        // through its own job timeout until the request schema carries it.
        max_duration_secs: None,
        max_output_bytes: request.max_output_bytes,
        oracle_stream,
        oracle_mode,
        failure_patterns: request.failure_patterns,
        reject_patterns: request.reject_patterns,
        python_executable: request.python_executable,
        python_wheelhouse: request.python_wheelhouse,
        python_extras: request.python_extras,
        prepare_spec: request.prepare_spec,
        flaky: request.flaky_runs.is_some() || request.flaky_required.is_some(),
        flaky_runs: request.flaky_runs,
        flaky_required: request.flaky_required,
        json: false,
        jobs: request.jobs,
        state: request.state,
        restart: request.restart,
        command: request.command,
    })
}

fn emit_event(event: &ProgressEventV1) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, event)?;
    output
        .write_all(b"\n")
        .map_err(|source| io_error("write protocol event", Path::new("<stdout>"), source))?;
    output
        .flush()
        .map_err(|source| io_error("flush protocol event", Path::new("<stdout>"), source))
}

#[derive(Debug, Serialize)]
struct GalleryEntry {
    schema_version: u16,
    parent_artifact_id: String,
    slug: String,
    title: String,
    license: String,
    ecosystem: String,
    fingerprint_sha256: String,
    termination: String,
    original_files: u64,
    retained_files: u64,
    original_bytes: u64,
    retained_bytes: u64,
    source_included: bool,
    featured: bool,
}

fn run_gallery(arguments: GalleryArgs) -> Result<(), CliError> {
    match arguments.action {
        GalleryCommand::Prepare(arguments) => prepare_gallery(&arguments),
    }
}

fn prepare_gallery(arguments: &GalleryPrepareArgs) -> Result<(), CliError> {
    validate_gallery_text(&arguments.title, "title", 100)?;
    validate_gallery_license(&arguments.license)?;
    ensure_output_absent(&arguments.output)?;

    let verified = verify_artifact(&arguments.from)?;
    let evidence_path = verified.root().join("reduction.json");
    let bytes = fs::read(&evidence_path)
        .map_err(|source| io_error("read reduction evidence", &evidence_path, source))?;
    let evidence: ReductionEvidence = serde_json::from_slice(&bytes)?;
    if evidence.schema_version != EVIDENCE_SCHEMA_VERSION {
        return Err(CliError::Gallery(format!(
            "expected evidence schema {EVIDENCE_SCHEMA_VERSION}, found {}",
            evidence.schema_version
        )));
    }
    evidence.validate().map_err(CliError::InvalidArguments)?;
    if !evidence.failure.same_failure
        || evidence.failure.fingerprint_sha256.len() != 64
        || !evidence
            .failure
            .fingerprint_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(CliError::Gallery(
            "artifact has no verified same-failure fingerprint".to_owned(),
        ));
    }
    let title = arguments.title.trim().to_owned();
    let license = arguments.license.trim().to_owned();
    let entry = GalleryEntry {
        schema_version: 1,
        parent_artifact_id: verified.artifact_id().to_owned(),
        slug: gallery_slug(&title, &evidence.failure.fingerprint_sha256),
        title,
        license,
        ecosystem: evidence.ecosystem.clone(),
        fingerprint_sha256: evidence.failure.fingerprint_sha256.clone(),
        termination: evidence.failure.termination.clone(),
        original_files: evidence.measurements.original.files,
        retained_files: evidence.measurements.retained.files,
        original_bytes: evidence.measurements.original.bytes,
        retained_bytes: evidence.measurements.retained.bytes,
        source_included: arguments.include_source,
        featured: false,
    };

    let parent = usable_parent(&arguments.output)?;
    fs::create_dir_all(parent)
        .map_err(|source| io_error("create gallery output parent", parent, source))?;
    let staging = tempfile::Builder::new()
        .prefix(".reprocut-gallery-")
        .tempdir_in(parent)
        .map_err(|source| io_error("create gallery staging directory", parent, source))?;
    let submission = staging.path().join("submission");
    fs::create_dir(&submission)
        .map_err(|source| io_error("create gallery submission", &submission, source))?;

    let mut json = serde_json::to_vec_pretty(&entry)?;
    json.push(b'\n');
    write_file(&submission.join("entry.json"), &json)?;
    write_file(
        &submission.join("index.html"),
        gallery_html(&entry).as_bytes(),
    )?;
    write_file(
        &submission.join("LICENSE_DECLARATION.md"),
        format!(
            "# License declaration\n\nThe submitter declares this gallery submission under `{}`.\n",
            entry.license
        )
        .as_bytes(),
    )?;
    write_file(
        &submission.join("README.md"),
        format!(
            "# {}\n\nRedacted ReproCut gallery submission. Fingerprint: `{}`.\n\nNothing in this directory was uploaded automatically. Review every file before opening a pull request.\n",
            entry.title, entry.fingerprint_sha256
        )
        .as_bytes(),
    )?;
    if arguments.include_source {
        let project = verified.root().join("project");
        let inventory = ProjectInventory::scan(&project)?;
        ProjectSnapshot::from_inventory(&inventory, inventory.units())?
            .copy_to(&submission.join("source"))?;
    }
    let manifest = build_artifact_manifest(&submission)?;
    write_file(
        &submission.join("artifact-manifest.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;

    ensure_output_absent(&arguments.output)?;
    publish_staging(staging, &submission, &arguments.output)?;
    println!(
        "Prepared redacted gallery submission at {} (no upload performed)",
        arguments.output.display()
    );
    Ok(())
}

fn validate_gallery_text(value: &str, field: &str, max_chars: usize) -> Result<(), CliError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > max_chars
        || trimmed.chars().any(char::is_control)
    {
        return Err(CliError::Gallery(format!(
            "{field} must contain 1..={max_chars} printable characters"
        )));
    }
    Ok(())
}

fn validate_gallery_license(value: &str) -> Result<(), CliError> {
    validate_gallery_text(value, "license", 100)?;
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-+.()/ :".contains(character))
    {
        return Err(CliError::Gallery(
            "license must be a bounded SPDX expression".to_owned(),
        ));
    }
    Ok(())
}

fn gallery_slug(title: &str, fingerprint: &str) -> String {
    let mut slug = String::with_capacity(title.len().min(64));
    let mut separator = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            let separator_bytes = usize::from(separator && !slug.is_empty());
            if slug.len().saturating_add(separator_bytes).saturating_add(1) > 64 {
                break;
            }
            if separator_bytes == 1 {
                slug.push('-');
            }
            separator = false;
            slug.push(character);
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        format!("repro-{}", &fingerprint[..12])
    } else {
        slug
    }
}

fn gallery_html(entry: &GalleryEntry) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title} · ReproCut</title><style>body{{font:16px/1.55 system-ui;max-width:760px;margin:10vh auto;padding:0 24px;background:#0b0d10;color:#e9eef4}}code{{color:#7ee787}}.metric{{display:grid;grid-template-columns:repeat(2,1fr);gap:12px}}article{{border:1px solid #30363d;border-radius:16px;padding:24px}}</style><article><small>REPROCUT GALLERY · REDACTED</small><h1>{title}</h1><p><code>{fingerprint}</code></p><div class=\"metric\"><p>Files<br><strong>{original_files} → {retained_files}</strong></p><p>Bytes<br><strong>{original_bytes} → {retained_bytes}</strong></p></div><p>{termination} · {ecosystem}</p><p>License: {license}</p></article></html>\n",
        title = escape_gallery_html(&entry.title),
        fingerprint = escape_gallery_html(&entry.fingerprint_sha256),
        original_files = entry.original_files,
        retained_files = entry.retained_files,
        original_bytes = entry.original_bytes,
        retained_bytes = entry.retained_bytes,
        termination = escape_gallery_html(&entry.termination),
        ecosystem = escape_gallery_html(&entry.ecosystem),
        license = escape_gallery_html(&entry.license),
    )
}

fn escape_gallery_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn export_artifact(arguments: ExportArgs) -> Result<(), CliError> {
    match arguments.format {
        ExportFormat::Oci(arguments) => export_oci(arguments),
    }
}

fn export_oci(arguments: OciArgs) -> Result<(), CliError> {
    let verified = verify_artifact(&arguments.from)?;
    let evidence_path = verified.root().join("reduction.json");
    let bytes = fs::read(&evidence_path)
        .map_err(|source| io_error("read reduction evidence", &evidence_path, source))?;
    let evidence: ReductionEvidence = serde_json::from_slice(&bytes)?;
    let runtime = match evidence.ecosystem.as_str() {
        "cargo" => RuntimeFamily::Cargo,
        "python" => RuntimeFamily::Python,
        "npm" => RuntimeFamily::Npm,
        _ => RuntimeFamily::Generic,
    };
    let mut request = OciRequest::new(
        verified.root().to_path_buf(),
        arguments.output,
        runtime,
        evidence.command,
        evidence.failure.fingerprint_sha256,
        verified.artifact_id().to_owned(),
    );
    request = match arguments.builder {
        BuilderArg::Auto => request,
        BuilderArg::DockerBuildx => request.with_builder(Builder::DockerBuildx),
        BuilderArg::BuildKit => request.with_builder(Builder::BuildKit),
    };
    let builder = export_archive(&request)?;
    println!("Exported {} with {builder:?}", request.output().display());
    Ok(())
}

struct CompletedReduction {
    arguments: ReduceArgs,
    outcome: ReductionOutcome,
    evidence: ReductionEvidence,
    json: Vec<u8>,
}

fn reduce_project(arguments: ReduceArgs, resume: bool) -> Result<(), CliError> {
    let completed = execute_reduction(arguments, resume, true)?;
    if completed.arguments.json {
        println!("{}", String::from_utf8_lossy(&completed.json));
    } else {
        println!(
            "Reduced {} files to {}. Open {}/report.html",
            completed.outcome.original_files(),
            completed.outcome.snapshot().files().len(),
            completed.arguments.output.display()
        );
    }
    Ok(())
}

fn execute_reduction(
    mut arguments: ReduceArgs,
    resume: bool,
    human_progress: bool,
) -> Result<CompletedReduction, CliError> {
    if resume && arguments.restart {
        return Err(CliError::InvalidArguments(
            "resume and --restart are mutually exclusive",
        ));
    }
    let evaluation_policy = evaluation_policy(&arguments)?;
    let oracle_spec = OracleSpec::new(
        arguments.oracle_mode.into(),
        arguments.oracle_stream.into(),
        arguments.failure_patterns.clone(),
        arguments.reject_patterns.clone(),
    )?;
    let python_isolation = python_isolation_request(&arguments)?;
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
    .with_oracle(oracle_spec)
    .with_runtime(arguments.jobs, session_mode(&arguments, resume))
    .with_inventory_policy(adapter.inventory_policy().clone())
    .with_ecosystem(adapter.ecosystem(), arguments.prepare.into());
    let request = match arguments.max_duration_secs {
        Some(budget) => request.with_max_duration(Duration::from_secs(budget)),
        None => request,
    };
    let request = if let Some(isolation) = python_isolation {
        request.with_python_isolation(isolation)
    } else {
        request
    };

    if human_progress {
        eprintln!("reprocut: proving a stable baseline and searching safe cuts...");
    }
    let outcome = ReductionEngine::run(&request)?;
    if human_progress {
        eprintln!(
            "reprocut: stable baseline preserved; {} → {} files",
            outcome.original_files(),
            outcome.snapshot().files().len()
        );
    }

    let evidence = build_evidence(&arguments, &outcome)?;
    evidence.validate().map_err(CliError::InvalidArguments)?;
    let json = serde_json::to_vec_pretty(&evidence)?;
    publish_artifact(&arguments, &outcome, &evidence, &json)?;

    Ok(CompletedReduction {
        arguments,
        outcome,
        evidence,
        json,
    })
}

fn python_isolation_request(
    arguments: &ReduceArgs,
) -> Result<Option<PythonIsolationRequest>, CliError> {
    let selected = arguments.prepare == PrepareArg::IsolatedPython;
    let fields_present = arguments.python_executable.is_some()
        || arguments.python_wheelhouse.is_some()
        || !arguments.python_extras.is_empty()
        || arguments.prepare_spec.is_some();
    if !selected && fields_present {
        return Err(CliError::InvalidArguments(
            "Python isolation fields require --prepare isolated-python",
        ));
    }
    if !selected {
        return Ok(None);
    }
    let interpreter = arguments
        .python_executable
        .clone()
        .ok_or(CliError::InvalidArguments(
            "--prepare isolated-python requires --python-executable and --python-wheelhouse",
        ))?;
    let wheelhouse = arguments
        .python_wheelhouse
        .clone()
        .ok_or(CliError::InvalidArguments(
            "--prepare isolated-python requires --python-executable and --python-wheelhouse",
        ))?;
    let mut isolation = PythonIsolationRequest::new(interpreter, wheelhouse)
        .with_extras(arguments.python_extras.clone())?;
    if let Some(spec) = &arguments.prepare_spec {
        isolation = isolation.with_prepare_spec(spec.clone());
    }
    Ok(Some(isolation))
}

fn split_command(command: &[String]) -> (&str, &[String]) {
    let (program, arguments) = command
        .split_first()
        .expect("clap requires at least one command element");
    (program, arguments)
}

fn build_attempt_summaries(outcome: &ReductionOutcome) -> Vec<AttemptSummary> {
    outcome
        .attempt_events()
        .iter()
        .map(|event| AttemptSummary {
            event_id: event.id(),
            candidate_sha256: event.candidate().to_hex(),
            verdict: verdict_name(event.verdict()).to_owned(),
            observed_runs: event.observed_runs(),
            inconclusive_runs: event.inconclusive_runs(),
            completed_at_unix: event.completed_at(),
            evidence: serde_json::from_str(event.evidence_json())
                .unwrap_or_else(|_| serde_json::Value::String(event.evidence_json().to_owned())),
        })
        .collect()
}

fn build_accepted_file_sizes(outcome: &ReductionOutcome) -> Vec<usize> {
    let mut sizes =
        Vec::with_capacity(outcome.reduction().accepted_sizes().len().saturating_add(1));
    sizes.push(outcome.original_files());
    sizes.extend_from_slice(outcome.reduction().accepted_sizes());
    sizes
}

fn build_retained_manifest(outcome: &ReductionOutcome) -> Result<RetainedManifest, CliError> {
    let entries = outcome
        .snapshot()
        .files()
        .iter()
        .map(|file| {
            RetainedEntry::regular_file(file.path(), file.contents(), file.executable_mask())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RetainedManifest::new(entries)?)
}

fn build_final_observations(outcome: &ReductionOutcome) -> Vec<FinalObservationEvidence> {
    outcome
        .final_observations()
        .iter()
        .enumerate()
        .map(|(index, final_observation)| {
            let observation = final_observation.observation();
            FinalObservationEvidence {
                ordinal: u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX),
                verdict: verdict_name(final_observation.verdict()).to_owned(),
                termination: termination_name(observation.termination()),
                exit_code: observation.exit_code(),
                signal: observation.signal(),
                timed_out: observation.timed_out(),
                streams_truncated: observation.streams_truncated(),
                containment: containment_name(observation.containment()).to_owned(),
                stdout_sha256: ContentDigest::of(observation.stdout()).to_hex(),
                stdout_bytes: u64::try_from(observation.stdout().len()).unwrap_or(u64::MAX),
                stderr_sha256: ContentDigest::of(observation.stderr()).to_hex(),
                stderr_bytes: u64::try_from(observation.stderr().len()).unwrap_or(u64::MAX),
            }
        })
        .collect()
}

fn build_evidence(
    arguments: &ReduceArgs,
    outcome: &ReductionOutcome,
) -> Result<ReductionEvidence, CliError> {
    let fingerprint = outcome.fingerprint();
    let attempts = build_attempt_summaries(outcome);
    let accepted_file_sizes = build_accepted_file_sizes(outcome);
    let retained_manifest = build_retained_manifest(outcome)?;
    let final_observations = build_final_observations(outcome);

    Ok(ReductionEvidence {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        source_root: arguments.root.display().to_string(),
        source_snapshot_sha256: outcome.source_snapshot_digest().to_hex(),
        output: arguments.output.display().to_string(),
        command: arguments.command.clone(),
        ecosystem: arguments.ecosystem.name().to_owned(),
        preparation: PreparationEvidence {
            mode: prepare_name(arguments.prepare).to_owned(),
            contract_sha256: Some(outcome.preparation_digest().to_hex()),
            limitations: Vec::new(),
        },
        measurements: MeasurementSet {
            original: MaterialMeasurement {
                files: u64::try_from(outcome.original_files()).unwrap_or(u64::MAX),
                bytes: outcome.original_bytes(),
                lines: outcome.original_lines(),
                syntax_nodes: None,
            },
            retained: material_measurement(outcome.snapshot()),
            elapsed_ms: u64::try_from(outcome.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
        search: SearchEvidence {
            attempts: outcome
                .reduction()
                .attempts()
                .saturating_add(outcome.structured_attempts()),
            file_attempts: outcome.reduction().attempts(),
            structured_attempts: outcome.structured_attempts(),
            inconclusive_attempts: outcome.inconclusive_attempts(),
            cache_hits: outcome.cache_hits(),
            baseline_runs: outcome.baseline_runs(),
            final_verifications: outcome.final_verifications(),
            jobs: arguments.jobs,
            state: outcome.state_path().map(|path| path.display().to_string()),
            resumed: outcome.resumed(),
            completion: outcome.completion().as_str().to_owned(),
            accepted_file_sizes,
            evaluation_policy: policy_evidence(arguments),
        },
        failure: FailureEvidence {
            same_failure: true,
            fingerprint_sha256: fingerprint.digest().to_hex(),
            exit_code: fingerprint.exit_code(),
            signal: fingerprint.signal(),
            termination: termination_name(fingerprint.termination()),
            oracle_stream: diagnostic_channel_name(arguments.oracle_stream.into()).to_owned(),
            oracle_mode: oracle_mode_name(fingerprint.mode()).to_owned(),
            anchor: fingerprint.anchor().to_owned(),
            anchors: fingerprint
                .anchors()
                .iter()
                .map(|anchor| ChannelAnchor {
                    channel: diagnostic_channel_name(anchor.channel()).to_owned(),
                    text: anchor.text().to_owned(),
                })
                .collect(),
            normalization_schema: fingerprint.normalization_schema(),
            failure_patterns: fingerprint.failure_patterns().to_vec(),
            reject_patterns: fingerprint.reject_patterns().to_vec(),
            oracle_spec_sha256: fingerprint.oracle_spec_digest().to_hex(),
        },
        kept_files: outcome
            .snapshot()
            .files()
            .iter()
            .map(|file| RetentionEvidence {
                path: file.path().to_owned(),
                observation: "Present in the final repeatedly verified snapshot; no semantic-causality claim is inferred."
                    .to_owned(),
            })
            .collect(),
        retained_manifest,
        final_observations,
        accepted_structured_edits: outcome.accepted_structured_edits().to_vec(),
        attempts,
        limitations: {
            let mut limitations = vec![
                "Elapsed time is one wall-clock observation, not a benchmark.".to_owned(),
                "Retained paths are observations from the verified final snapshot, not claims of semantic necessity."
                    .to_owned(),
                "Syntax-node counts are omitted until a grammar-valid cross-language counter is available."
                    .to_owned(),
            ];
            if outcome.completion() == Completion::BudgetExhausted {
                limitations.push(
                    "The wall-time budget elapsed with candidates unexplored. Every retained file passed final verification, but a longer run may reduce further."
                        .to_owned(),
                );
            }
            limitations
        },
    })
}

fn material_measurement(snapshot: &reprocut_workspace::ProjectSnapshot) -> MaterialMeasurement {
    let lines = snapshot.files().iter().fold(0_u64, |total, file| {
        let contents = file.contents();
        let newlines =
            u64::try_from(memchr::memchr_iter(b'\n', contents).count()).unwrap_or(u64::MAX);
        total.saturating_add(newlines).saturating_add(u64::from(
            !contents.is_empty() && contents.last() != Some(&b'\n'),
        ))
    });
    MaterialMeasurement {
        files: u64::try_from(snapshot.files().len()).unwrap_or(u64::MAX),
        bytes: snapshot.total_bytes(),
        lines,
        syntax_nodes: None,
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

fn policy_evidence(arguments: &ReduceArgs) -> EvaluationPolicyEvidence {
    if arguments.flaky {
        EvaluationPolicyEvidence {
            mode: "flaky".to_owned(),
            runs: arguments.flaky_runs.unwrap_or(DEFAULT_FLAKY_RUNS),
            required: arguments.flaky_required.unwrap_or(DEFAULT_FLAKY_REQUIRED),
        }
    } else {
        EvaluationPolicyEvidence {
            mode: "strict".to_owned(),
            runs: 3,
            required: 3,
        }
    }
}

const fn prepare_name(prepare: PrepareArg) -> &'static str {
    match prepare {
        PrepareArg::None => "none",
        PrepareArg::Offline => "offline",
        PrepareArg::LifecycleScripts => "lifecycle_scripts",
        PrepareArg::IsolatedPython => "isolated_python",
    }
}

const fn verdict_name(verdict: CandidateVerdict) -> &'static str {
    match verdict {
        CandidateVerdict::Preserved => "preserved",
        CandidateVerdict::Rejected => "rejected",
        CandidateVerdict::Inconclusive => "inconclusive",
    }
}

fn termination_name(termination: TerminationReason) -> String {
    match termination {
        TerminationReason::ExitCode(exit) => format!("exit {exit}"),
        TerminationReason::UnixSignal(signal) => format!("signal {signal}"),
        TerminationReason::TimedOut => "timed out".to_owned(),
        TerminationReason::RunnerFailure => "runner failure".to_owned(),
    }
}

const fn diagnostic_channel_name(channel: DiagnosticChannel) -> &'static str {
    match channel {
        DiagnosticChannel::Auto => "auto",
        DiagnosticChannel::Stderr => "stderr",
        DiagnosticChannel::Stdout => "stdout",
        DiagnosticChannel::Combined => "combined",
    }
}

const fn oracle_mode_name(mode: OracleMode) -> &'static str {
    match mode {
        OracleMode::Automatic => "automatic",
        OracleMode::Regex => "regex",
        OracleMode::ExitZero => "exit_zero",
    }
}

const fn containment_name(containment: ContainmentMechanism) -> &'static str {
    match containment {
        ContainmentMechanism::DirectChild => "direct_child",
        ContainmentMechanism::PosixProcessGroup => "posix_process_group",
        ContainmentMechanism::WindowsJobObject => "windows_job_object",
    }
}

fn publish_artifact(
    arguments: &ReduceArgs,
    outcome: &ReductionOutcome,
    evidence: &ReductionEvidence,
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

    let report = render_report(&report_model(evidence));
    write_file(&artifact.join("report.html"), report.as_bytes())?;
    let issue = render_issue(evidence);
    write_file(&artifact.join("issue.md"), issue.as_bytes())?;
    write_file(&artifact.join("reduction.json"), json)?;
    let attempts_path = artifact.join("attempts.jsonl");
    let attempts_file = fs::File::create(&attempts_path)
        .map_err(|source| io_error("create attempt ledger", &attempts_path, source))?;
    let mut attempts_writer = io::BufWriter::new(attempts_file);
    write_attempts_jsonl(&evidence.attempts, &mut attempts_writer)?;
    attempts_writer
        .flush()
        .map_err(|source| io_error("flush attempt ledger", &attempts_path, source))?;
    // Windows forbids renaming a directory while a descendant file handle is open.
    drop(attempts_writer);
    write_reproduction_scripts(&artifact, &arguments.command)?;
    let manifest = build_artifact_manifest(&artifact)?;
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    write_file(&artifact.join("artifact-manifest.json"), &manifest_json)?;
    let _verified = verify_artifact(&artifact)?;

    ensure_output_absent(&arguments.output)?;
    publish_staging(staging, &artifact, &arguments.output)
}

fn report_model(evidence: &ReductionEvidence) -> ReportModel {
    ReportModel::from(evidence)
}

fn write_reproduction_scripts(artifact: &Path, command: &[String]) -> Result<(), CliError> {
    let scripts = render_reproduction_scripts(command);
    let shell_path = artifact.join("reproduce.sh");
    write_file(&shell_path, scripts.shell.as_bytes())?;
    #[cfg(unix)]
    make_executable(&shell_path)?;
    write_file(
        &artifact.join("reproduce.ps1"),
        scripts.powershell.as_bytes(),
    )
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
