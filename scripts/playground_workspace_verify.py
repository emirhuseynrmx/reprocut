#!/usr/bin/env python3
"""Compile the complete ReproCut Rust workspace as one Playground crate."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
INCLUDE_STR = re.compile(r'include_str!\("([^"]+)"\)')


def read(path: str) -> str:
    source_path = ROOT / path
    source = source_path.read_text(encoding="utf-8")

    def replace(match: re.Match[str]) -> str:
        asset = (source_path.parent / match.group(1)).resolve().read_text(encoding="utf-8")
        fence = "####"
        while f'"{fence}' in asset:
            fence += "#"
        return f'r{fence}"{asset}"{fence}'

    return INCLUDE_STR.sub(replace, source)


def without_inner_attributes(source: str) -> str:
    return "\n".join(line for line in source.splitlines() if not line.startswith("#!["))


def wrap(name: str, source: str) -> str:
    return f"mod {name} {{\n{without_inner_attributes(source)}\n}}\n"


def compose_cli() -> str:
    model = read("crates/reprocut-core/src/model.rs")
    oracle = read("crates/reprocut-core/src/oracle.rs").replace(
        "use crate::{CandidateVerdict, ExecutionObservation, FailureFingerprint};",
        "use super::{CandidateVerdict, ExecutionObservation, FailureFingerprint};",
    )
    reducer = read("crates/reprocut-core/src/reducer.rs").replace(
        "use crate::CandidateVerdict;", "use super::CandidateVerdict;"
    )
    core = f"""mod reprocut_core {{
mod model {{ {model} }}
mod oracle {{ {oracle} }}
mod reducer {{ {reducer} }}
pub use model::{{CandidateVerdict, ExecutionObservation, FailureFingerprint}};
pub use oracle::{{FailureOracle, OracleError}};
pub use reducer::{{reduce, ReductionResult, ReductionUnit}};
}}
"""
    runner = read("crates/reprocut-runner/src/lib.rs").replace(
        "use reprocut_core::", "use crate::reprocut_core::"
    )
    workspace = read("crates/reprocut-workspace/src/lib.rs").replace(
        "use reprocut_core::", "use crate::reprocut_core::"
    )
    engine = (
        read("crates/reprocut-engine/src/lib.rs")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
        .replace("use reprocut_runner::", "use crate::reprocut_runner::")
        .replace("use reprocut_workspace::", "use crate::reprocut_workspace::")
        .replace(
            "Result<reprocut_core::ExecutionObservation, EngineError>",
            "Result<crate::reprocut_core::ExecutionObservation, EngineError>",
        )
    )
    report = read("crates/reprocut-report/src/lib.rs")
    cli = (
        without_inner_attributes(read("crates/reprocut-cli/src/main.rs"))
        .replace("use reprocut_engine::", "use crate::reprocut_engine::")
        .replace("use reprocut_report::", "use crate::reprocut_report::")
        .replace("use reprocut_workspace::", "use crate::reprocut_workspace::")
    )
    return "\n".join(
        [
            "#![forbid(unsafe_code)]",
            core,
            wrap("reprocut_runner", runner),
            wrap("reprocut_workspace", workspace),
            wrap("reprocut_engine", engine),
            wrap("reprocut_report", report),
            cli,
        ]
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--append", type=Path, required=True)
    args = parser.parse_args()

    code = compose_cli() + "\n" + args.append.resolve().read_text(encoding="utf-8")
    payload = json.dumps(
        {
            "channel": "stable",
            "mode": "debug",
            "edition": "2021",
            "crateType": "bin",
            "tests": True,
            "code": code,
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        "https://play.rust-lang.org/execute",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=180) as response:
        result = json.load(response)

    sys.stderr.write(result.get("stderr", ""))
    sys.stdout.write(result.get("stdout", ""))
    return 0 if result.get("success") else 1


if __name__ == "__main__":
    raise SystemExit(main())
