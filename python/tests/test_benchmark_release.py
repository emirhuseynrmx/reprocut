from __future__ import annotations

import importlib.util
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "benchmark_release.py"
SPEC = importlib.util.spec_from_file_location("benchmark_release", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


def test_release_fixture_has_exactly_312_files_and_a_real_failure(tmp_path: Path) -> None:
    root = tmp_path / "fixture"
    BENCHMARK.build_fixture(root)

    files = [path for path in root.rglob("*") if path.is_file()]
    completed = subprocess.run(
        [sys.executable, "src/bug.py"],
        cwd=root,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=5,
    )

    assert len(files) == 312
    assert completed.returncode != 0
    assert "unsupported operand type" in completed.stderr
    assert "Decimal" in completed.stderr


def test_summary_rejects_non_deterministic_reductions() -> None:
    first = _run("a" * 64)
    second = _run("b" * 64)

    with pytest.raises(BENCHMARK.BenchmarkError, match="fingerprint"):
        BENCHMARK.summarize([first, second])


def test_markdown_contains_every_release_metric() -> None:
    run = _run("a" * 64)
    document = {
        "measured_runs": 1,
        "poll_interval_ms": 10.0,
        "summary": BENCHMARK.summarize([run]),
    }

    markdown = BENCHMARK.render_markdown(document)

    for label in [
        "Files",
        "Bytes",
        "Lines",
        "End-to-end wall time",
        "Oracle executions",
        "Candidate attempts",
        "Sampled process-tree peak RSS",
    ]:
        assert label in markdown


def _run(fingerprint: str) -> dict[str, object]:
    return {
        "wall_ms": 100.0,
        "engine_elapsed_ms": 95,
        "sampled_peak_rss_bytes": 32 * 1024 * 1024,
        "oracle_runs": 42,
        "candidate_attempts": 12,
        "cache_hits": 2,
        "original": {"files": 312, "bytes": 1000, "lines": 500, "syntax_nodes": None},
        "retained": {"files": 3, "bytes": 200, "lines": 20, "syntax_nodes": None},
        "fingerprint_sha256": fingerprint,
        "accepted_structured_edits": ["syntax:src/bug.py:delete:0:1"],
    }
