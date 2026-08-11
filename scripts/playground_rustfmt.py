#!/usr/bin/env python3
"""Check or mechanically apply official Rust Playground formatting."""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import re
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXTERNAL_MODULE = re.compile(
    r"(?m)^([ \t]*(?:pub[ \t]+)?mod[ \t]+[A-Za-z_][A-Za-z0-9_]*)[ \t]*;[ \t]*$"
)
MASKED_MODULE = re.compile(
    r"(?m)^([ \t]*(?:pub[ \t]+)?mod[ \t]+[A-Za-z_][A-Za-z0-9_]*)[ \t]*"
    r"\{[ \t]*/\*__REPROCUT_EXTERNAL_MODULE__\*/[ \t]*"
    r"(?:\r?\n[ \t]*)?\}[ \t]*$"
)


def rust_sources() -> list[Path]:
    roots = [ROOT / "crates", ROOT / "scripts" / "verification"]
    return sorted(path for root in roots for path in root.rglob("*.rs"))


def format_source(path: Path) -> tuple[Path, str]:
    source = path.read_text(encoding="utf-8")
    standalone = EXTERNAL_MODULE.sub(r"\1 { /*__REPROCUT_EXTERNAL_MODULE__*/ }", source)
    payload = json.dumps(
        {
            "channel": "stable",
            "edition": "2021",
            "code": standalone,
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        "https://play.rust-lang.org/format",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        result = json.load(response)
    if not result.get("success"):
        raise RuntimeError(f"rustfmt failed for {path}: {result.get('stderr', '')}")
    formatted = MASKED_MODULE.sub(r"\1;", str(result["code"]))
    return path, formatted


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    parser.add_argument("paths", nargs="*", type=Path)
    arguments = parser.parse_args()
    changed: list[tuple[Path, str]] = []
    sources = (
        [path.resolve() for path in arguments.paths]
        if arguments.paths
        else rust_sources()
    )

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
        for path, formatted in executor.map(format_source, sources):
            current = path.read_text(encoding="utf-8").replace("\r\n", "\n")
            if current != formatted:
                changed.append((path, formatted))

    if arguments.write:
        for path, formatted in changed:
            path.write_text(formatted, encoding="utf-8", newline="\n")
        print(f"formatted {len(changed)} Rust files")
        return 0
    if changed:
        for path, _formatted in changed:
            print(path.relative_to(ROOT).as_posix())
        return 1
    print(f"rustfmt-compatible: {len(sources)} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
