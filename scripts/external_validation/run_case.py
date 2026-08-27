#!/usr/bin/env python3
"""Trusted host-side boundary for external ReproCut validation."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import subprocess
import tempfile
from dataclasses import asdict
from pathlib import Path
from typing import Sequence

from validate_cases import CaseSpec, load_cases, select_case


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
        "/work:rw,exec,nosuid,nodev,size=12g,uid=10001,gid=10001,mode=1770",
        "--tmpfs",
        "/tmp:rw,exec,nosuid,nodev,size=2g,uid=10001,gid=10001,mode=1770",
        "--mount",
        "type=volume,destination=/evidence",
        image,
    ]


def docker_remove_argv(container_name: str) -> list[str]:
    """Remove a validation container and its anonymous evidence volume."""
    return ["docker", "rm", "--force", "--volumes", container_name]


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


def prepare_build_context(
    *,
    case: CaseSpec,
    repo_root: Path,
    base_snapshot: Path,
    head_snapshot: Path,
    destination: Path,
    base_sha: str,
    reprocut_sha: str,
) -> None:
    """Assemble a bounded Docker context from pinned, metadata-free inputs."""
    if destination.exists():
        raise EvidenceError(f"build context already exists: {destination}")
    destination.mkdir(parents=True)
    shutil.copytree(
        repo_root,
        destination / "reprocut",
        ignore=shutil.ignore_patterns(".git", "target", "external-validation-output", "__pycache__"),
    )
    shutil.copytree(base_snapshot, destination / "base", ignore=shutil.ignore_patterns(".git"))
    shutil.copytree(head_snapshot, destination / "head", ignore=shutil.ignore_patterns(".git"))
    document = asdict(case)
    document.update({"base_sha": base_sha, "reprocut_sha": reprocut_sha, "schema_version": 1})
    (destination / "case.json").write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def materialize_snapshots(case: CaseSpec, workspace: Path) -> tuple[Path, Path, str]:
    """Fetch the pinned head and resolve one immutable base snapshot."""
    repository = workspace / "repository"
    base_snapshot = workspace / "base-snapshot"
    head_snapshot = workspace / "head-snapshot"
    run_argv(["git", "init", str(repository)], check=True)
    run_argv(["git", "-C", str(repository), "remote", "add", "origin", case.repository], check=True)
    run_argv(
        ["git", "-C", str(repository), "fetch", "--depth", "1", "--no-tags", "origin", case.head_sha],
        check=True,
        timeout=600,
    )
    fetched_head = run_argv(
        ["git", "-C", str(repository), "rev-parse", "FETCH_HEAD"], check=True
    ).stdout.strip()
    if fetched_head != case.head_sha:
        raise CommandError(f"fetched head {fetched_head} does not match pinned {case.head_sha}")
    run_argv(
        ["git", "-C", str(repository), "fetch", "--depth", "1", "--no-tags", "origin", case.base_ref],
        check=True,
        timeout=600,
    )
    base_sha = run_argv(
        ["git", "-C", str(repository), "rev-parse", "FETCH_HEAD"], check=True
    ).stdout.strip()
    run_argv(
        ["git", "-C", str(repository), "worktree", "add", "--detach", str(base_snapshot), base_sha],
        check=True,
    )
    run_argv(
        ["git", "-C", str(repository), "worktree", "add", "--detach", str(head_snapshot), case.head_sha],
        check=True,
    )
    return base_snapshot, head_snapshot, base_sha


def execute_case(case: CaseSpec, repo_root: Path, output: Path) -> None:
    """Build and run one isolated validation case, preserving evidence on failure."""
    if output.exists():
        raise EvidenceError(f"output already exists: {output}")
    docker_probe = run_argv(["docker", "version", "--format", "{{.Server.Version}}"])
    if docker_probe.returncode != 0:
        raise CommandError(f"Docker is unavailable: {docker_probe.stderr.strip()}")
    reprocut_sha = run_argv(["git", "-C", str(repo_root), "rev-parse", "HEAD"], check=True).stdout.strip()
    image = f"reprocut-validation:{case.case_id}-{reprocut_sha[:12]}"
    container_name = f"reprocut-validation-{case.case_id}"

    with tempfile.TemporaryDirectory(prefix=f"reprocut-{case.case_id}-") as temporary:
        workspace = Path(temporary)
        base_snapshot, head_snapshot, base_sha = materialize_snapshots(case, workspace)
        context = workspace / "context"
        prepare_build_context(
            case=case,
            repo_root=repo_root,
            base_snapshot=base_snapshot,
            head_snapshot=head_snapshot,
            destination=context,
            base_sha=base_sha,
            reprocut_sha=reprocut_sha,
        )
        dockerfile = context / "reprocut" / "scripts" / "external_validation" / "Dockerfile"
        build = run_argv(
            [
                "docker", "build", "--pull", "--file", str(dockerfile),
                "--build-arg", f"CASE_ID={case.case_id}", "--tag", image, str(context),
            ],
            timeout=max(1800, case.timeout_minutes * 60),
        )
        print(build.stdout, end="")
        print(build.stderr, end="", file=os.sys.stderr)
        if build.returncode != 0:
            raise CommandError(f"Docker image build failed with exit code {build.returncode}")

        run_argv(docker_remove_argv(container_name))
        created = False
        container_exit = 125
        try:
            run_argv(docker_create_argv(case, image), check=True)
            created = True
            started = run_argv(
                ["docker", "start", "--attach", container_name],
                timeout=(case.timeout_minutes * 60) + 120,
            )
            container_exit = started.returncode
            print(started.stdout, end="")
            print(started.stderr, end="", file=os.sys.stderr)
            raw = workspace / "raw-evidence"
            raw.mkdir()
            copied = run_argv(
                ["docker", "cp", f"{container_name}:/evidence/.", str(raw)],
                timeout=300,
            )
            if copied.returncode != 0:
                raise CommandError(f"cannot extract evidence: {copied.stderr.strip()}")
            sanitize_evidence(raw, output)
        finally:
            if created:
                run_argv(docker_remove_argv(container_name))
            run_argv(["docker", "image", "rm", "--force", image])
        if container_exit != 0:
            raise CommandError(f"validation container exited {container_exit}; sanitized evidence is at {output}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--catalog", type=Path, default=Path(__file__).with_name("cases.json"))
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    arguments = parser.parse_args()
    case = select_case(load_cases(arguments.catalog), arguments.case)
    execute_case(case, arguments.repo_root.resolve(), arguments.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
