#!/usr/bin/env bash
set -euo pipefail
umask 077

case_id="$(jq -r .case_id /case.json)"
attempt_timeout_ms="$(jq -r .attempt_timeout_ms /case.json)"
timeout_minutes="$(jq -r .timeout_minutes /case.json)"
mkdir -p /evidence/admission-logs /evidence/final-verification /work/cargo-home /work/cargo-target

finalize_evidence() {
  local container_rc="$1"
  CONTAINER_RC="$container_rc" python3 - <<'PY'
import csv
import json
import os
from pathlib import Path

case = json.loads(Path('/case.json').read_text())
manifest = dict(case)
manifest['isolation'] = {
    'network': 'none', 'cpus': 2, 'memory': case['memory'], 'pids_limit': 1024,
    'capabilities': 'none', 'no_new_privileges': True, 'runtime_uid': 10001,
}
Path('/evidence/manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
admission_path = Path('/evidence/admission.tsv')
rows = []
if admission_path.exists():
    with admission_path.open(newline='') as handle:
        rows = [
            {'snapshot': row[0], 'run': int(row[1]), 'exit_code': int(row[2]), 'matched': row[3] == 'true'}
            for row in csv.reader(handle, delimiter='\t') if row
        ]
Path('/evidence/admission.json').write_text(json.dumps({'schema_version': 1, 'observations': rows}, indent=2, sort_keys=True) + '\n')
result_path = Path('/evidence/result.json')
if not result_path.exists():
    result_path.write_text(json.dumps({
        'schema_version': 1,
        'status': 'container_failed' if int(os.environ['CONTAINER_RC']) else 'passed',
        'container_exit_code': int(os.environ['CONTAINER_RC']),
    }, indent=2, sort_keys=True) + '\n')
PY
}
trap 'container_rc=$?; finalize_evidence "$container_rc"' EXIT

copy_seed_tree() {
  local source="$1" destination="$2"
  cp -R --no-preserve=ownership,mode "$source"/. "$destination"/
  chmod -R u+rwX "$destination"
}

if [ -d /opt/cargo ]; then
  copy_seed_tree /opt/cargo /work/cargo-home
fi
export CARGO_HOME=/work/cargo-home
export CARGO_TARGET_DIR=/work/cargo-target
export CARGO_NET_OFFLINE=true
export PYTHONDONTWRITEBYTECODE=1
export HOME=/work/home
mkdir -p "$HOME"

mapfile -t oracle_argv < <(jq -r '.oracle_argv[]' /case.json)
mapfile -t required_regex < <(jq -r '.required_regex[]' /case.json)
mapfile -t rejected_regex < <(jq -r '.rejected_regex[]' /case.json)

prepare_snapshot() {
  local source="$1" destination="$2"
  rm -rf "$destination"
  mkdir -p "$destination"
  copy_seed_tree "$source" "$destination"
}

prepare_bevy_reduction_source() {
  local destination="$1"
  local path
  local -a focus_paths=(
    crates/bevy_ecs/src/archetype.rs
    crates/bevy_ecs/src/world/despawn_all.rs
    crates/bevy_ecs/src/world/mod.rs
  )
  rm -rf "$destination"
  mkdir -p "$destination"
  for path in "${focus_paths[@]}"; do
    if [ -f "/inputs/head/$path" ]; then
      mkdir -p "$destination/$(dirname "$path")"
      cp --no-preserve=ownership,mode "/inputs/head/$path" "$destination/$path"
    fi
  done
}

matches_contract() {
  local log="$1" expected="$2" rc="$3"
  if [ "$expected" = pass ]; then
    [ "$rc" -eq 0 ] || return 1
    return 0
  fi
  [ "$rc" -ne 0 ] || return 1
  local pattern
  for pattern in "${required_regex[@]}"; do
    grep -E -q -- "$pattern" "$log" || return 1
  done
  for pattern in "${rejected_regex[@]}"; do
    if grep -E -q -- "$pattern" "$log"; then
      return 1
    fi
  done
}

observe() {
  local source="$1" label="$2" expected="$3" index="$4"
  local candidate="/work/observation-${label}-${index}"
  local log="/evidence/admission-logs/${label}-${index}.log"
  local -a observation_argv=("${oracle_argv[@]}")
  if [ "$case_id" = ipe ]; then
    IPE_BIN="/usr/local/bin/ipe-${label}"
    observation_argv=(env "IPE_BIN=$IPE_BIN" "${oracle_argv[@]}")
  fi
  prepare_snapshot "$source" "$candidate"
  set +e
  (cd "$candidate" && timeout --signal=TERM --kill-after=10s "$((attempt_timeout_ms / 1000))s" "${observation_argv[@]}") >"$log" 2>&1
  local rc=$?
  set -e
  printf '%s\t%s\t%s\t%s\n' "$label" "$index" "$rc" "$(matches_contract "$log" "$expected" "$rc" && echo true || echo false)" >> /evidence/admission.tsv
  matches_contract "$log" "$expected" "$rc"
  rm -rf "$candidate"
}

: > /evidence/admission.tsv
for index in 1 2 3; do observe /inputs/base base pass "$index"; done
for index in 1 2 3; do observe /inputs/head head fail "$index"; done

if [ "$case_id" = bevy ]; then
  prepare_bevy_reduction_source /work/reduction-source
else
  prepare_snapshot /inputs/head /work/reduction-source
fi
reprocut_args=(
  reduce
  --root /work/reduction-source
  --output /evidence/reprocut
  --jobs 1
  --timeout-ms "$attempt_timeout_ms"
  --max-output-bytes 1048576
  --oracle-stream combined
  --oracle-mode regex
)
for pattern in "${required_regex[@]}"; do reprocut_args+=(--failure-regex "$pattern"); done
for pattern in "${rejected_regex[@]}"; do reprocut_args+=(--reject-regex "$pattern"); done

reduction_argv=("${oracle_argv[@]}")
if [ "$case_id" = ipe ]; then
  reduction_argv=(env IPE_BIN=/usr/local/bin/ipe-head "${oracle_argv[@]}")
fi
case "$case_id" in
  openruyi|ipe)
    reprocut_args+=(--ecosystem none --prepare none)
    ;;
  bevy)
    reprocut_args+=(--ecosystem none --prepare none)
    ;;
esac

set +e
timeout --signal=TERM --kill-after=30s "$((timeout_minutes * 60))s" \
  /opt/reprocut/reprocut "${reprocut_args[@]}" -- "${reduction_argv[@]}" \
  > /evidence/reprocut.stdout.log 2> /evidence/reprocut.stderr.log
reprocut_rc=$?
set -e
printf '%s\n' "$reprocut_rc" > /evidence/reprocut.exit-code
if [ "$reprocut_rc" -ne 0 ]; then
  python3 - <<'PY'
import json
from pathlib import Path
Path('/evidence/result.json').write_text(json.dumps({
    'schema_version': 1,
    'status': 'reprocut_failed',
    'exit_code': int(Path('/evidence/reprocut.exit-code').read_text()),
}, indent=2, sort_keys=True) + '\n')
PY
  exit "$reprocut_rc"
fi

/opt/reprocut/reprocut verify /evidence/reprocut --json > /evidence/reprocut-verify.json

: > /evidence/final-verification.tsv
final_oracle_argv=("${oracle_argv[@]}")
if [ "$case_id" = ipe ]; then
  final_oracle_argv=(env IPE_BIN=/usr/local/bin/ipe-head "${oracle_argv[@]}")
fi
for index in 1 2 3; do
  candidate="/work/final-${index}"
  log="/evidence/final-verification/run-${index}.log"
  prepare_snapshot /evidence/reprocut/project "$candidate"
  set +e
  (cd "$candidate" && timeout --signal=TERM --kill-after=10s "$((attempt_timeout_ms / 1000))s" "${final_oracle_argv[@]}") >"$log" 2>&1
  rc=$?
  set -e
  preserved="$(matches_contract "$log" fail "$rc" && echo true || echo false)"
  printf '%s\t%s\t%s\n' "$index" "$rc" "$preserved" >> /evidence/final-verification.tsv
  [ "$preserved" = true ]
  rm -rf "$candidate"
done

python3 - <<'PY'
import csv
import json
from pathlib import Path

case = json.loads(Path('/case.json').read_text())
with Path('/evidence/admission.tsv').open(newline='') as handle:
    admission_rows = [
        {'snapshot': row[0], 'run': int(row[1]), 'exit_code': int(row[2]), 'matched': row[3] == 'true'}
        for row in csv.reader(handle, delimiter='\t')
    ]
with Path('/evidence/final-verification.tsv').open(newline='') as handle:
    final_rows = [
        {'run': int(row[0]), 'exit_code': int(row[1]), 'preserved': row[2] == 'true'}
        for row in csv.reader(handle, delimiter='\t')
    ]
manifest = dict(case)
manifest.update({
    'isolation': {
        'network': 'none', 'cpus': 2, 'memory': case['memory'], 'pids_limit': 1024,
        'capabilities': 'none', 'no_new_privileges': True, 'runtime_uid': 10001,
    },
    'reduction_oracle_argv': case['oracle_argv'],
})
Path('/evidence/manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
Path('/evidence/admission.json').write_text(json.dumps({'schema_version': 1, 'observations': admission_rows}, indent=2, sort_keys=True) + '\n')
reduction = json.loads(Path('/evidence/reprocut/reduction.json').read_text())
original = reduction['measurements']['original']
retained = reduction['measurements']['retained']
# File count alone is the wrong question. Cutting one 100 MB source file down to
# twelve lines leaves the count at 1 -> 1 and would be rejected, while deleting a
# single empty file from a 200 MB tree would pass. A case counts as reduced when
# no dimension grew and at least one shrank.
DIMENSIONS = ('files', 'bytes', 'lines')
measured = {
    dimension: (original.get(dimension), retained.get(dimension))
    for dimension in DIMENSIONS
    if original.get(dimension) is not None and retained.get(dimension) is not None
}
if not measured:
    raise SystemExit('reduction evidence reports no comparable size dimension')
grew = {
    dimension: pair for dimension, pair in measured.items() if pair[1] > pair[0]
}
if grew:
    detail = ', '.join(f'{name}: {before} -> {after}' for name, (before, after) in sorted(grew.items()))
    raise SystemExit(f'reduction grew in at least one dimension: {detail}')
shrank = {
    dimension: pair for dimension, pair in measured.items() if pair[1] < pair[0]
}
if not shrank:
    detail = ', '.join(f'{name}: {before} -> {after}' for name, (before, after) in sorted(measured.items()))
    raise SystemExit(f'reduction is not strictly smaller in any dimension: {detail}')
Path('/evidence/result.json').write_text(json.dumps({
    'schema_version': 2, 'status': 'passed', 'final_verification': final_rows,
    'declared_metrics': sorted(measured),
    'reduced_metrics': sorted(shrank),
    'original': {name: original.get(name) for name in DIMENSIONS},
    'retained': {name: retained.get(name) for name in DIMENSIONS},
    'reduction': reduction,
}, indent=2, sort_keys=True) + '\n')
PY
