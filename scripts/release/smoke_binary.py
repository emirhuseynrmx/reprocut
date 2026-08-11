#!/usr/bin/env python3
"""Exercise a built ReproCut binary against the real Python failure fixture."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


def tree_digest(root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(
        candidate for candidate in root.rglob("*") if candidate.is_file()
    ):
        digest.update(path.relative_to(root).as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def smoke(
    *, binary: Path, python: str, fixture: Path, version: str, launcher: list[str]
) -> None:
    invocation = [*launcher, str(binary)]
    version_result = subprocess.run(
        [*invocation, "--version"],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=60,
    )
    if version_result.returncode != 0 or version not in version_result.stdout:
        raise RuntimeError(
            f"release binary version smoke failed: {version_result.stderr}"
        )

    with tempfile.TemporaryDirectory(prefix="reprocut-release-smoke-") as temporary:
        sandbox = Path(temporary)
        source = sandbox / "source"
        output = sandbox / "minimal"
        state = sandbox / "state.sqlite3"
        shutil.copytree(fixture, source)
        before = tree_digest(source)
        result = subprocess.run(
            [
                *invocation,
                "reduce",
                "--root",
                str(source),
                "--output",
                str(output),
                "--state",
                str(state),
                "--timeout-ms",
                "5000",
                "--",
                python,
                "bug.py",
            ],
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=180,
            env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        )
        if result.returncode != 0:
            raise RuntimeError(f"release reduction smoke failed: {result.stderr}")
        evidence = json.loads((output / "reduction.json").read_text(encoding="utf-8"))
        if (
            evidence["schema_version"] != 2
            or evidence["failure"]["same_failure"] is not True
        ):
            raise RuntimeError("release smoke produced invalid same-failure evidence")
        if evidence["search"]["final_verifications"] != 3:
            raise RuntimeError(
                "release smoke did not complete three final verifications"
            )
        files = sorted(
            path.relative_to(output / "project").as_posix()
            for path in (output / "project").rglob("*")
            if path.is_file()
        )
        if files != ["bug.py"]:
            raise RuntimeError(f"release smoke retained unexpected files: {files}")
        if tree_digest(source) != before:
            raise RuntimeError("release binary modified its source fixture")
        required = {
            "attempts.jsonl",
            "issue.md",
            "reduction.json",
            "report.html",
            "reproduce.ps1",
            "reproduce.sh",
        }
        if not required.issubset(
            path.name for path in output.iterdir() if path.is_file()
        ):
            raise RuntimeError("release smoke artifact is incomplete")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--python", required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--launcher-json", default="[]")
    arguments = parser.parse_args()
    launcher = json.loads(arguments.launcher_json)
    if not isinstance(launcher, list) or not all(
        isinstance(item, str) for item in launcher
    ):
        raise ValueError("launcher JSON must be an array of strings")
    smoke(
        binary=arguments.binary,
        python=arguments.python,
        fixture=arguments.fixture,
        version=arguments.version,
        launcher=launcher,
    )
    print(f"verified release binary: {arguments.binary}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
