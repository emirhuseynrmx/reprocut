"""Counts C tokens, so size is compared in something less arbitrary than bytes."""

import re
import sys
from pathlib import Path

TOKEN = re.compile(r"[A-Za-z_]\w*|\d+\.?\d*|[^\s\w]")


def main() -> int:
    text = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    print(len(TOKEN.findall(text)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
