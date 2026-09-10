#!/usr/bin/env bash
# Clones the openruyi case at its pinned failing commit.
set -euo pipefail

WORK="${1:?usage: repo_case.sh <workdir>}"
REPO=https://github.com/redrose2100/openruyi-precommit-hooks.git
HEAD_SHA=1a0e915e4e0daa89cce0b97dc488801fe4225a0e

mkdir -p "$WORK"
cd "$WORK"
rm -rf case
git init -q case
cd case
git remote add origin "$REPO"
git fetch -q --depth 1 origin "$HEAD_SHA"
git checkout -q FETCH_HEAD
rm -rf .git
cd ..
find case -type f | wc -l
