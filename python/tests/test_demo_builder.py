from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

from build_demo import format_summary  # noqa: E402


def test_terminal_summary_is_portable_to_the_windows_code_page() -> None:
    summary = format_summary(
        output="demo/result", original_files=18, retained_files=3, attempts=19
    )

    assert "18 -> 3 files" in summary
    summary.encode("cp1254", errors="strict")
