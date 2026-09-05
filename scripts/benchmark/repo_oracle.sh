#!/usr/bin/env bash
# The one decision the repo arena gives every reducer.
#
# The upstream failure is a pre-commit hook rewriting a file that is missing its final
# newline, so the tree still fails only while that specific file is both present and still
# wrong. Requiring the file name as well as the hook message is what stops a reducer from
# "succeeding" by leaving some other unfixed file behind.
set -uo pipefail

printf 'x' >>"${BENCHMARK_COUNTER:?BENCHMARK_COUNTER is required}"

mapfile -d '' -t files < <(
  find . -type f -not -path './.git/*' -printf '%P\0' | LC_ALL=C sort -z
)
present=0
if [ "${#files[@]}" -gt 0 ]; then
  output="$("${FIXER:-end-of-file-fixer}" -- "${files[@]}" 2>&1)"
  if [ $? -ne 0 ] \
    && printf '%s\n' "$output" | grep -qE 'Fixing (tests/)?check_spec_bcond'; then
    present=1
  fi
fi

if [ "$present" -eq 1 ]; then
  echo "REPO-ARENA: files were modified by this hook; check_spec_bcond is still unfixed"
  [ "${BENCHMARK_POLARITY:-interesting}" = failing ] && exit 1
  exit 0
fi
echo "REPO-ARENA: the hook left the tree alone; not the defect under test"
[ "${BENCHMARK_POLARITY:-interesting}" = failing ] && exit 0
exit 1
