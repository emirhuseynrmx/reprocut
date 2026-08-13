#!/usr/bin/env python3
"""Create one deterministic, self-describing ReproCut binary archive."""

from __future__ import annotations

import argparse
import gzip
import json
import os
import re
import stat
import tarfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

SUPPORTED_TARGETS = {
    "x86_64-unknown-linux-gnu": ("tar.gz", "reprocut"),
    "x86_64-unknown-linux-musl": ("tar.gz", "reprocut"),
    "aarch64-unknown-linux-gnu": ("tar.gz", "reprocut"),
    "x86_64-pc-windows-msvc": ("zip", "reprocut.exe"),
    "x86_64-apple-darwin": ("tar.gz", "reprocut"),
    "aarch64-apple-darwin": ("tar.gz", "reprocut"),
}
COMPLETIONS = (
    "_reprocut",
    "_reprocut.ps1",
    "reprocut.bash",
    "reprocut.fish",
)
REVISION = re.compile(r"^[0-9a-f]{40}$")
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$")
MAX_INPUT_BYTES = 256 * 1024 * 1024


@dataclass(frozen=True)
class PackageRequest:
    binary: Path
    completions: Path
    repository: Path
    output: Path
    target: str
    version: str
    source_revision: str
    source_date_epoch: int


def package_binary(request: PackageRequest) -> Path:
    archive_format, binary_name = target_layout(request.target)
    validate_request(request)
    root = f"reprocut-{request.version}-{request.target}"
    suffix = ".zip" if archive_format == "zip" else ".tar.gz"
    if request.output.is_symlink():
        raise ValueError(f"release output cannot be a symbolic link: {request.output}")
    output = request.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"{root}{suffix}"
    if archive.exists() or archive.is_symlink():
        raise FileExistsError(f"release archive already exists: {archive}")

    version = {
        "schema_version": 1,
        "name": "reprocut",
        "version": request.version,
        "target": request.target,
        "binary": binary_name,
        "source_revision": request.source_revision,
        "source_date_epoch": request.source_date_epoch,
    }
    files = [
        ("LICENSE-APACHE", read_regular(request.repository / "LICENSE-APACHE"), 0o644),
        ("LICENSE-MIT", read_regular(request.repository / "LICENSE-MIT"), 0o644),
        ("README.md", read_regular(request.repository / "README.md"), 0o644),
        (
            "VERSION.json",
            (json.dumps(version, sort_keys=True, indent=2) + "\n").encode("utf-8"),
            0o644,
        ),
    ]
    files.extend(
        (f"completions/{name}", read_regular(request.completions / name), 0o644)
        for name in COMPLETIONS
    )
    files.append((binary_name, read_regular(request.binary), 0o755))

    temporary = archive.with_name(f".{archive.name}.{os.getpid()}.tmp")
    try:
        if archive_format == "zip":
            write_zip(temporary, root, files, request.source_date_epoch)
        else:
            write_tar_gz(temporary, root, files, request.source_date_epoch)
        if archive.exists() or archive.is_symlink():
            raise FileExistsError(f"release archive appeared while packaging: {archive}")
        os.replace(temporary, archive)
    finally:
        temporary.unlink(missing_ok=True)
    return archive


def target_layout(target: str) -> tuple[str, str]:
    try:
        return SUPPORTED_TARGETS[target]
    except KeyError as error:
        raise ValueError(f"unsupported release target: {target}") from error


def validate_request(request: PackageRequest) -> None:
    if not VERSION.fullmatch(request.version):
        raise ValueError(f"invalid semantic version: {request.version}")
    if not REVISION.fullmatch(request.source_revision):
        raise ValueError("source revision must be a lowercase 40-character Git digest")
    if request.source_date_epoch < 0:
        raise ValueError("source date epoch cannot be negative")
    read_regular(request.binary)
    for name in COMPLETIONS:
        if not read_regular(request.completions / name):
            raise ValueError(f"completion file is empty: {name}")


def read_regular(path: Path) -> bytes:
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or path.is_symlink():
        raise ValueError(f"release input must be a regular non-symlink file: {path}")
    if metadata.st_size > MAX_INPUT_BYTES:
        raise ValueError(f"release input exceeds {MAX_INPUT_BYTES} bytes: {path}")
    return path.read_bytes()


def write_tar_gz(
    output: Path,
    root: str,
    files: list[tuple[str, bytes, int]],
    epoch: int,
) -> None:
    with (
        output.open("xb") as raw,
        gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch, compresslevel=9) as zipped,
        tarfile.open(fileobj=zipped, mode="w", format=tarfile.PAX_FORMAT) as archive,
    ):
        for relative, contents, mode in files:
            member = tarfile.TarInfo(f"{root}/{relative}")
            member.size = len(contents)
            member.mode = mode
            member.mtime = epoch
            member.uid = 0
            member.gid = 0
            member.uname = ""
            member.gname = ""
            archive.addfile(member, io_bytes(contents))


def write_zip(
    output: Path,
    root: str,
    files: list[tuple[str, bytes, int]],
    epoch: int,
) -> None:
    date_time = zip_timestamp(epoch)
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for relative, contents, mode in files:
            member = zipfile.ZipInfo(f"{root}/{relative}", date_time=date_time)
            member.create_system = 3
            member.compress_type = zipfile.ZIP_DEFLATED
            member.external_attr = (stat.S_IFREG | mode) << 16
            archive.writestr(member, contents, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)


def zip_timestamp(epoch: int) -> tuple[int, int, int, int, int, int]:
    import datetime

    earliest = datetime.datetime(1980, 1, 1, tzinfo=datetime.timezone.utc)
    instant = datetime.datetime.fromtimestamp(epoch, tz=datetime.timezone.utc)
    instant = max(instant, earliest)
    return (
        instant.year,
        instant.month,
        instant.day,
        instant.hour,
        instant.minute,
        instant.second,
    )


def io_bytes(contents: bytes):
    import io

    return io.BytesIO(contents)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--completions", type=Path, required=True)
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", choices=sorted(SUPPORTED_TARGETS), required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--source-date-epoch", type=int, required=True)
    arguments = parser.parse_args()
    archive = package_binary(PackageRequest(**vars(arguments)))
    print(archive)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
