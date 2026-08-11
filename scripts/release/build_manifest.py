#!/usr/bin/env python3
"""Aggregate verified release archives, SPDX SBOMs, and SHA-256 checksums."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path

from package_binary import SUPPORTED_TARGETS
from verify_archive import verify_archive


def build_manifest(
    input_root: Path,
    output_root: Path,
    *,
    version: str,
    source_revision: str,
    expected_targets: set[str] | None = None,
) -> tuple[Path, Path]:
    expected = (
        set(SUPPORTED_TARGETS) if expected_targets is None else set(expected_targets)
    )
    archives: dict[str, Path] = {}
    for candidate in sorted(input_root.rglob("reprocut-*")):
        if not candidate.is_file() or candidate.is_symlink():
            continue
        for target in expected:
            root = f"reprocut-{version}-{target}"
            suffix = ".zip" if SUPPORTED_TARGETS[target][0] == "zip" else ".tar.gz"
            if candidate.name == f"{root}{suffix}":
                if target in archives:
                    raise ValueError(f"duplicate release archive for {target}")
                archives[target] = candidate
    if set(archives) != expected:
        raise ValueError(
            f"release target set differs; missing={sorted(expected - set(archives))}, "
            f"extra={sorted(set(archives) - expected)}"
        )

    records = []
    checksums: dict[str, str] = {}
    for target in sorted(archives):
        archive = archives[target]
        verified = verify_archive(
            archive, expected_target=target, expected_version=version
        )
        if verified.source_revision != source_revision:
            raise ValueError(f"source revision mismatch for {target}")
        sbom = archive.with_name(f"{archive.name}.spdx.json")
        if not sbom.is_file() or sbom.is_symlink():
            raise ValueError(f"missing SPDX SBOM for {archive.name}")
        validate_spdx(sbom)
        archive_sha = sha256(archive)
        sbom_sha = sha256(sbom)
        checksums[archive.name] = archive_sha
        checksums[sbom.name] = sbom_sha
        records.append(
            {
                "target": target,
                "archive": archive.name,
                "archive_sha256": archive_sha,
                "archive_bytes": archive.stat().st_size,
                "sbom": sbom.name,
                "sbom_sha256": sbom_sha,
                "sbom_bytes": sbom.stat().st_size,
            }
        )

    output_root.mkdir(parents=True, exist_ok=True)
    checksums_path = output_root / "SHA256SUMS"
    manifest_path = output_root / "release-manifest.json"
    for path in (checksums_path, manifest_path):
        if path.exists() or path.is_symlink():
            raise FileExistsError(f"release aggregate already exists: {path}")
    checksum_text = "".join(
        f"{digest}  {name}\n" for name, digest in sorted(checksums.items())
    )
    manifest = {
        "schema_version": 1,
        "name": "reprocut",
        "version": version,
        "source_revision": source_revision,
        "artifacts": records,
    }
    try:
        atomic_write(checksums_path, checksum_text.encode("utf-8"))
        atomic_write(
            manifest_path,
            (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode("utf-8"),
        )
    except BaseException:
        checksums_path.unlink(missing_ok=True)
        manifest_path.unlink(missing_ok=True)
        raise
    return checksums_path, manifest_path


def validate_spdx(path: Path) -> None:
    try:
        value = json.loads(path.read_bytes())
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid SPDX JSON: {path}") from error
    if not isinstance(value, dict) or not str(value.get("spdxVersion", "")).startswith(
        "SPDX-"
    ):
        raise ValueError(f"SBOM is not an SPDX JSON document: {path}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def atomic_write(path: Path, contents: bytes) -> None:
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    try:
        with temporary.open("xb") as stream:
            stream.write(contents)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-revision", required=True)
    arguments = parser.parse_args()
    checksums, manifest = build_manifest(
        arguments.input,
        arguments.output,
        version=arguments.version,
        source_revision=arguments.source_revision,
    )
    print(checksums)
    print(manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
