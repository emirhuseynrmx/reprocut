"""Every reducer that will install, on the same one-file C failure.

Each tool gets its own directory, its own copy of the case, and the same interestingness
script - which counts its own invocations, so a tool that silently never ran shows up as
zero calls instead of as a tool that could not reduce anything.

A tool that fails to install is recorded as unavailable rather than failing the run: the
comparison is worth publishing with four entrants, and pretending otherwise would hide
which ones were actually measured.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEST = ROOT / "scripts" / "benchmark" / "interesting.sh"
TOKEN = re.compile(r"[A-Za-z_]\w*|\d+\.?\d*|[^\s\w]")
BUDGET = int(os.environ.get("BENCHMARK_BUDGET_SECONDS", "900"))
PERSES = Path(os.environ.get("PERSES_JAR", "/opt/perses/perses_deploy.jar"))


def command(tool: str, work: Path) -> list[str] | None:
    """The invocation each tool wants for 'reduce case.c under this test'."""
    if tool == "cvise":
        return ["cvise", "--n", "1", str(TEST), "case.c"]
    if tool == "creduce":
        return ["creduce", "--n", "1", str(TEST), "case.c"]
    if tool == "shrinkray":
        return ["shrinkray", "--parallelism", "1", str(TEST), "case.c"]
    if tool == "perses":
        if not PERSES.exists():
            return None
        return ["java", "-jar", str(PERSES), "--test-script", str(TEST),
                "--input-file", "case.c", "--in-place", "true"]
    if tool == "reprocut":
        return [str(ROOT / "target" / "release" / "reprocut"), "reduce",
                "--root", "source", "--output", "output", "--jobs", "1",
                "--ecosystem", "none", "--prepare", "none",
                "--oracle-stream", "combined", "--oracle-mode", "regex",
                "--failure-regex", "^BENCHMARK: the injected diagnostic is present$",
                "--max-duration-secs", str(BUDGET), "--", str(TEST)]
    return None


def measure(path: Path) -> dict:
    text = path.read_text(errors="replace")
    return {"bytes": path.stat().st_size, "lines": len(text.splitlines()),
            "tokens": len(TOKEN.findall(text))}


def run(tool: str, case: Path, include: Path, work: Path) -> dict:
    argv = command(tool, work)
    if argv is None or (shutil.which(argv[0]) is None and not Path(argv[0]).exists()):
        return {"tool": tool, "available": False}

    home = work / tool
    shutil.rmtree(home, ignore_errors=True)
    home.mkdir(parents=True)
    if tool == "reprocut":
        (home / "source").mkdir()
        shutil.copyfile(case, home / "source" / "case.c")
    else:
        shutil.copyfile(case, home / "case.c")

    counter = home / "counter"
    counter.write_bytes(b"")
    environment = dict(os.environ, BENCHMARK_COUNTER=str(counter),
                       BENCHMARK_FILE="case.c", BENCHMARK_INCLUDE=str(include),
                       BENCHMARK_POLARITY="failing" if tool == "reprocut" else "interesting")
    start = time.monotonic()
    with (home / "tool.log").open("wb") as log:
        completed = subprocess.run(argv, cwd=home, stdout=log, stderr=subprocess.STDOUT,
                                   env=environment, timeout=BUDGET + 120, check=False)
    elapsed = time.monotonic() - start

    reduced = home / "case.c"
    if tool == "reprocut":
        found = sorted((home / "output").rglob("case.c")) if (home / "output").exists() else []
        reduced = found[0] if found else None
    if reduced is None or not reduced.exists():
        return {"tool": tool, "available": True, "produced_output": False,
                "exit_code": completed.returncode, "seconds": round(elapsed, 1),
                "oracle_calls": counter.stat().st_size}

    record = {"tool": tool, "available": True, "produced_output": True,
              "exit_code": completed.returncode, "seconds": round(elapsed, 1),
              "oracle_calls": counter.stat().st_size}
    record.update(measure(reduced))
    if tool == "reprocut":
        reduction = home / "output" / "reduction.json"
        if reduction.exists():
            record["diagnostic_drift"] = json.loads(
                reduction.read_text())["failure"].get("diagnostic_drift")
    return record


def main() -> int:
    work = Path(sys.argv[1]).resolve()
    case, include = work / "original.c", work / "include"
    original = measure(case)

    rows = [run(tool, case, include, work) for tool in
            ("cvise", "creduce", "shrinkray", "perses", "reprocut")]
    (work / "arena.json").write_text(
        json.dumps({"original": original, "results": rows}, indent=2) + "\n")

    print(f"original{original['bytes']:>12,}{original['lines']:>9,}{original['tokens']:>10,}")
    print(f"{'tool':<11}{'bytes':>9}{'lines':>9}{'tokens':>10}{'oracle':>9}{'seconds':>9}")
    for row in rows:
        if not row.get("available"):
            print(f"{row['tool']:<11}{'not installed':>37}")
        elif not row.get("produced_output"):
            print(f"{row['tool']:<11}{'no output':>28}{row['oracle_calls']:>9,}"
                  f"{row['seconds']:>9.1f}")
        else:
            print(f"{row['tool']:<11}{row['bytes']:>9,}{row['lines']:>9,}{row['tokens']:>10,}"
                  f"{row['oracle_calls']:>9,}{row['seconds']:>9.1f}")
    drift = next((r.get("diagnostic_drift") for r in rows if r["tool"] == "reprocut"), None)
    print()
    print("reprocut drift:", "not measured" if drift is None else
          f"{drift['novel_lines']} novel of {drift['final_lines']} "
          f"(reportable: {drift['reportable']})")


def main() -> int:
    if sys.argv[1] == "--render":
        return render(Path(sys.argv[2]).resolve())
    work = Path(sys.argv[1]).resolve()
    case, include = work / "original.c", work / "include"
    original = measure(case)
    rows = [run(tool, case, include, work) for tool in
            ("cvise", "creduce", "shrinkray", "perses", "reprocut")]
    (work / "arena.json").write_text(
        json.dumps({"original": original, "results": rows}, indent=2) + chr(10))
    report(original, rows)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
