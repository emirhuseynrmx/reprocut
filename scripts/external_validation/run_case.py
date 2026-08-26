#!/usr/bin/env python3
"""Trusted host-side boundary for external ReproCut validation."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import stat
import subprocess
from pathlib import Path
from typing import Sequence

from validate_cases import CaseSpec


MAX_EVIDENCE_BYTES = 1024 * 1024 * 1024


class EvidenceError(RuntimeError):
    """Candidate evidence is unsafe or violates the evidence contract."""


class CommandError(RuntimeError):
    """A trusted host command failed with captured diagnostics."""


def run_argv(
    argv: Sequence[str],
    *,
    cwd: Path | None = None,
    timeout: float | None = None,
    check: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Run an argv vector without invoking a command shell."""
    completed = subprocess.run(
        list(argv),
        cwd=cwd,
        timeout=timeout,
        check=False,
        shell=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and completed.returncode != 0:
        diagnostic = completed.stderr.strip() or completed.stdout.strip() or "no diagnostic output"
        raise CommandError(f"command exited {completed.returncode}: {diagnostic}")
    return completed


def docker_create_argv(case: CaseSpec, image: str) -> list[str]:
    """Return the complete no-network, no-mount Docker create invocation."""
    return [
        "docker",
        "create",
        "--name",
        f"reprocut-validation-{case.case_id}",
        "--network",
        "none",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--pids-limit",
        "1024",
        "--cpus",
        "2",
        "--memory",
        case.memory,
        "--memory-swap",
        case.memory,
        "--user",
        "10001:10001",
        "--read-only",
        "--tmpfs",
        "/work:rw,exec,nosuid,nodev,size=12g",
        "--tmpfs",
        "/tmp:rw,exec,nosuid,nodev,size=2g",
        image,
    ]


def _inventory_regular_files(source: Path) -> list[tuple[str, Path, int]]:
    if not source.is_dir():
        raise EvidenceError(f"evidence source is not a directory: {source}")
    entries: list[tuple[str, Path, int]] = []
    total_bytes = 0
    for root, directory_names, file_names in os.walk(source, topdown=True, followlinks=False):
        root_path = Path(root)
        for name in directory_names:
            path = root_path / name
            mode = path.lstat().st_mode
            if stat.S_ISLNK(mode):
                raise EvidenceError(f"evidence contains symlink: {path.relative_to(source)}")
            if not stat.S_ISDIR(mode):
                raise EvidenceError(f"evidence contains non-directory entry: {path.relative_to(source)}")
        for name in file_names:
            path = root_path / name
            metadata = path.lstat()
            relative = path.relative_to(source).as_posix()
            if stat.S_ISLNK(metadata.st_mode):
                raise EvidenceError(f"evidence contains symlink: {relative}")
            if not stat.S_ISREG(metadata.st_mode):
                raise EvidenceError(f"evidence contains non-regular file: {relative}")
            if relative == "integrity.json":
                raise EvidenceError("raw evidence may not supply integrity.json")
            total_bytes += metadata.st_size
            if total_bytes > MAX_EVIDENCE_BYTES:
                raise EvidenceError("evidence exceeds the 1 GiB size ceiling")
            entries.append((relative, path, metadata.st_size))
    entries.sort(key=lambda entry: entry[0])
    if len({relative for relative, _, _ in entries}) != len(entries):
        raise EvidenceError("evidence contains duplicate normalized paths")
    return entries


def sanitize_evidence(source: Path, destination: Path) -> dict[str, str]:
    """Copy only bounded regular files and append a trusted integrity envelope."""
    if destination.exists():
        raise EvidenceError(f"evidence destination already exists: {destination}")
    entries = _inventory_regular_files(source)
    destination.mkdir(parents=True)
    inventory: dict[str, str] = {}
    for relative, source_path, expected_size in entries:
        destination_path = destination / Path(relative)
        destination_path.parent.mkdir(parents=True, exist_ok=True)
        digest = hashlib.sha256()
        copied = 0
        with source_path.open("rb") as input_file, destination_path.open("xb") as output_file:
            while chunk := input_file.read(1024 * 1024):
                digest.update(chunk)
                output_file.write(chunk)
                copied += len(chunk)
        if copied != expected_size:
            raise EvidenceError(f"evidence file changed while copying: {relative}")
        inventory[relative] = digest.hexdigest()
    envelope = {"algorithm": "sha256", "files": inventory, "schema_version": 1}
    (destination / "integrity.json").write_text(
        json.dumps(envelope, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return inventory
