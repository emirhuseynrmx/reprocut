from __future__ import annotations

import io
import json
import sys
import tarfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "release"))

from build_manifest import build_manifest
from package_binary import PackageRequest, package_binary
from verify_archive import ArchiveError, verify_archive

TARGET = "x86_64-unknown-linux-gnu"


def fixture_request(tmp_path: Path, output: Path) -> PackageRequest:
    binary = tmp_path / "reprocut"
    binary.write_bytes(b"#!/bin/sh\necho reprocut 0.1.0\n")
    completions = tmp_path / "completions"
    completions.mkdir(exist_ok=True)
    for name in ("reprocut.bash", "_reprocut", "reprocut.fish", "_reprocut.ps1"):
        (completions / name).write_text(f"completion {name}\n", encoding="utf-8")
    return PackageRequest(
        binary=binary,
        completions=completions,
        repository=ROOT,
        output=output,
        target=TARGET,
        version="0.1.0",
        source_revision="a" * 40,
        source_date_epoch=1_700_000_000,
    )


def test_archive_is_reproducible_bounded_and_self_describing(tmp_path: Path) -> None:
    first = package_binary(fixture_request(tmp_path, tmp_path / "first"))
    second = package_binary(fixture_request(tmp_path, tmp_path / "second"))

    assert first.read_bytes() == second.read_bytes()
    result = verify_archive(first, expected_target=TARGET, expected_version="0.1.0")
    assert result.binary == "reprocut"
    assert result.source_revision == "a" * 40
    assert result.members == (
        "LICENSE-APACHE",
        "LICENSE-MIT",
        "README.md",
        "VERSION.json",
        "completions/_reprocut",
        "completions/_reprocut.ps1",
        "completions/reprocut.bash",
        "completions/reprocut.fish",
        "reprocut",
    )
    with tarfile.open(first, "r:gz") as archive:
        version = json.load(
            archive.extractfile(f"reprocut-0.1.0-{TARGET}/VERSION.json")
        )
    assert version["source_date_epoch"] == 1_700_000_000


def test_packaging_refuses_overwrite_and_unsupported_target(tmp_path: Path) -> None:
    request = fixture_request(tmp_path, tmp_path / "dist")
    package_binary(request)
    with pytest.raises(FileExistsError):
        package_binary(request)
    with pytest.raises(ValueError, match="unsupported release target"):
        package_binary(
            PackageRequest(
                **{**request.__dict__, "target": "javascript-unknown-browser"}
            )
        )


def test_verifier_rejects_path_traversal_before_extraction(tmp_path: Path) -> None:
    malicious = tmp_path / "malicious.tar.gz"
    with tarfile.open(malicious, "w:gz") as archive:
        member = tarfile.TarInfo("root/../../owned")
        member.size = 1
        archive.addfile(member, io.BytesIO(b"x"))
    with pytest.raises(ArchiveError, match="unsafe archive member"):
        verify_archive(malicious, expected_target=TARGET, expected_version="0.1.0")


def test_windows_target_is_a_verified_zip_with_executable_metadata(
    tmp_path: Path,
) -> None:
    request = fixture_request(tmp_path, tmp_path / "windows")
    request = PackageRequest(**{**request.__dict__, "target": "x86_64-pc-windows-msvc"})
    archive = package_binary(request)

    assert archive.suffix == ".zip"
    result = verify_archive(
        archive,
        expected_target="x86_64-pc-windows-msvc",
        expected_version="0.1.0",
    )
    assert result.binary == "reprocut.exe"


def test_manifest_binds_archive_sbom_checksum_and_revision(tmp_path: Path) -> None:
    archive = package_binary(fixture_request(tmp_path, tmp_path / "artifacts"))
    sbom = archive.with_name(f"{archive.name}.spdx.json")
    sbom.write_text(
        json.dumps({"spdxVersion": "SPDX-2.3", "name": archive.name}),
        encoding="utf-8",
    )

    checksums, manifest_path = build_manifest(
        archive.parent,
        tmp_path / "aggregate",
        version="0.1.0",
        source_revision="a" * 40,
        expected_targets={TARGET},
    )

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    assert manifest["source_revision"] == "a" * 40
    assert manifest["artifacts"][0]["target"] == TARGET
    lines = checksums.read_text(encoding="utf-8").splitlines()
    assert len(lines) == 2
    assert any(archive.name in line for line in lines)
    assert any(sbom.name in line for line in lines)
