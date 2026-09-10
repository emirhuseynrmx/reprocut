#!/usr/bin/env bash
# The one decision every reducer in the arena is given.
#
# The tools disagree on what an exit code means: cvise, creduce, shrinkray and Perses read
# zero as "this is the failure", while ReproCut needs the baseline command to fail before it
# will start. Inverting the code for one of them keeps the decision identical and the
# comparison honest; giving them different tests would not.
set -uo pipefail

file="${BENCHMARK_FILE:-case.c}"
printf 'x' >>"${BENCHMARK_COUNTER:?BENCHMARK_COUNTER is required}"

output="$(gcc -fsyntax-only -Werror=int-conversion -I"${BENCHMARK_INCLUDE:-.}" "$file" 2>&1)"
if printf '%s\n' "$output" | grep -q "makes pointer from integer without a cast"; then
  echo "BENCHMARK: the injected diagnostic is present"
  [ "${BENCHMARK_POLARITY:-interesting}" = failing ] && exit 1
  exit 0
fi
echo "BENCHMARK: the injected diagnostic is absent"
[ "${BENCHMARK_POLARITY:-interesting}" = failing ] && exit 0
exit 1
