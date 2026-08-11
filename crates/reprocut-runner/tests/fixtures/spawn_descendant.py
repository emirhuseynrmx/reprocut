"""Cross-platform process-tree fixture used by external runner smoke tests."""

from __future__ import annotations

import subprocess
import sys
import time
from pathlib import Path


def main() -> None:
    mode, marker = sys.argv[1], Path(sys.argv[2])
    if mode == "parent":
        subprocess.Popen([sys.executable, __file__, "descendant", str(marker)])
        time.sleep(30)
    else:
        time.sleep(0.25)
        marker.write_text("descendant-survived", encoding="utf-8")


if __name__ == "__main__":
    main()
