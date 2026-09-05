#!/usr/bin/env python3
"""Unified interestingness test for Python reducers in the Arena.

Handles exit-code polarity transparently:
- Standard convention (picire, shrinkray): exit 0 = interesting (failure reproduced)
- ReproCut convention: exit 1 = failing (failure reproduced)
Controlled by BENCHMARK_POLARITY ("failing" vs "interesting").
"""

from __future__ import annotations

import os
import re
import subprocess
import sys

PYTHON = sys.executable
FILE = os.environ.get("BENCHMARK_FILE", "case.py")
COUNTER = os.environ.get("BENCHMARK_COUNTER")
POLARITY = os.environ.get("BENCHMARK_POLARITY", "interesting")
TARGET_REGEX = os.environ.get(
    "BENCHMARK_TARGET_REGEX",
    r"TypeError: unsupported operand type\(s\) for \+: 'decimal\.Decimal' and 'str'",
)

if len(sys.argv) > 1 and sys.argv[1]:
    FILE = sys.argv[1]

# Count oracle execution
if COUNTER:
    try:
        with open(COUNTER, "ab") as f:
            f.write(b"x")
    except Exception:
        pass

try:
    proc = subprocess.run(
        [PYTHON, FILE],
        capture_output=True,
        text=True,
        timeout=10,
    )
    output = proc.stderr + "\n" + proc.stdout
    target_present = (proc.returncode != 0) and bool(re.search(TARGET_REGEX, output))
except Exception:
    target_present = False

if target_present:
    print("BENCHMARK: the injected diagnostic is present")
    sys.exit(1 if POLARITY == "failing" else 0)
else:
    print("BENCHMARK: the injected diagnostic is absent")
    sys.exit(0 if POLARITY == "failing" else 1)
