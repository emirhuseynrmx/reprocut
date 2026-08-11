#!/usr/bin/env python3
"""Compile one self-contained Rust source file with the official Playground API."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path

INCLUDE_STR = re.compile(r'include_str!\("([^"]+)"\)')


def inline_includes(source_path: Path, source: str) -> str:
    def replace(match: re.Match[str]) -> str:
        asset = (source_path.parent / match.group(1)).resolve().read_text(encoding="utf-8")
        fence = "####"
        while f'"{fence}' in asset:
            fence += "#"
        return f'r{fence}"{asset}"{fence}'

    return INCLUDE_STR.sub(replace, source)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("--append", type=Path)
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    source_path = args.source.resolve()
    code = inline_includes(source_path, source_path.read_text(encoding="utf-8"))
    if args.append is not None:
        append_path = args.append.resolve()
        code += "\n" + inline_includes(append_path, append_path.read_text(encoding="utf-8"))

    body = json.dumps(
        {
            "channel": "stable",
            "mode": "debug",
            "edition": "2021",
            "crateType": "bin",
            "tests": not args.run,
            "code": code,
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        "https://play.rust-lang.org/execute",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        result = json.load(response)

    stderr = result.get("stderr", "")
    stdout = result.get("stdout", "")
    sys.stderr.write(stderr)
    if args.output is None:
        sys.stdout.write(stdout)
    elif result.get("success"):
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(stdout, encoding="utf-8", newline="\n")
        print(f"wrote {args.output} ({len(stdout.encode('utf-8'))} bytes)")
    return 0 if result.get("success") else 1


if __name__ == "__main__":
    raise SystemExit(main())
