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

from playground_workspace_verify import ROOT, compose_cli

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
fn write_demo_file(root: &Path, relative: &str, contents: &str) {{
    let path = root.join(relative);
    if let Some(parent) = path.parent() {{
        fs::create_dir_all(parent).expect("create demo parent");
    }}
    fs::write(path, contents).expect("write embedded demo file");
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
    .with_runtime(1, SessionMode::Create(sandbox.path().join("state.sqlite3")));
    let outcome = ReductionEngine::run(&request).expect("remote reduction succeeds");
    let kept = outcome
        .reduction()
        .kept()
        .iter()
        .map(|unit| unit.path().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(kept, vec!["bug.py", "checkout.py", "fixtures/order.json"]);

    let arguments = ReduceArgs {{
        root: PathBuf::from("demo/source"),
        output: PathBuf::from("demo/result"),
        ecosystem: EcosystemArg::Python,
        prepare: PrepareArg::None,
        timeout_ms: 3_000,
        max_output_bytes: 64 * 1_024,
        oracle_stream: OracleStreamArg::Auto,
        flaky: false,
        flaky_runs: None,
        flaky_required: None,
        json: true,
        jobs: 1,
        state: None,
        restart: false,
        command: vec!["python".to_owned(), "bug.py".to_owned()],
    }};
    let mut evidence = build_evidence(&arguments, &outcome);
    evidence.source_root = "demo/source".to_owned();
    evidence.output = "demo/result".to_owned();
    evidence.search.state = None;
    evidence.limitations.push(
        "The official Playground host has no Python executable, so search used a content-equivalent shell oracle; the source and final project are independently executed three times by this builder's local Python runtime."
            .to_owned(),
    );
    let report = render_report(&ReportModel::from(&evidence));
    let issue = render_issue(&evidence);
    let metadata = serde_json::to_string(&evidence).expect("serialize evidence");
    let mut attempts = Vec::new();
    write_attempts_jsonl(&evidence.attempts, &mut attempts).expect("serialize attempts");
    let attempts = String::from_utf8(attempts).expect("attempts are UTF-8");

    println!("{META_BEGIN}");
    println!("{{}}", metadata);
    println!("{META_END}");
    println!("{HTML_BEGIN}");
    print!("{{report}}");
    println!("{HTML_END}");
    println!("{ISSUE_BEGIN}");
    print!("{{issue}}");
    println!("{ISSUE_END}");
    println!("{ATTEMPTS_BEGIN}");
    print!("{{attempts}}");
    println!("{ATTEMPTS_END}");
}}
"""
    code = compose_cli(runner_override=DEMO_RUNNER).replace(
        "fn main() -> ExitCode {", "fn cli_entry() -> ExitCode {", 1
    )
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
    return all(
        remote[key] == local[key]
        for key in ("exit_code", "signal", "anchor", "anchors")
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
    report = between(remote_output, HTML_BEGIN, HTML_END)
    issue = between(remote_output, ISSUE_BEGIN, ISSUE_END)
    attempts = between(remote_output, ATTEMPTS_BEGIN, ATTEMPTS_END)

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
