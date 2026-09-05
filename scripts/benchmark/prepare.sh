#!/usr/bin/env bash
# Builds the one-file case every reducer in the arena is given.
set -euo pipefail

WORK="${1:?usage: prepare.sh <workdir>}"
LODEPNG_SHA=8c6a9e30576f07bf470ad6f09458a2dcd7a6a84a

mkdir -p "$WORK/include"
cd "$WORK"

# The header is fixed context, not part of the reduction: every tool is asked to reduce
# exactly one file, so none of them gets a file tree to collapse.
curl -fsSL --retry 5 \
  "https://raw.githubusercontent.com/lvandeve/lodepng/${LODEPNG_SHA}/lodepng.h" \
  -o include/lodepng.h
curl -fsSL --retry 5 \
  "https://raw.githubusercontent.com/lvandeve/lodepng/${LODEPNG_SHA}/lodepng.cpp" \
  -o clean.c

# A benchmark on a file that already fails for other reasons measures nothing, so prove the
# base compiles clean before the injection is what makes it fail.
if ! gcc -fsyntax-only -Werror=int-conversion -I include clean.c 2>clean-errors.log; then
  echo "base file does not compile cleanly; the benchmark would be meaningless" >&2
  head -20 clean-errors.log >&2
  exit 2
fi

cp clean.c original.c
cat >>original.c <<'C'

/* Injected failure site: the only reason this file must not compile. */
int reprocut_benchmark_site(void) {
    char *value = 4242;
    return (int)(long)value;
}
C
wc -c original.c
