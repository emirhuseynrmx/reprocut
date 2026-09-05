#!/usr/bin/env bash
# The single interestingness test both reducers are given.
#
# cvise reads the exit code; ReproCut matches the printed line. One script serves both so
# neither tool is answering a different question.
set -uo pipefail

file="${BENCHMARK_FILE:-case.c}"
printf 'x' >>"${BENCHMARK_COUNTER:?BENCHMARK_COUNTER is required}"

output="$(gcc -fsyntax-only -Werror=int-conversion "$file" 2>&1)"
if printf '%s\n' "$output" | grep -q "makes pointer from integer without a cast"; then
  echo "BENCHMARK: the injected diagnostic is present"
  exit 0
fi
echo "BENCHMARK: the injected diagnostic is absent"
exit 1
