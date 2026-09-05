"""Multi-reducer arena driver for Python failures.

Compares reference and modern testcase reducers:
- picire: University of Szeged (Renata Hodovan & Akos Kiss) - official published ddmin reference.
- picireny: University of Szeged - official published HDD reference (ANTLR).
- shrinkray: David R. MacIver (author of Hypothesis) - tree-sitter & CST based modern reducer.
- reprocut: ReproCut 0.1.0 engine (Tree-sitter AST & graph-aware).

Each tool gets its own directory, its own copy of the case, and the same interestingness test.
Tools that are unavailable on the host environment are reported cleanly as unavailable.
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
TEST_BAT = ROOT / "scripts" / "benchmark" / "interesting_python.bat"
TEST_PY = ROOT / "scripts" / "benchmark" / "interesting_python.py"
TOKEN = re.compile(r"[A-Za-z_]\w*|\d+\.?\d*|[^\s\w]")
BUDGET = int(os.environ.get("BENCHMARK_BUDGET_SECONDS", "300"))
PYTHON = Path(sys.executable)
REPROCUT = ROOT / "target" / "release" / "reprocut.exe"


def tool_available(tool: str) -> tuple[bool, str]:
    """Check if tool is installed and supported on the current platform."""
    if tool == "picire":
        if shutil.which("picire") is None:
            return False, "not installed (pip install picire)"
        return True, "installed"
    elif tool == "picireny":
        if shutil.which("picireny") is None:
            return False, "not installed (pip install picireny)"
        if shutil.which("java") is None:
            return False, "unavailable (requires Java for ANTLR)"
        return True, "installed"
    elif tool == "shrinkray":
        if shutil.which("shrinkray") is None:
            return False, "not installed (pip install shrinkray)"
        try:
            import resource
            return True, "installed"
        except ImportError:
            return False, "unavailable (POSIX only: missing resource module on Windows)"
    elif tool == "reprocut":
        if not REPROCUT.exists():
            return False, "binary not built (cargo build --release -p reprocut-cli)"
        return True, "installed"
    return False, "unknown tool"


def command(tool: str, work: Path) -> list[str] | None:
    avail, _ = tool_available(tool)
    if not avail:
        return None

    if tool == "picire":
        return ["picire", "-i", "case.py", "--test", str(TEST_BAT), "-o", "out", "-j", "1", "--quiet"]
    if tool == "picireny":
        return ["picireny", "-i", "case.py", "--test", str(TEST_BAT), "-o", "out", "-j", "1", "--quiet"]
    if tool == "shrinkray":
        return ["shrinkray", "--parallelism", "1", str(TEST_BAT), "case.py"]
    if tool == "reprocut":
        return [
            str(REPROCUT), "reduce",
            "--root", "source", "--output", "output", "--jobs", "1",
            "--ecosystem", "none", "--prepare", "none",
            "--oracle-stream", "combined", "--oracle-mode", "regex",
            "--failure-regex", "^BENCHMARK: the injected diagnostic is present$",
            "--max-duration-secs", str(BUDGET), "--", str(TEST_BAT), "case.py"
        ]
    return None


def measure(path: Path) -> dict:
    text = path.read_text(errors="replace")
    return {
        "bytes": path.stat().st_size,
        "lines": len(text.splitlines()),
        "tokens": len(TOKEN.findall(text)),
    }


def run(tool: str, case: Path, work: Path) -> dict:
    avail, reason = tool_available(tool)
    if not avail:
        return {"tool": tool, "available": False, "reason": reason}

    argv = command(tool, work)
    if argv is None:
        return {"tool": tool, "available": False, "reason": reason}

    home = work / tool
    shutil.rmtree(home, ignore_errors=True)
    home.mkdir(parents=True)

    if tool == "reprocut":
        (home / "source").mkdir()
        shutil.copyfile(case, home / "source" / "case.py")
    else:
        shutil.copyfile(case, home / "case.py")

    counter = home / "counter"
    counter.write_bytes(b"")

    env = dict(
        os.environ,
        BENCHMARK_COUNTER=str(counter),
        BENCHMARK_FILE="case.py",
        BENCHMARK_POLARITY="failing" if tool == "reprocut" else "interesting",
    )

    start = time.monotonic()
    with (home / "tool.log").open("wb") as log:
        completed = subprocess.run(
            argv,
            cwd=home,
            stdout=log,
            stderr=subprocess.STDOUT,
            env=env,
            timeout=BUDGET + 60,
            check=False,
        )
    elapsed = time.monotonic() - start

    reduced: Path | None = None
    if tool == "reprocut":
        found = sorted((home / "output").rglob("case.py")) if (home / "output").exists() else []
        reduced = found[0] if found else None
    elif tool in ("picire", "picireny"):
        candidate = home / "out" / "case.py"
        reduced = candidate if candidate.exists() else None
    else:
        reduced = home / "case.py"

    oracle_calls = counter.stat().st_size if counter.exists() else 0

    if reduced is None or not reduced.exists():
        return {
            "tool": tool,
            "available": True,
            "produced_output": False,
            "exit_code": completed.returncode,
            "seconds": round(elapsed, 1),
            "oracle_calls": oracle_calls,
        }

    record = {
        "tool": tool,
        "available": True,
        "produced_output": True,
        "exit_code": completed.returncode,
        "seconds": round(elapsed, 1),
        "oracle_calls": oracle_calls,
    }
    record.update(measure(reduced))

    # Evaluate diagnostic preservation
    check_env = dict(os.environ, BENCHMARK_FILE=str(reduced), BENCHMARK_POLARITY="interesting")
    check_proc = subprocess.run(
        [str(PYTHON), str(TEST_PY), str(reduced)],
        capture_output=True,
        text=True,
        env=check_env,
    )
    reproduced = check_proc.returncode == 0
    record["reproduced_original_bug"] = reproduced

    if tool == "reprocut":
        reduction = home / "output" / "reduction.json"
        if reduction.exists():
            record["diagnostic_drift"] = json.loads(
                reduction.read_text(encoding="utf-8")
            )["failure"].get("diagnostic_drift")
    return record


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python arena_python.py <workdir>")
        return 1

    work = Path(sys.argv[1]).resolve()
    case = work / "original.py"
    if not case.exists():
        print(f"Error: {case} does not exist. Please prepare the benchmark case first.")
        return 1

    original = measure(case)
    tools = ("picire", "picireny", "shrinkray", "reprocut")
    rows = [run(tool, case, work) for tool in tools]

    (work / "arena_python.json").write_text(
        json.dumps({"original": original, "results": rows}, indent=2) + "\n",
        encoding="utf-8",
    )

    print("=" * 95)
    print(" PYTHON REDUCTION ARENA: HEAD-TO-HEAD BENCHMARK")
    print("=" * 95)
    print(f"original  bytes: {original['bytes']:>8,} | lines: {original['lines']:>5,} | tokens: {original['tokens']:>6,}")
    print("-" * 95)
    print(f"{'Tool':<12}{'Bytes':>9}{'Lines':>8}{'Tokens':>9}{'Oracle':>9}{'Seconds':>10}{'Status / Reason':>35}")
    print("-" * 95)

    for row in rows:
        if not row.get("available"):
            print(f"{row['tool']:<12}{'--':>9}{'--':>8}{'--':>9}{'--':>9}{'--':>10}{row.get('reason', 'unavailable'):>35}")
        elif not row.get("produced_output"):
            print(f"{row['tool']:<12}{'no output':>26}{row['oracle_calls']:>9,}{row['seconds']:>10.1f}{'Failed to produce output':>35}")
        else:
            bug_stat = "Bug Kept (0 novel)" if row.get("reproduced_original_bug") else "BUG DRIFTED/LOST"
            print(f"{row['tool']:<12}{row['bytes']:>9,}{row['lines']:>8,}{row['tokens']:>9,}{row['oracle_calls']:>9,}{row['seconds']:>10.1f}{bug_stat:>35}")

    drift = next((r.get("diagnostic_drift") for r in rows if r["tool"] == "reprocut"), None)
    print("=" * 95)
    if drift is not None:
        print(f"ReproCut drift report: {drift['novel_lines']} novel lines of {drift['final_lines']} (reportable: {drift['reportable']})")
    print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
