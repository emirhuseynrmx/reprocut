"""Minimal checkout entry point with an intentionally real type failure."""

from __future__ import annotations

import json
from pathlib import Path

from checkout import quote_total


order = json.loads(Path("fixtures/order.json").read_text(encoding="utf-8"))
print(quote_total(order))
