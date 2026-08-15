#!/usr/bin/env python3
"""Preflight and resume the immutable ReproCut crates.io publication chain."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

VERSION = "0.1.0"
PUBLISH_ORDER = (
    "reprocut-core",
    "reprocut-report",
    "reprocut-oci",
    "reprocut-workspace",
    "reprocut-runner",
    "reprocut-state",
    "reprocut-syntax",
    "reprocut-adapters",
    "reprocut-engine",
    "reprocut",
)
CRATE_DIRECTORIES = {
    package: ("reprocut-cli" if package == "reprocut" else package) for package in PUBLISH_ORDER
}
PACKAGE_DEPENDENCIES = {
    "reprocut-core": (),
    "reprocut-report": ("reprocut-core",),
    "reprocut-oci": (),
    "reprocut-workspace": ("reprocut-core",),
    "reprocut-runner": ("reprocut-core",),
    "reprocut-state": ("reprocut-core",),
    "reprocut-syntax": ("reprocut-core",),
    "reprocut-adapters": ("reprocut-workspace",),
    "reprocut-engine": (
        "reprocut-adapters",
        "reprocut-core",
        "reprocut-runner",
        "reprocut-state",
        "reprocut-syntax",
        "reprocut-workspace",
    ),
    "reprocut": (
        "reprocut-adapters",
        "reprocut-core",
        "reprocut-engine",
        "reprocut-oci",
        "reprocut-report",
        "reprocut-workspace",
    ),
}
CHECKSUM = re.compile(r"^[0-9a-f]{64}$")
CommandRunner = Callable[[list[str], Path], None]
Sleeper = Callable[[float], None]


class PublishError(RuntimeError):
    """The release chain disagrees with the immutable registry state."""


@dataclass(frozen=True)
class PublishResult:
    package: str
    version: str
    checksum: str
    status: str


class RegistryClient:
    """Bounded, read-only crates.io API client used after Cargo authentication."""

    def __init__(
        self,
        *,
        base_url: str = "https://crates.io/api/v1",
        timeout_seconds: float = 15.0,
    ) -> None:
        if urllib.parse.urlparse(base_url).scheme != "https":
            raise ValueError("registry API must use HTTPS")
        if timeout_seconds <= 0:
            raise ValueError("registry timeout must be positive")
        self.base_url = base_url.rstrip("/")
        self.timeout_seconds = timeout_seconds

    def version(self, package: str, version: str) -> dict[str, object] | None:
        payload = self._get_json(
            f"/crates/{urllib.parse.quote(package, safe='')}/"
            f"{urllib.parse.quote(version, safe='')}",
            allow_not_found=True,
        )
        if payload is None:
            return None
        document = payload.get("version", payload)
        if not isinstance(document, dict):
            raise PublishError(f"registry returned an invalid version document for {package}")
        return document

    def owners(self, package: str) -> set[str]:
        payload = self._get_json(
            f"/crates/{urllib.parse.quote(package, safe='')}/owners",
            allow_not_found=False,
        )
        if payload is None:  # pragma: no cover - disallowed by allow_not_found=False
            raise PublishError(f"registry owner response disappeared for {package}")
        owners: set[str] = set()
        for category in ("users", "teams"):
            entries = payload.get(category, [])
            if not isinstance(entries, list):
                raise PublishError(f"registry returned invalid owners for {package}")
            for entry in entries:
                if isinstance(entry, dict) and isinstance(entry.get("login"), str):
                    owners.add(entry["login"])
        return owners

    def _get_json(self, path: str, *, allow_not_found: bool) -> dict[str, object] | None:
        request = urllib.request.Request(
            f"{self.base_url}{path}",
            headers={"User-Agent": "reprocut-release/0.1.0"},
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout_seconds) as response:
                payload = json.load(response)
        except urllib.error.HTTPError as error:
            if allow_not_found and error.code == 404:
                return None
            raise PublishError(f"registry request failed with HTTP {error.code}: {path}") from error
        except (OSError, json.JSONDecodeError) as error:
            raise PublishError(f"registry request failed: {path}: {error}") from error
        if not isinstance(payload, dict):
            raise PublishError(f"registry returned a non-object response: {path}")
        return payload


def cargo_patch_arguments(package: str) -> list[str]:
    arguments: list[str] = []
    for dependency in PACKAGE_DEPENDENCIES[package]:
        directory = CRATE_DIRECTORIES[dependency]
        arguments.extend(
            [
                "--config",
                f'patch.crates-io.{dependency}.path="crates/{directory}"',
            ]
        )
    return arguments


def preflight(repository: Path, *, run: CommandRunner | None = None) -> list[str]:
    """Verify every packaged crate before the first irreversible upload."""

    repository = repository.resolve()
    runner = run or run_command
    verified: list[str] = []
    for package in PUBLISH_ORDER:
        runner(
            [
                cargo_executable(),
                "package",
                "--locked",
                "-p",
                package,
                *cargo_patch_arguments(package),
            ],
            repository,
        )
        ensure_archive(repository, package)
        verified.append(package)
    return verified


def publish(
    repository: Path,
    *,
    registry: RegistryClient,
    expected_owner: str,
    run: CommandRunner | None = None,
    sleep: Sleeper = time.sleep,
    attempts: int = 30,
    delay_seconds: float = 10.0,
) -> list[PublishResult]:
    """Publish missing crates and prove identical already-published versions."""

    if not expected_owner.strip():
        raise ValueError("expected registry owner cannot be empty")
    if attempts <= 0 or delay_seconds < 0:
        raise ValueError("registry polling bounds are invalid")
    repository = repository.resolve()
    runner = run or run_command
    results: list[PublishResult] = []
    for package in PUBLISH_ORDER:
        archive = ensure_archive(repository, package)
        local_checksum = sha256_file(archive)
        remote = registry.version(package, VERSION)
        if remote is None:
            runner(
                [cargo_executable(), "publish", "--locked", "-p", package],
                repository,
            )
            remote = wait_for_version(
                registry,
                package,
                attempts=attempts,
                delay_seconds=delay_seconds,
                sleep=sleep,
            )
            status = "published"
        else:
            status = "skipped"
        verify_remote(
            registry,
            package=package,
            document=remote,
            local_checksum=local_checksum,
            expected_owner=expected_owner,
        )
        results.append(PublishResult(package, VERSION, local_checksum, status))
    return results


def wait_for_version(
    registry: RegistryClient,
    package: str,
    *,
    attempts: int,
    delay_seconds: float,
    sleep: Sleeper,
) -> dict[str, object]:
    for attempt in range(1, attempts + 1):
        document = registry.version(package, VERSION)
        if document is not None:
            return document
        if attempt < attempts:
            sleep(delay_seconds)
    raise PublishError(f"registry propagation timed out for {package} {VERSION}")


def verify_remote(
    registry: RegistryClient,
    *,
    package: str,
    document: dict[str, object],
    local_checksum: str,
    expected_owner: str,
) -> None:
    remote_checksum = document.get("checksum")
    if not isinstance(remote_checksum, str) or CHECKSUM.fullmatch(remote_checksum) is None:
        raise PublishError(f"registry checksum is missing or malformed for {package}")
    if remote_checksum != local_checksum:
        raise PublishError(
            f"registry checksum differs for immutable {package} {VERSION}: "
            f"local={local_checksum}, registry={remote_checksum}"
        )
    owners = registry.owners(package)
    if expected_owner not in owners:
        raise PublishError(
            f"expected owner {expected_owner!r} is absent for {package}; "
            f"registry owners={sorted(owners)}"
        )


def ensure_archive(repository: Path, package: str) -> Path:
    archive = repository / "target" / "package" / f"{package}-{VERSION}.crate"
    if archive.is_symlink() or not archive.is_file() or archive.stat().st_size == 0:
        raise PublishError(f"verified package archive is missing or invalid: {archive}")
    license_path = repository / "LICENSE"
    if license_path.is_symlink() or not license_path.is_file():
        raise PublishError(f"canonical LICENSE is missing or invalid: {license_path}")
    expected_license = license_path.read_bytes()
    member_name = f"{package}-{VERSION}/LICENSE"
    try:
        with tarfile.open(archive, "r:gz") as package_archive:
            try:
                member = package_archive.getmember(member_name)
            except KeyError as error:
                raise PublishError(f"{package} package does not contain LICENSE") from error
            if not member.isfile():
                raise PublishError(f"{package} package LICENSE is not a regular file")
            source = package_archive.extractfile(member)
            if source is None or source.read(len(expected_license) + 1) != expected_license:
                raise PublishError(f"{package} package LICENSE differs from repository LICENSE")
    except (OSError, tarfile.TarError) as error:
        raise PublishError(f"cannot verify {package} package LICENSE: {error}") from error
    return archive


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def cargo_executable() -> str:
    return os.environ.get("CARGO", "cargo")


def run_command(command: list[str], repository: Path) -> None:
    subprocess.run(command, cwd=repository, check=True)


def parse_arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subcommands = parser.add_subparsers(dest="action", required=True)
    preflight_parser = subcommands.add_parser("preflight")
    preflight_parser.add_argument("--repository", type=Path, default=Path.cwd())
    publish_parser = subcommands.add_parser("publish")
    publish_parser.add_argument("--repository", type=Path, default=Path.cwd())
    publish_parser.add_argument("--expected-owner", required=True)
    publish_parser.add_argument("--attempts", type=int, default=30)
    publish_parser.add_argument("--delay-seconds", type=float, default=10.0)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    arguments = parse_arguments(argv)
    try:
        if arguments.action == "preflight":
            for package in preflight(arguments.repository):
                print(f"verified {package} {VERSION}")
        else:
            results = publish(
                arguments.repository,
                registry=RegistryClient(),
                expected_owner=arguments.expected_owner,
                attempts=arguments.attempts,
                delay_seconds=arguments.delay_seconds,
            )
            for result in results:
                print(f"{result.status} {result.package} {result.version} sha256={result.checksum}")
    except (OSError, PublishError, subprocess.CalledProcessError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
