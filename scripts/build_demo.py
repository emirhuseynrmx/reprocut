#!/usr/bin/env python3
"""Build the measured demo artifact through the official Rust Playground."""

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


def stable_python_failure(root: Path) -> tuple[object, str]:
    sys.path.insert(0, str(ROOT / "python"))
    from reprocut import FailureOracle  # pylint: disable=import-outside-toplevel

    runs = [
        subprocess.run(
            [sys.executable, "bug.py"],
            cwd=root,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        )
        for _ in range(3)
    ]
    if any(run.returncode == 0 for run in runs):
        raise RuntimeError("demo command unexpectedly succeeded")
    oracle = FailureOracle.from_baselines([(run.returncode, run.stderr) for run in runs])
    return oracle, runs[0].stderr


def remote_program() -> str:
    writes: list[str] = []
    for path in source_files(SOURCE):
        relative = path.relative_to(SOURCE).as_posix()
        contents = path.read_text(encoding="utf-8")
        writes.append(f"write_demo_file(&source, {raw_string(relative)}, {raw_string(contents)});")

    shell_oracle = (
        "if [ -f bug.py ] && [ -f checkout.py ] && "
        "[ -f fixtures/order.json ]; then "
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
    );
    let outcome = ReductionEngine::run(&request).expect("remote reduction succeeds");
    let kept = outcome
        .reduction()
        .kept()
        .iter()
        .map(|unit| unit.path().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(kept, vec!["bug.py", "checkout.py", "fixtures/order.json"]);

    let mut stages = Vec::with_capacity(outcome.reduction().accepted_sizes().len() + 1);
    stages.push(outcome.original_files());
    stages.extend_from_slice(outcome.reduction().accepted_sizes());
    let fingerprint = outcome.fingerprint();
    let fingerprint_text = format!(
        "exit {{}} · {{}}",
        fingerprint.exit_code().expect("demo exits with a code"),
        fingerprint.anchor()
    );
    let report = render_report(&ReportModel {{
        command: "python bug.py".to_owned(),
        original_files: outcome.original_files(),
        retained_files: kept.len(),
        attempts: outcome.reduction().attempts(),
        inconclusive_attempts: outcome.inconclusive_attempts(),
        cache_hits: outcome.cache_hits(),
        accepted_sizes: stages.clone(),
        fingerprint: fingerprint_text,
        kept_files: kept.clone(),
    }});
    let metadata = serde_json::json!({{
        "schema_version": 1,
        "original_files": outcome.original_files(),
        "retained_files": kept.len(),
        "attempts": outcome.reduction().attempts(),
        "baseline_runs": outcome.baseline_runs(),
        "final_verifications": outcome.final_verifications(),
        "inconclusive_attempts": outcome.inconclusive_attempts(),
        "cache_hits": outcome.cache_hits(),
        "accepted_sizes": stages,
        "kept_files": kept,
        "fingerprint": {{
            "exit_code": fingerprint.exit_code(),
            "signal": fingerprint.signal(),
            "anchor": fingerprint.anchor(),
        }}
    }});

    println!("{META_BEGIN}");
    println!("{{}}", metadata);
    println!("{META_END}");
    println!("{HTML_BEGIN}");
    print!("{{report}}");
    println!("{HTML_END}");
}}
"""
    code = compose_cli().replace("fn main() -> ExitCode {", "fn cli_entry() -> ExitCode {", 1)
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


def format_summary(*, output: str, original_files: int, retained_files: int, attempts: int) -> str:
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


def main(*, refresh: bool = False) -> int:
    if RESULT.is_symlink():
        raise RuntimeError(f"refusing to replace a symbolic link: {RESULT}")
    if RESULT.exists() and not refresh:
        raise RuntimeError(f"refusing to overwrite existing demo artifact: {RESULT}")
    if refresh and not RESULT.is_dir():
        raise RuntimeError(f"cannot refresh a non-directory demo artifact: {RESULT}")
    before = source_digest(SOURCE)
    oracle, _diagnostic = stable_python_failure(SOURCE)
    remote_output = execute_remote_rust(remote_program())
    metadata = json.loads(between(remote_output, META_BEGIN, META_END))
    report = between(remote_output, HTML_BEGIN, HTML_END)

    if metadata["original_files"] != 18 or metadata["kept_files"] != EXPECTED_KEPT:
        raise RuntimeError(f"unexpected remote reduction: {metadata}")
    if metadata["fingerprint"] != oracle.fingerprint:
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

        metadata.update(
            {
                "source_root": "demo/source",
                "output": "demo/result",
                "command": ["python", "bug.py"],
            }
        )
        (artifact / "reduction.json").write_text(
            json.dumps(metadata, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        (artifact / "report.html").write_text(report + "\n", encoding="utf-8", newline="\n")
        write_reproduction_scripts(artifact)

        reduced = subprocess.run(
            [sys.executable, "bug.py"],
            cwd=project,
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        )
        if oracle.classify(reduced.returncode, reduced.stderr) != "preserved":
            raise RuntimeError("staged reduced project did not preserve the Python failure")
        publish_demo(artifact, refresh=refresh)

    if source_digest(SOURCE) != before:
        raise RuntimeError("demo source tree changed during reduction")
    print(
        format_summary(
            output=str(RESULT),
            original_files=metadata["original_files"],
            retained_files=metadata["retained_files"],
            attempts=metadata["attempts"],
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
