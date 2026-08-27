#!/usr/bin/env bash
set -euo pipefail

if [ -f tools/scripts/regen-sky-examples.sh ]; then
  exec bash tools/scripts/regen-sky-examples.sh --check
fi
if [ -f scripts/regen-sky-examples.sh ]; then
  exec bash scripts/regen-sky-examples.sh --check
fi

echo "ipe regen oracle: upstream regen script is missing" >&2
exit 2
