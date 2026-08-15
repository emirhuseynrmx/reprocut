#!/usr/bin/env python3
"""Validate a ReproCut release archive without extracting it."""

from __future__ import annotations

import argparse
import json
import stat
import tarfile
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from package_binary import COMPLETIONS, MAX_INPUT_BYTES, REVISION, target_layout

MAX_MEMBERS = 32
MAX_TOTAL_BYTES = 512 * 1024 * 1024


class ArchiveError(ValueError):
    pass


@dataclass(frozen=True)
class ArchiveVerification:
    archive: Path
    target: str
    version: str
    binary: str
    source_revision: str
    members: tuple[str, ...]


@dataclass(frozen=True)
class Member:
    name: str
    contents: bytes
    mode: int


def verify_archive(
    archive: Path, *, expected_target: str, expected_version: str
) -> ArchiveVerification:
    archive = archive.resolve()
    if archive.is_symlink() or not archive.is_file():
        raise ArchiveError("archive must be a regular non-symlink file")
    if archive.stat().st_size > MAX_INPUT_BYTES:
        raise ArchiveError("archive exceeds the bounded input size")
    members = read_members(archive)
    if len(members) > MAX_MEMBERS:
        raise ArchiveError("archive has too many members")
    if sum(len(member.contents) for member in members) > MAX_TOTAL_BYTES:
        raise ArchiveError("archive expands beyond its size bound")

    root = f"reprocut-{expected_version}-{expected_target}"
    relative: dict[str, Member] = {}
    for member in members:
        safe = safe_member(member.name)
        if len(safe.parts) < 2 or safe.parts[0] != root:
            raise ArchiveError(f"archive member escaped the release root: {member.name}")
        key = PurePosixPath(*safe.parts[1:]).as_posix()
        if key in relative:
            raise ArchiveError(f"duplicate archive member: {key}")
        relative[key] = member

    _archive_format, binary = target_layout(expected_target)
    expected = {
        "LICENSE",
        "README.md",
        "VERSION.json",
        binary,
        *(f"completions/{name}" for name in COMPLETIONS),
    }
    if set(relative) != expected:
        missing = sorted(expected - set(relative))
        extra = sorted(set(relative) - expected)
        raise ArchiveError(f"release members differ; missing={missing}, extra={extra}")
    if relative[binary].mode & 0o111 == 0:
        raise ArchiveError("release binary is not executable")
    if any(not relative[f"completions/{name}"].contents for name in COMPLETIONS):
        raise ArchiveError("release contains an empty completion script")

    try:
        version = json.loads(relative["VERSION.json"].contents)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ArchiveError("VERSION.json is invalid") from error
    required = {
        "schema_version",
        "name",
        "version",
        "target",
        "binary",
        "source_revision",
        "source_date_epoch",
    }
    if set(version) != required:
        raise ArchiveError("VERSION.json fields differ from schema 1")
    if version["schema_version"] != 1 or version["name"] != "reprocut":
        raise ArchiveError("VERSION.json identity is invalid")
    if version["version"] != expected_version or version["target"] != expected_target:
        raise ArchiveError("VERSION.json version or target does not match archive")
    if version["binary"] != binary:
        raise ArchiveError("VERSION.json binary does not match target")
    if not isinstance(version["source_date_epoch"], int) or version["source_date_epoch"] < 0:
        raise ArchiveError("VERSION.json source date is invalid")
    revision = version["source_revision"]
    if not isinstance(revision, str) or REVISION.fullmatch(revision) is None:
        raise ArchiveError("VERSION.json source revision is invalid")
    return ArchiveVerification(
        archive=archive,
        target=expected_target,
        version=expected_version,
        binary=binary,
        source_revision=revision,
        members=tuple(sorted(relative)),
    )


def safe_member(name: str) -> PurePosixPath:
    if "\0" in name or "\\" in name or name.startswith("/"):
        raise ArchiveError(f"unsafe archive member: {name}")
    value = PurePosixPath(name)
    if (
        not value.parts
        or value.as_posix() != name
        or any(part in {"", ".", ".."} for part in value.parts)
    ):
        raise ArchiveError(f"unsafe archive member: {name}")
    return value


def read_members(archive: Path) -> list[Member]:
    if archive.name.endswith(".tar.gz"):
        with tarfile.open(archive, "r:gz") as source:
            result = []
            for member in source.getmembers():
                if not member.isfile() or member.issym() or member.islnk():
                    raise ArchiveError(f"non-regular archive member: {member.name}")
                if member.size > MAX_INPUT_BYTES:
                    raise ArchiveError(f"oversized archive member: {member.name}")
                stream = source.extractfile(member)
                if stream is None:
                    raise ArchiveError(f"unreadable archive member: {member.name}")
                result.append(Member(member.name, stream.read(MAX_INPUT_BYTES + 1), member.mode))
            return result
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as source:
            result = []
            for member in source.infolist():
                mode = member.external_attr >> 16
                if member.is_dir() or stat.S_IFMT(mode) not in {0, stat.S_IFREG}:
                    raise ArchiveError(f"non-regular archive member: {member.filename}")
                if member.file_size > MAX_INPUT_BYTES:
                    raise ArchiveError(f"oversized archive member: {member.filename}")
                result.append(
                    Member(
                        member.filename,
                        source.read(member, pwd=None),
                        mode,
                    )
                )
            return result
    raise ArchiveError("release archive must be .tar.gz or .zip")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    arguments = parser.parse_args()
    result = verify_archive(
        arguments.archive,
        expected_target=arguments.target,
        expected_version=arguments.version,
    )
    print(
        json.dumps(
            {
                "archive": str(result.archive),
                "target": result.target,
                "version": result.version,
                "source_revision": result.source_revision,
                "members": result.members,
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
