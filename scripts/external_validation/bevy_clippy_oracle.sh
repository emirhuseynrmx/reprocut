#!/usr/bin/env bash
set -uo pipefail

run_clippy() {
  cargo clippy \
    -p bevy_ecs \
    --lib \
    --all-features \
    --features bevy_reflect/auto_register_static \
    -- \
    -Dwarnings
}


# `undocumented_unsafe_blocks` fires all over bevy_ecs, so "clippy failed" cannot tell the
# defect under test from one a cut introduced somewhere else. The captured run that proved
# this kept the five original errors and added thirty unrelated ones, and matching on the
# lint name alone accepted it. Decide here instead and emit one line that names the defect.
FOCUS=crates/bevy_ecs/src/world/despawn_all.rs
LINT="unsafe block missing a safety comment"

verdict() {
  local output status counts total other focused elsewhere
  output="$(run_clippy 2>&1)"
  status=$?
  printf '%s
' "$output" >&2

  counts="$(printf '%s
' "$output" | awk -v focus="$FOCUS" -v lint="$LINT" '
    /^error: could not compile/ { next }
    /^error(\[[^]]*\])?: / {
      sub(/^error(\[[^]]*\])?: /, "")
      pending = $0
      total++
      if (pending != lint) other++
      next
    }
    pending != "" && /^ *--> / {
      path = $2
      sub(/:[0-9]+:[0-9]+$/, "", path)
      if (path == focus) focused++; else elsewhere++
      pending = ""
    }
    END { printf "%d %d %d %d", total + 0, other + 0, focused + 0, elsewhere + 0 }
  ')"
  read -r total other focused elsewhere <<<"$counts"

  if [ "$total" -eq 0 ]; then
    echo "BEVY-ORACLE: clippy reported no error; the defect under test is absent" >&2
  elif [ "$other" -gt 0 ]; then
    echo "BEVY-ORACLE: ${other} of ${total} errors are a different lint; not the defect under test" >&2
  elif [ "$elsewhere" -gt 0 ]; then
    echo "BEVY-ORACLE: ${elsewhere} safety-comment errors sit outside ${FOCUS}; not the defect under test" >&2
  elif [ "$focused" -eq 0 ]; then
    echo "BEVY-ORACLE: no error carries a location; not the defect under test" >&2
  else
    echo "BEVY-ORACLE: exactly ${focused} undocumented unsafe block(s) in ${FOCUS}" >&2
  fi
  exit "$status"
}

# Admission runs receive complete pinned snapshots. Reduction candidates contain
# only the files changed by the failing commit and are overlaid onto one reusable
# full workspace so dependencies remain immutable rather than reducible noise.
if [ -f Cargo.toml ]; then
  verdict
fi

candidate="$PWD"
workspace=/work/bevy-oracle-workspace
if [ ! -f "$workspace/Cargo.toml" ]; then
  mkdir -p "$workspace"
  cp -R --no-preserve=ownership,mode /inputs/head/. "$workspace"/
  chmod -R u+rwX "$workspace"
fi

focus_paths=(
  crates/bevy_ecs/src/archetype.rs
  crates/bevy_ecs/src/world/despawn_all.rs
  crates/bevy_ecs/src/world/mod.rs
)
for path in "${focus_paths[@]}"; do
  rm -f -- "$workspace/$path"
  if [ -f "$candidate/$path" ]; then
    mkdir -p "$(dirname "$workspace/$path")"
    cp --no-preserve=ownership,mode "$candidate/$path" "$workspace/$path"
  fi
done

cd "$workspace"
verdict
