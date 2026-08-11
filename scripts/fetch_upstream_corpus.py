#!/usr/bin/env python3
"""Fetch pinned compiler-bug subjects without executing upstream code."""

from __future__ import annotations

import argparse
import json
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MANIFEST = ROOT / "benchmarks" / "upstream-corpus.json"
MAX_MEMBER_BYTES = 8 * 1024 * 1024
MAX_TOTAL_BYTES = 128 * 1024 * 1024


class CorpusError(RuntimeError):
    """A manifest, archive, or safe-publication contract failed."""


def load_manifest(path: Path) -> dict[str, object]:
    """Load and validate the immutable metadata contract."""
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("schema_version") != 1:
        raise CorpusError("unsupported corpus schema")
    commit = document.get("commit")
    if not isinstance(commit, str) or len(commit) != 40 or any(
        character not in "0123456789abcdef" for character in commit
    ):
        raise CorpusError("corpus commit must be a lowercase 40-character Git object ID")
    cases = document.get("cases")
    if not isinstance(cases, list) or len(cases) != document.get("case_count"):
        raise CorpusError("case_count does not match cases")
    identifiers = [case.get("id") for case in cases if isinstance(case, dict)]
    if len(identifiers) != len(cases) or len(set(identifiers)) != len(identifiers):
        raise CorpusError("case identifiers must be present and unique")
    if document.get("redistribution") != "download-only":
        raise CorpusError("upstream GPL corpus must remain download-only")
    return document


def selected_members(manifest: dict[str, object]) -> dict[PurePosixPath, tuple[str, str]]:
    """Return exact archive suffixes mapped to case and destination name."""
    layouts = manifest["layouts"]
    cases = manifest["cases"]
    if not isinstance(layouts, dict) or not isinstance(cases, list):
        raise CorpusError("invalid layout or case collection")
    selected: dict[PurePosixPath, tuple[str, str]] = {}
    for case in cases:
        if not isinstance(case, dict):
            raise CorpusError("case must be an object")
        identifier = case.get("id")
        upstream_path = case.get("upstream_path")
        layout = case.get("layout")
        if not all(isinstance(value, str) for value in (identifier, upstream_path, layout)):
            raise CorpusError("case id, path, and layout must be strings")
        files = layouts.get(layout)
        if not isinstance(files, list) or not files:
            raise CorpusError(f"unknown or empty layout: {layout}")
        base = PurePosixPath(upstream_path)
        if base.is_absolute() or ".." in base.parts:
            raise CorpusError(f"unsafe upstream path: {upstream_path}")
        for name in files:
            if not isinstance(name, str) or PurePosixPath(name).name != name:
                raise CorpusError(f"unsafe corpus filename: {name}")
            selected[base / name] = (identifier, name)
    return selected


def download_file(url: str, destination: Path) -> int:
    """Stream one pinned regular file with a strict byte limit."""
    request = urllib.request.Request(url, headers={"User-Agent": "reprocut-corpus/0.1"})
    total = 0
    destination.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(request, timeout=120) as response:
        content_length = response.headers.get("Content-Length")
        if content_length is not None and int(content_length) > MAX_MEMBER_BYTES:
            raise CorpusError(f"upstream file exceeds the 8 MiB safety limit: {destination.name}")
        with destination.open("xb") as output:
            while chunk := response.read(64 * 1024):
                total += len(chunk)
                if total > MAX_MEMBER_BYTES:
                    raise CorpusError(
                        f"upstream file exceeds the 8 MiB safety limit: {destination.name}"
                    )
                output.write(chunk)
    return total


def fetch_selected_files(manifest: dict[str, object], staging: Path) -> int:
    """Download only exact allowlisted files from the pinned commit."""
    template = manifest.get("raw_url_template")
    commit = manifest.get("commit")
    if (
        not isinstance(template, str)
        or template
        != "https://raw.githubusercontent.com/uw-pluverse/perses/{commit}/{path}"
        or not isinstance(commit, str)
    ):
        raise CorpusError("raw_url_template must be the reviewed Perses HTTPS endpoint")
    total = 0
    for path, (identifier, name) in sorted(selected_members(manifest).items()):
        encoded_path = urllib.parse.quote(str(path), safe="/")
        url = template.format(commit=commit, path=encoded_path)
        try:
            total += download_file(url, staging / identifier / name)
        except (OSError, urllib.error.URLError) as error:
            raise CorpusError(f"download failed for pinned member {path}: {error}") from error
        if total > MAX_TOTAL_BYTES:
            raise CorpusError("selected corpus exceeds the 128 MiB safety limit")
    return total


def write_provenance(staging: Path, manifest: dict[str, object]) -> None:
    """Write non-executable provenance beside each downloaded subject."""
    cases = manifest["cases"]
    assert isinstance(cases, list)
    for case in cases:
        assert isinstance(case, dict)
        identifier = case["id"]
        provenance = {
            "schema_version": 1,
            "id": identifier,
            "issue_url": case["issue_url"],
            "upstream_repository": manifest["repository"],
            "upstream_commit": manifest["commit"],
            "upstream_path": case["upstream_path"],
            "upstream_license": manifest["upstream_license"],
            "execution_policy": "never executed by fetch_upstream_corpus.py",
        }
        target = staging / str(identifier) / "SOURCE.json"
        target.write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")


def publish(staging: Path, destination: Path) -> None:
    """Atomically publish to a destination that did not exist."""
    if destination.exists():
        raise CorpusError(f"refusing to overwrite destination: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging.replace(destination)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument(
        "--accept-gpl-3.0",
        dest="accept_gpl_3_0",
        action="store_true",
        help="acknowledge that downloaded subjects remain GPL-3.0-only",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.accept_gpl_3_0:
        raise CorpusError("pass --accept-gpl-3.0 before downloading the upstream corpus")
    manifest = load_manifest(args.manifest.resolve())
    destination = args.destination.resolve()
    if destination.exists():
        raise CorpusError(f"refusing to overwrite destination: {destination}")
    with tempfile.TemporaryDirectory(prefix="reprocut-corpus-") as temporary:
        temporary_root = Path(temporary)
        staging = temporary_root / "staging"
        staging.mkdir()
        fetch_selected_files(manifest, staging)
        write_provenance(staging, manifest)
        publish(staging, destination)
    print(f"Fetched {manifest['case_count']} pinned cases into {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
