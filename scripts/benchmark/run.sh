#!/usr/bin/env bash
# Head-to-head reduction on one single-file C failure.
#
# The case is deliberately the kind cvise was built for and ReproCut was not: one file, no
# repository structure to collapse. Whatever ReproCut wins or loses here, it wins or loses
# on reduction itself.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="${1:?usage: run.sh <workdir>}"
LODEPNG_SHA=8c6a9e30576f07bf470ad6f09458a2dcd7a6a84a
BUDGET_SECONDS=${BENCHMARK_BUDGET_SECONDS:-900}

mkdir -p "$WORK"
cd "$WORK"

# The header is fixed context, not part of the reduction: both tools are asked to reduce
# exactly one file, so neither gets a file tree to collapse.
mkdir -p include
curl -fsSL --retry 5 \
  "https://raw.githubusercontent.com/lvandeve/lodepng/${LODEPNG_SHA}/lodepng.h" \
  -o include/lodepng.h
curl -fsSL --retry 5 \
  "https://raw.githubusercontent.com/lvandeve/lodepng/${LODEPNG_SHA}/lodepng.cpp" \
  -o original.c
export BENCHMARK_INCLUDE="$PWD/include"
cp original.c clean.c
cat >>original.c <<'C'

/* Injected failure site: the only reason this file must not compile. */
int reprocut_benchmark_site(void) {
    char *value = 4242;
    return (int)(long)value;
}
C

# A benchmark on a file that already fails for other reasons measures nothing, so prove
# the base compiles clean before the injection is what makes it fail.
if ! gcc -fsyntax-only -Werror=int-conversion -I include clean.c 2>clean-errors.log; then
  echo "base file does not compile cleanly; the benchmark would be meaningless" >&2
  head -20 clean-errors.log >&2
  exit 2
fi

original_bytes=$(wc -c <original.c)
original_lines=$(wc -l <original.c)
original_tokens=$(python3 "$ROOT/scripts/benchmark/tokens.py" original.c)

measure() {
  local name="$1" file="$2" counter="$3" seconds="$4"
  python3 - "$name" "$file" "$counter" "$seconds" \
    "$original_bytes" "$original_lines" "$original_tokens" "$ROOT" <<'PY'
import json, subprocess, sys
from pathlib import Path
name, file, counter, seconds, ob, ol, ot, root = sys.argv[1:9]
path = Path(file)
tokens = subprocess.run(
    ["python3", f"{root}/scripts/benchmark/tokens.py", file],
    capture_output=True, text=True, check=True).stdout.strip()
record = {
    "tool": name,
    "seconds": round(float(seconds), 1),
    "oracle_calls": len(Path(counter).read_bytes()),
    "bytes": path.stat().st_size,
    "lines": len(path.read_text(errors="replace").splitlines()),
    "tokens": int(tokens),
    "original": {"bytes": int(ob), "lines": int(ol), "tokens": int(ot)},
}
Path(f"{name}-result.json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record))
PY
}

# --- cvise -------------------------------------------------------------------
rm -rf cvise-run && mkdir cvise-run && cd cvise-run
cp ../original.c case.c
: >counter
start=$(date +%s.%N)
BENCHMARK_COUNTER="$PWD/counter" BENCHMARK_FILE=case.c \
  BENCHMARK_INCLUDE="$BENCHMARK_INCLUDE" \
  timeout "${BUDGET_SECONDS}s" cvise --n 1 "$ROOT/scripts/benchmark/interesting.sh" case.c \
  >cvise.log 2>&1 || echo "cvise exited $?" >>cvise.log
elapsed=$(echo "$(date +%s.%N) - $start" | bc)
cd ..
measure cvise cvise-run/case.c cvise-run/counter "$elapsed"

# --- reprocut ----------------------------------------------------------------
rm -rf reprocut-src reprocut-out && mkdir reprocut-src && cp original.c reprocut-src/case.c
: >reprocut-counter
start=$(date +%s.%N)
BENCHMARK_COUNTER="$PWD/reprocut-counter" BENCHMARK_FILE=case.c \
  BENCHMARK_INCLUDE="$BENCHMARK_INCLUDE" \
  timeout "$((BUDGET_SECONDS + 60))s" "$ROOT/target/release/reprocut" reduce \
  --root reprocut-src \
  --output reprocut-out \
  --jobs 1 \
  --ecosystem none \
  --prepare none \
  --oracle-stream combined \
  --oracle-mode regex \
  --failure-regex '^BENCHMARK: the injected diagnostic is present$' \
  --max-duration-secs "$BUDGET_SECONDS" \
  -- "$ROOT/scripts/benchmark/interesting.sh" \
  >reprocut.log 2>&1 || echo "reprocut exited $?" >>reprocut.log
elapsed=$(echo "$(date +%s.%N) - $start" | bc)
measure reprocut reprocut-out/project/case.c reprocut-counter "$elapsed"

python3 - <<'PY'
import json
from pathlib import Path
rows = [json.loads(Path(f"{n}-result.json").read_text()) for n in ("cvise", "reprocut")]
original = rows[0]["original"]
drift = None
reduction = Path("reprocut-out/reduction.json")
if reduction.exists():
    drift = json.loads(reduction.read_text())["failure"].get("diagnostic_drift")
summary = {"original": original, "results": rows, "reprocut_diagnostic_drift": drift}
Path("comparison.json").write_text(json.dumps(summary, indent=2) + "\n")
head = f"{'tool':<10}{'bytes':>10}{'lines':>8}{'tokens':>9}{'oracle':>8}{'seconds':>9}"
print(f"original  {original['bytes']:>10,}{original['lines']:>8,}{original['tokens']:>9,}")
print(head)
for row in rows:
    print(f"{row['tool']:<10}{row['bytes']:>10,}{row['lines']:>8,}{row['tokens']:>9,}"
          f"{row['oracle_calls']:>8,}{row['seconds']:>9.1f}")
print()
print("reprocut diagnostic drift:", "not measured" if drift is None else
      f"{drift['novel_lines']} novel of {drift['final_lines']} (reportable: {drift['reportable']})")
PY
