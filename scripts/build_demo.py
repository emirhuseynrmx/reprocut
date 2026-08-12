#!/usr/bin/env python3
"""Build and independently verify the checked-in ReproCut demo artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

from playground_workspace_verify import ROOT, compose_engine, report_source, wrap

SOURCE = ROOT / "demo" / "source"
RESULT = ROOT / "demo" / "result"
EXPECTED_KEPT = ["bug.py", "checkout.py", "fixtures/order.json"]
META_BEGIN = "__REPROCUT_META_BEGIN__"
META_END = "__REPROCUT_META_END__"
HTML_BEGIN = "__REPROCUT_HTML_BEGIN__"
HTML_END = "__REPROCUT_HTML_END__"
ISSUE_BEGIN = "__REPROCUT_ISSUE_BEGIN__"
ISSUE_END = "__REPROCUT_ISSUE_END__"
ATTEMPTS_BEGIN = "__REPROCUT_ATTEMPTS_BEGIN__"
ATTEMPTS_END = "__REPROCUT_ATTEMPTS_END__"

# The official Playground image has no Python executable. This adapter lets the
# real Rust search engine execute a content-equivalent shell property there.
# The builder separately runs both Python trees three times before publication.
DEMO_RUNNER = r'''
use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
use crate::reprocut_core::{ContainmentMechanism, ExecutionObservation, TerminationReason};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    program: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    timeout: Duration,
    max_output_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildEnvironment;

impl ChildEnvironment {
    pub const fn inherit() -> Self { Self }
    pub fn set(self, _name: impl Into<OsString>, _value: impl Into<OsString>) -> Self { self }
    pub fn remove(self, _name: impl Into<OsString>) -> Self { self }
    pub fn prepend_path(self, _directory: impl Into<PathBuf>) -> Self { self }
}

impl CommandSpec {
    pub fn new(
        program: PathBuf,
        arguments: Vec<OsString>,
        working_directory: PathBuf,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Self {
        Self { program, arguments, working_directory, timeout, max_output_bytes }
    }

    pub fn with_environment(self, _environment: ChildEnvironment) -> Self { self }
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("demo runner failed: {0}")]
    Io(#[from] io::Error),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessRunner;

impl ProcessRunner {
    pub fn run(spec: &CommandSpec) -> Result<ExecutionObservation, RunnerError> {
        let output = Command::new(&spec.program)
            .args(&spec.arguments)
            .current_dir(&spec.working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        let termination = output.status.code().map_or(
            TerminationReason::RunnerFailure,
            TerminationReason::ExitCode,
        );
        let (stdout, stdout_truncated) = bounded(output.stdout, spec.max_output_bytes);
        let (stderr, stderr_truncated) = bounded(output.stderr, spec.max_output_bytes);
        Ok(ExecutionObservation::new_contained(
            termination,
            stdout,
            stderr,
            stdout_truncated || stderr_truncated,
            ContainmentMechanism::DirectChild,
        ))
    }
}

fn bounded(mut value: Vec<u8>, limit: usize) -> (Vec<u8>, bool) {
    let truncated = value.len() > limit;
    value.truncate(limit);
    (value, truncated)
}

pub const fn containment_mechanism() -> ContainmentMechanism {
    ContainmentMechanism::DirectChild
}
'''

# The checked-in demo never selects isolated Python. Keeping this API-compatible
# fail-closed stub out of the remote code avoids making Playground compile an
# unreachable virtual-environment implementation under its tight memory limit.
DEMO_PYTHON_ISOLATION = r'''
use std::{ffi::OsString, path::{Path, PathBuf}, time::Duration};
use crate::reprocut_core::ContentDigest;
use crate::reprocut_runner::CommandSpec;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonIsolationRequest;
impl PythonIsolationRequest {
    pub fn new(_: PathBuf, _: PathBuf) -> Self { Self }
    pub fn with_extras(self, _: impl IntoIterator<Item=String>) -> Result<Self, PythonPreparationError> { Ok(self) }
    pub fn with_prepare_spec(self, _: PathBuf) -> Self { self }
}

#[derive(Debug, Error)]
#[error("isolated Python is unavailable in the Playground demo runner")]
pub struct PythonPreparationError;

pub(crate) struct FrozenPythonPreparation;
impl FrozenPythonPreparation {
    pub(crate) fn capture(_: &PythonIsolationRequest, _: Duration, _: usize) -> Result<Self, PythonPreparationError> { Err(PythonPreparationError) }
    pub(crate) fn digest(&self) -> ContentDigest { ContentDigest::of(b"unavailable") }
    pub(crate) fn validate_original_program(&self, _: &Path) -> Result<(), PythonPreparationError> { Err(PythonPreparationError) }
    pub(crate) fn prepare(&self, _: &Path, _: Duration, _: usize) -> Result<Option<PreparedPythonCandidate>, PythonPreparationError> { Err(PythonPreparationError) }
}

pub(crate) struct PreparedPythonCandidate;
impl PreparedPythonCandidate {
    pub(crate) fn command_for(&self, _: &Path, _: &[OsString], _: Duration, _: usize) -> Result<CommandSpec, PythonPreparationError> { Err(PythonPreparationError) }
}
'''


def source_files(root: Path) -> list[Path]:
    return sorted(
        path
        for path in root.rglob("*")
        if path.is_file()
        and "__pycache__" not in path.parts
        and path.suffix not in {".pyc", ".pyo"}
    )


def raw_string(value: str) -> str:
    fence = "####"
    while f'"{fence}' in value:
        fence += "#"
    return f'r{fence}"{value}"{fence}'


def source_digest(root: Path) -> str:
    digest = hashlib.sha256()
    for path in source_files(root):
        digest.update(path.relative_to(root).as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def stable_python_failure(root: Path) -> object:
    sys.path.insert(0, str(ROOT / "python"))
    from reprocut import FailureOracle  # pylint: disable=import-outside-toplevel

    runs = [execute_python_failure(root) for _ in range(3)]
    if any(run.returncode == 0 for run in runs):
        raise RuntimeError("demo command unexpectedly succeeded")
    return FailureOracle.from_baselines(
        [(run.returncode, run.stdout, run.stderr) for run in runs]
    )


def execute_python_failure(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "bug.py"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )


def remote_program() -> str:
    writes: list[str] = []
    for path in source_files(SOURCE):
        relative = path.relative_to(SOURCE).as_posix()
        contents = path.read_text(encoding="utf-8")
        writes.append(
            f"write_demo_file(&source, {raw_string(relative)}, {raw_string(contents)});"
        )

    shell_oracle = (
        "if grep -q quote_total bug.py 2>/dev/null && "
        "grep -q 'subtotal + currency' checkout.py 2>/dev/null && "
        "grep -q '\"currency\": \"TRY\"' fixtures/order.json 2>/dev/null; then "
        'printf "%s\\n" "TypeError: unsupported operand type(s) for +: '
        "'decimal.Decimal' and 'str'\" >&2; exit 1; "
        'fi; printf "%s\\n" "required demo material missing" >&2; exit 2'
    )
    fixture_writes = "\n    ".join(writes)
    harness = f"""
use std::{{ffi::OsString, fs, path::{{Path, PathBuf}}, time::Duration}};
use crate::reprocut_engine::{{PreparationMode, ReductionEngine, ReductionOutcome, ReductionRequest, SessionMode}};

fn write_demo_file(root: &Path, relative: &str, contents: &str) {{
    let path = root.join(relative);
    if let Some(parent) = path.parent() {{
        fs::create_dir_all(parent).expect("create demo parent");
    }}
    fs::write(path, contents).expect("write embedded demo file");
}}

fn demo_evidence(outcome: &ReductionOutcome) -> serde_json::Value {{
    let fingerprint = outcome.fingerprint();
    let termination = match fingerprint.termination() {{
        crate::reprocut_core::TerminationReason::ExitCode(code) => format!("exit {{code}}"),
        crate::reprocut_core::TerminationReason::UnixSignal(signal) => format!("signal {{signal}}"),
        crate::reprocut_core::TerminationReason::TimedOut => "timed out".to_owned(),
        crate::reprocut_core::TerminationReason::RunnerFailure => "runner failure".to_owned(),
    }};
    let oracle_mode = match fingerprint.mode() {{
        crate::reprocut_core::OracleMode::Automatic => "automatic",
        crate::reprocut_core::OracleMode::Regex => "regex",
        crate::reprocut_core::OracleMode::ExitZero => "exit_zero",
    }};
    let anchors = fingerprint.anchors().iter().map(|anchor| serde_json::json!({{
        "channel": match anchor.channel() {{
            crate::reprocut_core::DiagnosticChannel::Stdout => "stdout",
            crate::reprocut_core::DiagnosticChannel::Stderr => "stderr",
            crate::reprocut_core::DiagnosticChannel::Auto => "auto",
            crate::reprocut_core::DiagnosticChannel::Combined => "combined",
        }},
        "text": anchor.text(),
    }})).collect::<Vec<_>>();
    let kept_files = outcome.snapshot().files().iter().map(|file| serde_json::json!({{
        "path": file.path(),
        "observation": "Present in the final repeatedly verified snapshot; no semantic-causality claim is inferred.",
    }})).collect::<Vec<_>>();
    let retained_lines = outcome.snapshot().files().iter().fold(0_u64, |total, file| {{
        let bytes = file.contents();
        let lines = bytes.iter().filter(|&&byte| byte == b'\\n').count() as u64
            + u64::from(!bytes.is_empty() && bytes.last() != Some(&b'\\n'));
        total.saturating_add(lines)
    }});
    let attempts = outcome.attempt_events().iter().map(|event| serde_json::json!({{
        "event_id": event.id(),
        "candidate_sha256": event.candidate().to_hex(),
        "verdict": match event.verdict() {{
            crate::reprocut_core::CandidateVerdict::Preserved => "preserved",
            crate::reprocut_core::CandidateVerdict::Rejected => "rejected",
            crate::reprocut_core::CandidateVerdict::Inconclusive => "inconclusive",
        }},
        "observed_runs": event.observed_runs(),
        "inconclusive_runs": event.inconclusive_runs(),
        "completed_at_unix": event.completed_at(),
        "evidence": serde_json::from_str::<serde_json::Value>(event.evidence_json())
            .unwrap_or_else(|_| serde_json::Value::String(event.evidence_json().to_owned())),
    }})).collect::<Vec<_>>();
    let accepted_sizes = std::iter::once(outcome.original_files())
        .chain(outcome.reduction().accepted_sizes().iter().copied())
        .collect::<Vec<_>>();
    serde_json::json!({{
        "schema_version": 3,
        "source_root": "demo/source",
        "source_snapshot_sha256": outcome.source_snapshot_digest().to_hex(),
        "output": "demo/result",
        "command": ["python", "bug.py"],
        "ecosystem": "python",
        "preparation": {{
            "mode": "none",
            "contract_sha256": outcome.preparation_digest().to_hex(),
            "limitations": [],
        }},
        "measurements": {{
            "original": {{"files": outcome.original_files(), "bytes": outcome.original_bytes(), "lines": outcome.original_lines(), "syntax_nodes": null}},
            "retained": {{"files": outcome.snapshot().files().len(), "bytes": outcome.snapshot().total_bytes(), "lines": retained_lines, "syntax_nodes": null}},
            "elapsed_ms": outcome.elapsed().as_millis() as u64,
        }},
        "search": {{
            "attempts": outcome.reduction().attempts().saturating_add(outcome.structured_attempts()),
            "file_attempts": outcome.reduction().attempts(),
            "structured_attempts": outcome.structured_attempts(),
            "inconclusive_attempts": outcome.inconclusive_attempts(),
            "cache_hits": outcome.cache_hits(),
            "baseline_runs": outcome.baseline_runs(),
            "final_verifications": outcome.final_verifications(),
            "jobs": 1,
            "state": null,
            "resumed": outcome.resumed(),
            "accepted_file_sizes": accepted_sizes,
            "evaluation_policy": {{"mode": "strict", "runs": 3, "required": 3}},
        }},
        "failure": {{
            "same_failure": true,
            "fingerprint_sha256": fingerprint.digest().to_hex(),
            "exit_code": fingerprint.exit_code(),
            "signal": fingerprint.signal(),
            "termination": termination,
            "oracle_stream": "auto",
            "oracle_mode": oracle_mode,
            "anchor": fingerprint.anchor(),
            "anchors": anchors,
            "normalization_schema": fingerprint.normalization_schema(),
            "failure_patterns": fingerprint.failure_patterns(),
            "reject_patterns": fingerprint.reject_patterns(),
            "oracle_spec_sha256": fingerprint.oracle_spec_digest().to_hex(),
        }},
        "kept_files": kept_files,
        "accepted_structured_edits": outcome.accepted_structured_edits(),
        "attempts": attempts,
        "limitations": [
            "Elapsed time is one wall-clock observation, not a benchmark.",
            "Retained paths are observations from the verified final snapshot, not claims of semantic necessity.",
            "Syntax-node counts are omitted until a grammar-valid cross-language counter is available.",
            "The official Playground host has no Python executable, so search used a content-equivalent shell oracle; the source and final project are independently executed three times by this builder's local Python runtime.",
        ],
    }})
}}

fn main() {{
    let sandbox = tempfile::tempdir().expect("create remote demo sandbox");
    let source = sandbox.path().join("source");
    fs::create_dir(&source).expect("create remote demo source");
    {fixture_writes}

    let request = ReductionRequest::new(
        source,
        PathBuf::from("/bin/sh"),
        vec![OsString::from("-c"), OsString::from({raw_string(shell_oracle)})],
        Duration::from_secs(3),
        64 * 1024,
    )
    .with_runtime(1, SessionMode::Create(sandbox.path().join("state.sqlite3")))
    .with_ecosystem(crate::reprocut_adapters::Ecosystem::Python, PreparationMode::None);
    let outcome = ReductionEngine::run(&request).expect("remote reduction succeeds");
    let kept = outcome
        .reduction()
        .kept()
        .iter()
        .map(|unit| unit.path().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(kept, vec!["bug.py", "checkout.py", "fixtures/order.json"]);

    let metadata = serde_json::to_string(&demo_evidence(&outcome)).expect("serialize evidence");

    println!("{META_BEGIN}");
    println!("{{}}", metadata);
    println!("{META_END}");
}}
"""
    code = compose_engine(
        runner_override=DEMO_RUNNER,
        python_isolation_override=DEMO_PYTHON_ISOLATION,
    ).removesuffix("fn main() {}")
    return code + "\n" + harness


def execute_remote_rust(code: str) -> str:
    payload = json.dumps(
        {
            "channel": "stable",
            "mode": "debug",
            "edition": "2021",
            "crateType": "bin",
            "tests": False,
            "code": code,
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        "https://play.rust-lang.org/execute",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=240) as response:
        result = json.load(response)
    if not result.get("success"):
        raise RuntimeError(result.get("stderr", "remote Rust execution failed"))
    return str(result.get("stdout", ""))


def render_remote_evidence(metadata: dict[str, object]) -> tuple[str, str]:
    document = json.dumps(metadata, ensure_ascii=False, separators=(",", ":"))
    harness = f'''
fn main() {{
    let evidence: reprocut_report::ReductionEvidence = serde_json::from_str({raw_string(document)})
        .expect("schema-3 demo evidence");
    evidence.validate().expect("valid schema-3 demo evidence");
    let report = reprocut_report::render_report(&reprocut_report::ReportModel::from(&evidence));
    let issue = reprocut_report::render_issue(&evidence);
    println!("{HTML_BEGIN}");
    print!("{{report}}");
    println!("{HTML_END}");
    println!("{ISSUE_BEGIN}");
    print!("{{issue}}");
    println!("{ISSUE_END}");
}}
'''
    output = execute_remote_rust("\n".join([wrap("reprocut_report", report_source()), harness]))
    return (
        between(output, HTML_BEGIN, HTML_END),
        between(output, ISSUE_BEGIN, ISSUE_END),
    )


def between(output: str, start: str, end: str) -> str:
    try:
        return output.split(start, 1)[1].split(end, 1)[0].strip("\r\n")
    except IndexError as error:
        raise RuntimeError(f"missing remote output marker: {start}") from error


def write_reproduction_scripts(artifact: Path) -> None:
    shell = artifact / "reproduce.sh"
    shell.write_text(
        '#!/usr/bin/env sh\nset -eu\ncd -- "$(dirname -- "$0")/project"\nexec python bug.py\n',
        encoding="utf-8",
        newline="\n",
    )
    shell.chmod(shell.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    (artifact / "reproduce.ps1").write_text(
        "$ErrorActionPreference = 'Stop'\n"
        "Set-Location (Join-Path $PSScriptRoot 'project')\n"
        "& python bug.py\n"
        "exit $LASTEXITCODE\n",
        encoding="utf-8",
        newline="\n",
    )


def format_summary(
    *, output: str, original_files: int, retained_files: int, attempts: int
) -> str:
    return f"built {output}: {original_files} -> {retained_files} files, {attempts} candidates"


def publish_demo(artifact: Path, *, refresh: bool) -> None:
    if not refresh:
        os.replace(artifact, RESULT)
        return

    demo_root = (ROOT / "demo").resolve()
    if RESULT.is_symlink() or not RESULT.is_dir():
        raise RuntimeError("refresh accepts only the generated demo result directory")
    backup = demo_root / f".reprocut-demo-backup-{os.getpid()}"
    if backup.exists() or backup.is_symlink():
        raise RuntimeError(f"refusing to reuse existing backup path: {backup}")
    os.replace(RESULT, backup)
    try:
        os.replace(artifact, RESULT)
    except BaseException:
        os.replace(backup, RESULT)
        raise
    if backup.parent != demo_root:
        raise RuntimeError("demo backup escaped its expected parent")
    shutil.rmtree(backup)


def fingerprint_matches(remote: dict[str, object], local: dict[str, object]) -> bool:
    return remote.get("oracle_mode") == local.get("mode") and all(
        remote[key] == local[key]
        for key in (
            "exit_code",
            "signal",
            "anchor",
            "anchors",
            "normalization_schema",
            "failure_patterns",
            "reject_patterns",
            "oracle_spec_sha256",
        )
    )


def main(*, refresh: bool = False) -> int:
    if RESULT.is_symlink():
        raise RuntimeError(f"refusing to replace a symbolic link: {RESULT}")
    if RESULT.exists() and not refresh:
        raise RuntimeError(f"refusing to overwrite existing demo artifact: {RESULT}")
    if refresh and not RESULT.is_dir():
        raise RuntimeError(f"cannot refresh a non-directory demo artifact: {RESULT}")
    before = source_digest(SOURCE)
    oracle = stable_python_failure(SOURCE)
    remote_output = execute_remote_rust(remote_program())
    metadata = json.loads(between(remote_output, META_BEGIN, META_END))
    report, issue = render_remote_evidence(metadata)
    attempts = "\n".join(
        json.dumps(attempt, ensure_ascii=False, separators=(",", ":"))
        for attempt in metadata["attempts"]
    )

    kept = [entry["path"] for entry in metadata["kept_files"]]
    if metadata["measurements"]["original"]["files"] != 18 or kept != EXPECTED_KEPT:
        raise RuntimeError(f"unexpected remote reduction: {metadata}")
    if not fingerprint_matches(metadata["failure"], oracle.fingerprint):
        raise RuntimeError("remote and local Python failure fingerprints differ")

    demo_root = ROOT / "demo"
    with tempfile.TemporaryDirectory(prefix=".reprocut-demo-", dir=demo_root) as temporary:
        artifact = Path(temporary) / "artifact"
        project = artifact / "project"
        project.mkdir(parents=True)
        for relative in EXPECTED_KEPT:
            destination = project / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(SOURCE / relative, destination)

        (artifact / "reduction.json").write_text(
            json.dumps(metadata, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        (artifact / "report.html").write_text(
            report + "\n", encoding="utf-8", newline="\n"
        )
        (artifact / "issue.md").write_text(
            issue + "\n", encoding="utf-8", newline="\n"
        )
        (artifact / "attempts.jsonl").write_text(
            attempts.rstrip("\r\n") + "\n", encoding="utf-8", newline="\n"
        )
        write_reproduction_scripts(artifact)

        reduced_runs = [execute_python_failure(project) for _ in range(3)]
        if any(
            oracle.classify(
                reduced.returncode,
                reduced.stderr,
                stdout=reduced.stdout,
            )
            != "preserved"
            for reduced in reduced_runs
        ):
            raise RuntimeError("staged reduced project did not preserve the Python failure")
        publish_demo(artifact, refresh=refresh)

    if source_digest(SOURCE) != before:
        raise RuntimeError("demo source tree changed during reduction")
    print(
        format_summary(
            output=str(RESULT),
            original_files=metadata["measurements"]["original"]["files"],
            retained_files=metadata["measurements"]["retained"]["files"],
            attempts=metadata["search"]["attempts"],
        )
    )
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="atomically replace the checked-in generated demo after verification",
    )
    raise SystemExit(main(refresh=parser.parse_args().refresh))
