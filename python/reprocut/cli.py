"""Small Python console wrapper around the shared ReproCut Rust engine."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Optional, Sequence

from .client import (
    BaselineStableEvent,
    CompletedEvent,
    FailedEvent,
    ProgressEvent,
    ReductionRequest,
    ReproCutError,
    StartedEvent,
    reduce,
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="reprocut-py",
        description="Typed Python wrapper for the ReproCut Rust reduction engine",
    )
    subcommands = parser.add_subparsers(dest="action", required=True)
    for action in ("minimize", "resume"):
        command = subcommands.add_parser(action)
        command.add_argument("--root", type=Path, default=Path("."))
        command.add_argument("--output", type=Path, required=True)
        command.add_argument(
            "--ecosystem",
            choices=("auto", "cargo", "python", "npm", "none"),
            default="auto",
        )
        command.add_argument(
            "--preparation",
            choices=("none", "offline", "lifecycle_scripts", "isolated_python"),
            default="offline",
        )
        command.add_argument(
            "--oracle-stream",
            choices=("auto", "stderr", "stdout", "combined"),
            default="auto",
        )
        command.add_argument("--timeout-ms", type=int, default=5_000)
        command.add_argument("--max-output-bytes", type=int, default=1_048_576)
        command.add_argument("--flaky-runs", type=int)
        command.add_argument("--flaky-required", type=int)
        command.add_argument("--jobs", type=int, default=0)
        command.add_argument("--state", type=Path)
        command.add_argument("--binary", help="explicit Rust reprocut executable")
        command.add_argument("--client-timeout-seconds", type=float)
        command.add_argument("--json", action="store_true")
        command.add_argument("command", nargs=argparse.REMAINDER)
        if action == "minimize":
            command.add_argument("--restart", action="store_true")
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    failure_command = tuple(args.command[1:] if args.command[:1] == ["--"] else args.command)
    try:
        request = ReductionRequest(
            root=args.root,
            output=args.output,
            command=failure_command,
            action=args.action,
            ecosystem=args.ecosystem,
            preparation=args.preparation,
            timeout_ms=args.timeout_ms,
            max_output_bytes=args.max_output_bytes,
            oracle_stream=args.oracle_stream,
            flaky_runs=args.flaky_runs,
            flaky_required=args.flaky_required,
            jobs=args.jobs,
            state=args.state,
            restart=getattr(args, "restart", False),
        )
        result = reduce(
            request,
            progress=_print_progress,
            executable=args.binary,
            client_timeout_seconds=args.client_timeout_seconds,
        )
    except (ReproCutError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    if args.json:
        print(
            json.dumps(
                {
                    "output": os.fspath(result.output),
                    "evidence": os.fspath(result.evidence_path),
                    "report": os.fspath(result.report_path),
                    "issue": os.fspath(result.issue_path),
                    "fingerprint_sha256": result.fingerprint_sha256,
                },
                separators=(",", ":"),
            )
        )
    else:
        print(f"Verified reduction: {result.output}")
        print(f"Report: {result.report_path}")
    return 0


def _print_progress(event: ProgressEvent) -> None:
    if isinstance(event, StartedEvent):
        message = f"reprocut: {event.action} started for {event.root}"
    elif isinstance(event, BaselineStableEvent):
        message = f"reprocut: stable failure {event.fingerprint_sha256[:12]}"
    elif isinstance(event, CompletedEvent):
        message = f"reprocut: final verification passed in {event.output}"
    elif isinstance(event, FailedEvent):
        message = f"reprocut: failed: {event.message}"
    else:
        return
    print(message, file=sys.stderr)
