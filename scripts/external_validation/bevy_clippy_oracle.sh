#!/usr/bin/env bash
set -euo pipefail

run_clippy() {
  exec cargo clippy \
    -p bevy_ecs \
    --lib \
    --all-features \
    --features bevy_reflect/auto_register_static \
    -- \
    -Dwarnings
}

# Admission runs receive complete pinned snapshots. Reduction candidates contain
# only the files changed by the failing commit and are overlaid onto one reusable
# full workspace so dependencies remain immutable rather than reducible noise.
if [ -f Cargo.toml ]; then
  run_clippy
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
run_clippy
