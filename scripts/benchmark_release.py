#!/usr/bin/env python3
"""Produce a repeatable ReproCut release benchmark evidence bundle."""

from __future__ import annotations

import argparse
import contextlib
import json
import os
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

try:
    from release.schema_versions import EVIDENCE_SCHEMA
except ModuleNotFoundError:  # loaded directly by focused unit tests
    from scripts.release.schema_versions import EVIDENCE_SCHEMA

FIXTURE_FILES = 312
NOISE_FILES = FIXTURE_FILES - 3
DEFAULT_RUN_TIMEOUT_SECONDS = 600


class BenchmarkError(RuntimeError):
    """The benchmark contract or measured ReproCut run failed."""


def build_fixture(root: Path) -> None:
    """Create one deterministic 312-file Python failure project."""
    root.mkdir(parents=True, exist_ok=False)
    (root / "src").mkdir()
    (root / "fixtures").mkdir()
    (root / "noise").mkdir()
    (root / "src" / "bug.py").write_text(
        """from checkout import order_total


def unused_formatter(value):
    return f"unused:{value}"


def reproduce_failure():
    return order_total() + " USD"


reproduce_failure()
""",
        encoding="utf-8",
    )
    (root / "src" / "checkout.py").write_text(
        """import json
from decimal import Decimal
from pathlib import Path


def order_total():
    order = json.loads(Path("fixtures/order.json").read_text(encoding="utf-8"))
    return Decimal(order["total"])
""",
        encoding="utf-8",
    )
    (root / "fixtures" / "order.json").write_text('{"total":"42.50"}\n', encoding="utf-8")
    for index in range(NOISE_FILES):
        package = root / "noise" / f"package_{index % 13:02d}"
        package.mkdir(exist_ok=True)
        (package / f"module_{index:03d}.py").write_text(
            f"def unused_{index:03d}(value):\n    return value + {index}\n",
            encoding="utf-8",
        )
    actual = sum(1 for path in root.rglob("*") if path.is_file())
    if actual != FIXTURE_FILES:
        raise BenchmarkError(f"fixture contract expected {FIXTURE_FILES} files, created {actual}")


def process_tree_rss(root_process: Any, psutil_module: Any) -> int:
    """Return one best-effort RSS sample for the root and current descendants."""
    total = 0
    try:
        processes = [root_process, *root_process.children(recursive=True)]
    except (psutil_module.NoSuchProcess, psutil_module.AccessDenied):
        processes = [root_process]
    for process in processes:
        try:
            total += int(process.memory_info().rss)
        except (  # noqa: PERF203 - descendants disappear independently while sampled
            psutil_module.NoSuchProcess,
            psutil_module.AccessDenied,
        ):
            continue
    return total


def terminate_process_tree(process: subprocess.Popen[str], psutil_module: Any) -> None:
    """Terminate descendants before the root after a benchmark deadline."""
    try:
        root = psutil_module.Process(process.pid)
        descendants = root.children(recursive=True)
    except (psutil_module.NoSuchProcess, psutil_module.AccessDenied):
        descendants = []
    for child in descendants:
        with contextlib.suppress(psutil_module.NoSuchProcess, psutil_module.AccessDenied):
            child.terminate()
    if descendants:
        _, alive = psutil_module.wait_procs(descendants, timeout=2)
        for child in alive:
            with contextlib.suppress(psutil_module.NoSuchProcess, psutil_module.AccessDenied):
                child.kill()
    if process.poll() is None:
        process.kill()


def oracle_runs(evidence: dict[str, Any]) -> int:
    """Count actual observed oracle executions, excluding cache hits."""
    search = evidence["search"]
    attempts = evidence["attempts"]
    return (
        int(search["baseline_runs"])
        + int(search["final_verifications"])
        + sum(int(attempt["observed_runs"]) for attempt in attempts)
    )


def run_once(
    reprocut: Path,
    python: Path,
    jobs: int,
    poll_seconds: float,
    timeout_seconds: int,
) -> dict[str, Any]:
    """Measure one fresh end-to-end reduction including process-tree RSS."""
    try:
        import psutil
    except ModuleNotFoundError as error:
        raise BenchmarkError(
            "install the pinned benchmark extra: pip install .[benchmark]"
        ) from error

    with tempfile.TemporaryDirectory(prefix="reprocut-benchmark-") as temporary:
        temporary_root = Path(temporary)
        source = temporary_root / "source"
        output = temporary_root / "minimal"
        state = temporary_root / "state.sqlite3"
        request_path = temporary_root / "request.json"
        build_fixture(source)
        request = {
            "protocol_version": 1,
            "action": "minimize",
            "root": str(source),
            "output": str(output),
            "ecosystem": "python",
            "preparation": "offline",
            "command": [str(python), "src/bug.py"],
            "timeout_ms": 5000,
            "max_output_bytes": 1048576,
            "oracle_stream": "stderr",
            "jobs": jobs,
            "state": str(state),
        }
        request_path.write_text(json.dumps(request, indent=2) + "\n", encoding="utf-8")
        command = [str(reprocut), "protocol", "run", "--request", str(request_path)]
        started = time.perf_counter_ns()
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        observed = psutil.Process(process.pid)
        sampled_peak_rss = 0
        deadline = time.monotonic() + timeout_seconds
        while process.poll() is None:
            sampled_peak_rss = max(sampled_peak_rss, process_tree_rss(observed, psutil))
            if time.monotonic() >= deadline:
                terminate_process_tree(process, psutil)
                process.communicate(timeout=5)
                raise BenchmarkError(f"benchmark run exceeded {timeout_seconds} seconds")
            time.sleep(poll_seconds)
        sampled_peak_rss = max(sampled_peak_rss, process_tree_rss(observed, psutil))
        stdout, stderr = process.communicate()
        wall_ms = (time.perf_counter_ns() - started) / 1_000_000
        events = [json.loads(line) for line in stdout.splitlines() if line.strip()]
        if process.returncode != 0:
            terminal = events[-1].get("message") if events else stderr.strip()
            raise BenchmarkError(f"ReproCut benchmark run failed: {terminal}")
        if not events or events[-1].get("type") != "completed":
            raise BenchmarkError("protocol did not emit a completed terminal event")
        evidence = json.loads((output / "reduction.json").read_text(encoding="utf-8"))
        if (
            evidence.get("schema_version") != EVIDENCE_SCHEMA
            or evidence["failure"].get("same_failure") is not True
        ):
            raise BenchmarkError("run did not publish schema-v3 same-failure evidence")
        measurements = evidence["measurements"]
        search = evidence["search"]
        return {
            "wall_ms": round(wall_ms, 3),
            "engine_elapsed_ms": int(measurements["elapsed_ms"]),
            "sampled_peak_rss_bytes": sampled_peak_rss,
            "oracle_runs": oracle_runs(evidence),
            "candidate_attempts": int(search["attempts"]),
            "cache_hits": int(search["cache_hits"]),
            "original": measurements["original"],
            "retained": measurements["retained"],
            "fingerprint_sha256": evidence["failure"]["fingerprint_sha256"],
            "accepted_structured_edits": evidence["accepted_structured_edits"],
        }


def summarize(runs: list[dict[str, Any]]) -> dict[str, Any]:
    """Require deterministic outputs and compute honest distribution summaries."""
    if not runs:
        raise BenchmarkError("at least one measured run is required")
    first = runs[0]
    stable_fields = (
        "original",
        "retained",
        "fingerprint_sha256",
        "accepted_structured_edits",
    )
    for run in runs[1:]:
        for field in stable_fields:
            if run[field] != first[field]:
                raise BenchmarkError(f"non-deterministic benchmark field across runs: {field}")

    def distribution(field: str) -> dict[str, float | int]:
        values = [run[field] for run in runs]
        return {
            "median": statistics.median(values),
            "min": min(values),
            "max": max(values),
        }

    return {
        "original": first["original"],
        "retained": first["retained"],
        "fingerprint_sha256": first["fingerprint_sha256"],
        "wall_ms": distribution("wall_ms"),
        "engine_elapsed_ms": distribution("engine_elapsed_ms"),
        "sampled_peak_rss_bytes": distribution("sampled_peak_rss_bytes"),
        "oracle_runs": distribution("oracle_runs"),
        "candidate_attempts": distribution("candidate_attempts"),
        "cache_hits": distribution("cache_hits"),
    }


def render_markdown(document: dict[str, Any]) -> str:
    """Render the evidence summary without turning samples into broad claims."""
    summary = document["summary"]
    original = summary["original"]
    retained = summary["retained"]
    wall = summary["wall_ms"]
    memory = summary["sampled_peak_rss_bytes"]
    oracle = summary["oracle_runs"]
    attempts = summary["candidate_attempts"]
    mib = 1024 * 1024
    wall_range = f"{wall['min']:.3f}-{wall['max']:.3f}"
    oracle_range = f"{oracle['min']}-{oracle['max']}"
    attempt_range = f"{attempts['min']}-{attempts['max']}"
    memory_range = f"{memory['min'] / mib:.2f}-{memory['max'] / mib:.2f}"
    return f"""# ReproCut 0.1 release benchmark

This is a {document["measured_runs"]}-run measurement of the checked-in 312-file fixture on one
recorded machine. It is evidence for this environment, not a universal speed claim.

| Metric | Before | After / measured |
|---|---:|---:|
| Files | {original["files"]} | {retained["files"]} |
| Bytes | {original["bytes"]} | {retained["bytes"]} |
| Lines | {original["lines"]} | {retained["lines"]} |
| End-to-end wall time | — | {wall["median"]:.3f} ms median ({wall_range}) |
| Oracle executions | — | {oracle["median"]} median ({oracle_range}) |
| Candidate attempts | — | {attempts["median"]} median ({attempt_range}) |
| Sampled process-tree peak RSS | — | {memory["median"] / mib:.2f} MiB median ({memory_range}) |

Failure fingerprint: `{summary["fingerprint_sha256"]}`

Peak RSS is sampled every {document["poll_interval_ms"]:.3f} ms across the ReproCut process and
visible descendants; a short-lived peak between samples can be missed. Raw runs and complete
environment metadata are preserved in `benchmark.json`.
"""


def executable_version(executable: Path) -> str:
    """Read a bounded one-line version without a shell."""
    completed = subprocess.run(
        [str(executable), "--version"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=10,
    )
    return (completed.stdout or completed.stderr).strip().splitlines()[0]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reprocut", type=Path, required=True)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--jobs", type=int, default=max(1, min(4, os.cpu_count() or 1)))
    parser.add_argument("--poll-ms", type=float, default=10.0)
    parser.add_argument("--run-timeout-seconds", type=int, default=DEFAULT_RUN_TIMEOUT_SECONDS)
    return parser.parse_args()


def validate_args(args: argparse.Namespace) -> None:
    if args.runs < 1 or args.warmup < 0:
        raise BenchmarkError("runs must be positive and warmup cannot be negative")
    if args.jobs < 1 or not 1.0 <= args.poll_ms <= 1000.0:
        raise BenchmarkError("jobs must be positive and poll-ms must be within 1..1000")
    if args.run_timeout_seconds < 1:
        raise BenchmarkError("run-timeout-seconds must be positive")
    if not args.reprocut.is_file() or not args.python.is_file():
        raise BenchmarkError("reprocut and python must be existing regular files")
    if args.output.exists():
        raise BenchmarkError(f"refusing to overwrite benchmark output: {args.output}")


def main() -> int:
    args = parse_args()
    validate_args(args)
    poll_seconds = args.poll_ms / 1000
    for _ in range(args.warmup):
        run_once(
            args.reprocut.resolve(),
            args.python.resolve(),
            args.jobs,
            poll_seconds,
            args.run_timeout_seconds,
        )
    runs = [
        run_once(
            args.reprocut.resolve(),
            args.python.resolve(),
            args.jobs,
            poll_seconds,
            args.run_timeout_seconds,
        )
        for _ in range(args.runs)
    ]
    document = {
        "schema_version": 1,
        "fixture": "reprocut-312-file-python-failure-v1",
        "measured_runs": args.runs,
        "warmup_runs": args.warmup,
        "jobs": args.jobs,
        "poll_interval_ms": args.poll_ms,
        "environment": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "processor": platform.processor(),
            "logical_cpus": os.cpu_count(),
            "python": platform.python_version(),
            "reprocut": executable_version(args.reprocut.resolve()),
        },
        "summary": summarize(runs),
        "runs": runs,
    }
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="reprocut-benchmark-publish-", dir=output.parent
    ) as temp:
        staging = Path(temp) / "artifact"
        staging.mkdir()
        (staging / "benchmark.json").write_text(
            json.dumps(document, indent=2) + "\n", encoding="utf-8"
        )
        (staging / "benchmark.md").write_text(render_markdown(document), encoding="utf-8")
        staging.replace(output)
    print(f"Published benchmark evidence to {output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BenchmarkError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2) from None
