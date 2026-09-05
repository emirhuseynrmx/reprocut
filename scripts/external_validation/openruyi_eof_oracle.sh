#!/usr/bin/env bash
set -euo pipefail

mapfile -d '' -t files < <(
  find . -type f -not -path './.git/*' -printf '%P\0' | LC_ALL=C sort -z
)
if [ "${#files[@]}" -eq 0 ]; then
  exit 0
fi

set +e
/opt/precommit/bin/end-of-file-fixer -- "${files[@]}"
hook_rc=$?
set -e
if [ "$hook_rc" -ne 0 ]; then
  printf '%s\n' 'files were modified by this hook' >&2
  exit 1
fi
