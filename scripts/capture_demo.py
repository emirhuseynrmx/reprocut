#!/usr/bin/env python3
"""Capture and verify the deterministic ReproCut report GIF."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parent.parent
REPORT = ROOT / "demo" / "result" / "report.html"
FRAMES = ROOT / "output" / "playwright" / "gif-frames"
OUTPUT = ROOT / "assets" / "reprocut-demo.gif"
EXPECTED_SIZE = (1200, 675)
EXPECTED_FRAMES = 24
MAX_BYTES = 8 * 1024 * 1024


def bundled_node() -> tuple[Path, Path]:
    runtime = (
        Path.home()
        / ".cache"
        / "codex-runtimes"
        / "codex-primary-runtime"
        / "dependencies"
        / "node"
    )
    executable = runtime / "bin" / ("node.exe" if os.name == "nt" else "node")
    module = runtime / "node_modules" / "playwright"
    if executable.is_file() and module.is_dir():
        return executable, module

    discovered = shutil.which("node")
    if discovered is None:
        raise RuntimeError("Node.js was not found")
    result = subprocess.run(
        [discovered, "-p", "require.resolve('playwright/package.json')"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return Path(discovered), Path(result.stdout.strip()).parent


def prepare_frame_directory() -> None:
    output_root = (ROOT / "output").resolve()
    resolved_frames = FRAMES.resolve()
    if output_root not in resolved_frames.parents:
        raise RuntimeError("frame directory escaped the repository output root")
    if FRAMES.exists():
        shutil.rmtree(FRAMES)
    FRAMES.mkdir(parents=True)


def capture_frames() -> None:
    node, playwright = bundled_node()
    environment = {**os.environ, "REPROCUT_PLAYWRIGHT_MODULE": str(playwright)}
    subprocess.run(
        [node, ROOT / "scripts" / "capture_frames.cjs", REPORT, FRAMES],
        check=True,
        cwd=ROOT,
        env=environment,
    )


def encode_gif() -> None:
    frame_paths = sorted(FRAMES.glob("frame-*.png"))
    if len(frame_paths) != EXPECTED_FRAMES:
        raise RuntimeError(f"expected {EXPECTED_FRAMES} frames, found {len(frame_paths)}")

    frames: list[Image.Image] = []
    for frame_path in frame_paths:
        with Image.open(frame_path) as frame:
            if frame.size != EXPECTED_SIZE:
                raise RuntimeError(f"unexpected frame dimensions: {frame.size}")
            frames.append(frame.convert("P", palette=Image.Palette.ADAPTIVE, colors=128))

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    durations = [700] + [90] * 22 + [900]
    frames[0].save(
        OUTPUT,
        format="GIF",
        save_all=True,
        append_images=frames[1:],
        duration=durations,
        loop=0,
        optimize=True,
        disposal=2,
    )


def verify_gif() -> None:
    size = OUTPUT.stat().st_size
    if not 0 < size < MAX_BYTES:
        raise RuntimeError(f"GIF size outside bounded contract: {size} bytes")
    with Image.open(OUTPUT) as animation:
        if animation.format != "GIF":
            raise RuntimeError(f"unexpected animation format: {animation.format}")
        if animation.size != EXPECTED_SIZE:
            raise RuntimeError(f"unexpected GIF dimensions: {animation.size}")
        if getattr(animation, "n_frames", 1) < 20:
            raise RuntimeError("GIF contains fewer than 20 frames")
        if animation.info.get("loop") != 0:
            raise RuntimeError("GIF is not configured for an infinite loop")
        print(
            f"verified GIF: {animation.n_frames} frames, "
            f"{animation.size[0]}x{animation.size[1]}, {size} bytes"
        )


def main() -> int:
    if not REPORT.is_file():
        raise RuntimeError(f"demo report does not exist: {REPORT}")
    prepare_frame_directory()
    capture_frames()
    encode_gif()
    verify_gif()
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"capture failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
