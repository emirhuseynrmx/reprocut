#!/usr/bin/env bash
set -uo pipefail

if [ -f tools/scripts/regen-sky-examples.sh ]; then
  regen=tools/scripts/regen-sky-examples.sh
elif [ -f scripts/regen-sky-examples.sh ]; then
  regen=scripts/regen-sky-examples.sh
else
  echo "ipe regen oracle: upstream regen script is missing" >&2
  exit 2
fi

output="$(bash "$regen" --check 2>&1)"
status=$?
printf '%s\n' "$output" >&2

# The upstream check prints one summary line for several unrelated problems, and its detail
# is a diff report: deleting the corpus, damaging an unrelated port, or breaking the
# transform all make something differ. Matching that text cannot tell those apart, so decide
# here and emit one line that names the defect under test.
stale="$(
  printf '%s\n' "$output" \
    | sed -n 's|^regen --check: examples/sky/ipe/\([^ ]*\) differs from re-deriving it.*|\1|p' \
    | LC_ALL=C sort -u | tr '\n' ' '
)"
stale="${stale% }"

if printf '%s\n' "$output" | grep -q 'missing (run regen)'; then
  echo "IPE-ORACLE: ports are absent, not stale; not the defect under test" >&2
elif [ "$stale" = "08-notes-app" ]; then
  echo "IPE-ORACLE: exactly 08-notes-app is stale against a fresh transform" >&2
else
  echo "IPE-ORACLE: stale set is [${stale}], not the defect under test" >&2
fi
exit "$status"
