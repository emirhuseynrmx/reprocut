from __future__ import annotations

import json
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[2]


def test_checked_in_demo_is_measured_and_reproducible() -> None:
    result = ROOT / "demo" / "result"
    metadata = json.loads((result / "reduction.json").read_text(encoding="utf-8"))

    assert metadata["original_files"] == 18
    assert metadata["retained_files"] == 3
    assert metadata["final_verifications"] == 3
    assert metadata["inconclusive_attempts"] == 0
    assert metadata["kept_files"] == ["bug.py", "checkout.py", "fixtures/order.json"]
    assert sorted(
        path.relative_to(result / "project").as_posix()
        for path in (result / "project").rglob("*")
        if path.is_file()
    ) == metadata["kept_files"]


def test_demo_gif_contract() -> None:
    gif_path = ROOT / "assets" / "reprocut-demo.gif"
    assert 0 < gif_path.stat().st_size < 8 * 1024 * 1024

    with Image.open(gif_path) as animation:
        assert animation.format == "GIF"
        assert animation.size == (1200, 675)
        assert animation.n_frames == 24
        assert animation.info.get("loop") == 0
