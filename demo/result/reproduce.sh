#!/usr/bin/env sh
set -eu
cd -- "$(dirname -- "$0")/project"
exec 'python' 'bug.py'
