"""DustMite against ReproCut on a real multi-file repository failure.

This is the lane both tools actually share. cvise, C-Reduce and Perses take a single file
and cannot be entered here at all, which is the point: the comparison that matters for a
repository reducer is against the other repository reducer.

The case is the upstream openruyi failure at its pinned commit, not a fixture built for the
occasion - 95 files, one of which is missing its final newline.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ORACLE = ROOT / "scripts" / "benchmark" / "repo_oracle.sh"
BUDGET = int(os.environ.get("BENCHMARK_BUDGET_SECONDS", "900"))


def size(tree: Path) -> dict:
    files = [p for p in tree.rglob("*") if p.is_file() and ".git" not in p.parts]
    lines = 0
    for path in files:
        lines += len(path.read_text(errors="replace").splitlines())
    return {"files": len(files), "lines": lines,
            "bytes": sum(p.stat().st_size for p in files)}


def measure(tool: str, tree: Path, counter: Path, seconds: float, code: int) -> dict:
    record = {"tool": tool, "seconds": round(seconds, 1), "exit_code": code,
              "oracle_calls": counter.stat().st_size}
    record.update(size(tree))
    return record


def dustmite(work: Path, case: Path) -> dict:
    if shutil.which("dustmite") is None:
        return {"tool": "dustmite", "available": False}
    home = work / "dustmite"
    shutil.rmtree(home, ignore_errors=True)
    home.mkdir()
    shutil.copytree(case, home / "tree")
    counter = home / "counter"
    counter.write_bytes(b"")
    environment = dict(os.environ, BENCHMARK_COUNTER=str(counter),
                       BENCHMARK_POLARITY="interesting")
    start = time.monotonic()
    with (home / "tool.log").open("wb") as log:
        completed = subprocess.run(
            ["dustmite", "-j1", "--no-redirect", str(home / "tree"), str(ORACLE)],
            cwd=home, stdout=log, stderr=subprocess.STDOUT, env=environment,
            timeout=BUDGET + 120, check=False)
    elapsed = time.monotonic() - start
    reduced = home / "tree.reduced"
    if not reduced.exists():
        return {"tool": "dustmite", "available": True, "produced_output": False,
                "exit_code": completed.returncode, "seconds": round(elapsed, 1),
                "oracle_calls": counter.stat().st_size}
    record = measure("dustmite", reduced, counter, elapsed, completed.returncode)
    record["available"] = record["produced_output"] = True
    return record


def reprocut(work: Path, case: Path) -> dict:
    binary = ROOT / "target" / "release" / "reprocut"
    if not binary.exists():
        return {"tool": "reprocut", "available": False}
    home = work / "reprocut"
    shutil.rmtree(home, ignore_errors=True)
    home.mkdir()
    shutil.copytree(case, home / "tree")
    counter = home / "counter"
    counter.write_bytes(b"")
    environment = dict(os.environ, BENCHMARK_COUNTER=str(counter),
                       BENCHMARK_POLARITY="failing")
    start = time.monotonic()
    with (home / "tool.log").open("wb") as log:
        completed = subprocess.run(
            [str(binary), "reduce", "--root", "tree", "--output", "output", "--jobs", "1",
             "--ecosystem", "none", "--prepare", "none",
             "--oracle-stream", "combined", "--oracle-mode", "regex",
             "--failure-regex", "REPO-ARENA: files were modified by this hook",
             "--max-duration-secs", str(BUDGET), "--", str(ORACLE)],
            cwd=home, stdout=log, stderr=subprocess.STDOUT, env=environment,
            timeout=BUDGET + 120, check=False)
    elapsed = time.monotonic() - start
    project = home / "output" / "project"
    if not project.exists():
        return {"tool": "reprocut", "available": True, "produced_output": False,
                "exit_code": completed.returncode, "seconds": round(elapsed, 1),
                "oracle_calls": counter.stat().st_size}
    record = measure("reprocut", project, counter, elapsed, completed.returncode)
    record["available"] = record["produced_output"] = True
    reduction = home / "output" / "reduction.json"
    if reduction.exists():
        record["diagnostic_drift"] = json.loads(
            reduction.read_text())["failure"].get("diagnostic_drift")
    return record


def main() -> int:
    work = Path(sys.argv[1]).resolve()
    case = work / "case"
    original = size(case)
    rows = [dustmite(work, case), reprocut(work, case)]
    (work / "repo-arena.json").write_text(
        json.dumps({"original": original, "results": rows}, indent=2) + chr(10))

    print(f"{'':<11}{'files':>8}{'lines':>9}{'bytes':>11}{'oracle':>9}{'seconds':>9}")
    print(f"{'original':<11}{original['files']:>8,}{original['lines']:>9,}"
          f"{original['bytes']:>11,}")
    for row in rows:
        if not row.get("available"):
            print(f"{row['tool']:<11}{'not installed':>28}")
        elif not row.get("produced_output"):
            print(f"{row['tool']:<11}{'no output':>28}{row['oracle_calls']:>9,}"
                  f"{row['seconds']:>9.1f}")
        else:
            print(f"{row['tool']:<11}{row['files']:>8,}{row['lines']:>9,}"
                  f"{row['bytes']:>11,}{row['oracle_calls']:>9,}{row['seconds']:>9.1f}")
    drift = next((r.get("diagnostic_drift") for r in rows if r["tool"] == "reprocut"), None)
    print()
    print("reprocut drift:", "not measured" if drift is None else
          f"{drift['novel_lines']} novel of {drift['final_lines']} "
          f"(reportable: {drift['reportable']})")
    print("dustmite drift: not measured - the tool does not ask")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
