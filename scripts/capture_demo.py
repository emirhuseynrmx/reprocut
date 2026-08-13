#!/usr/bin/env python3
"""Render the deterministic evidence-driven ReproCut demo animation."""

from __future__ import annotations

import json
import math
import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / "demo" / "result" / "reduction.json"
OUTPUT = ROOT / "assets" / "reprocut-demo.gif"
BANNER = ROOT / "assets" / "reprocut-banner.svg"
SIZE = (1200, 675)
FRAME_COUNT = 24
MAX_BYTES = 8 * 1024 * 1024

PAPER = "#f2efe6"
INK = "#171a1d"
MUTED = "#66717d"
GRID = "#c9cfd4"
COBALT = "#3157d5"
RUST = "#bf4e3a"
PROOF = "#247655"
WHITE = "#fffdf8"


def font(size: int, *, mono: bool = False, bold: bool = False) -> ImageFont.FreeTypeFont:
    names = (
        (
            ["DejaVuSansMono-Bold.ttf", "consolab.ttf"]
            if bold
            else ["DejaVuSansMono.ttf", "consola.ttf"]
        )
        if mono
        else (["DejaVuSans-Bold.ttf", "arialbd.ttf"] if bold else ["DejaVuSans.ttf", "arial.ttf"])
    )
    for name in names:
        try:
            return ImageFont.truetype(name, size=size)
        except OSError:  # noqa: PERF203 - each fallback font is an independent probe
            continue
    raise RuntimeError(f"no bundled TrueType font found for {names}")


DISPLAY = font(66, bold=True)
NUMBER = font(88, mono=True, bold=True)
BODY = font(22)
BODY_BOLD = font(22, bold=True)
MONO = font(17, mono=True)
MONO_SMALL = font(14, mono=True)
LABEL = font(14, mono=True, bold=True)


def load_evidence() -> dict[str, object]:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    if evidence["schema_version"] != 3 or evidence["failure"]["same_failure"] is not True:
        raise RuntimeError("demo evidence is not a verified schema-3 reduction")
    for digest in (
        evidence.get("source_snapshot_sha256"),
        evidence["failure"].get("fingerprint_sha256"),
        evidence["failure"].get("oracle_spec_sha256"),
        evidence["preparation"].get("contract_sha256"),
    ):
        if not isinstance(digest, str) or len(digest) != 64:
            raise RuntimeError("demo evidence contains an invalid integrity digest")
    if evidence["measurements"]["original"]["files"] != 18:
        raise RuntimeError("demo animation contract expects the measured 18-file fixture")
    return evidence


def ease(value: float) -> float:
    return 1.0 - (1.0 - value) ** 3


def retained_count(frame: int, stages: list[int]) -> int:
    position = ease(frame / (FRAME_COUNT - 1)) * (len(stages) - 1)
    left = min(int(position), len(stages) - 1)
    right = min(left + 1, len(stages) - 1)
    blend = position - left
    return round(stages[left] * (1 - blend) + stages[right] * blend)


def draw_grid(draw: ImageDraw.ImageDraw) -> None:
    for x in range(0, SIZE[0], 32):
        draw.line((x, 0, x, SIZE[1]), fill=GRID, width=1)
    for y in range(0, SIZE[1], 32):
        draw.line((0, y, SIZE[0], y), fill=GRID, width=1)


def render_frame(evidence: dict[str, object], index: int) -> Image.Image:
    image = Image.new("RGB", SIZE, PAPER)
    draw = ImageDraw.Draw(image)
    draw_grid(draw)
    progress = index / (FRAME_COUNT - 1)
    stages = evidence["search"]["accepted_file_sizes"]
    current = retained_count(index, stages)
    attempts = evidence["search"]["attempts"]
    observed_attempt = min(attempts, max(1, math.ceil(progress * attempts)))
    fingerprint = evidence["failure"]["fingerprint_sha256"]

    draw.rounded_rectangle((48, 42, 1152, 633), radius=24, fill=WHITE, outline=INK, width=3)
    draw.text((78, 66), "REPRO", font=DISPLAY, fill=INK)
    repro_width = draw.textlength("REPRO", font=DISPLAY)
    draw.text((78 + repro_width - 1, 66), "/CUT", font=DISPLAY, fill=COBALT)
    draw.text((810, 82), "FAILURE REDUCTION RECORD", font=LABEL, fill=MUTED)
    draw.line((78, 151, 1122, 151), fill=INK, width=2)

    draw.text((82, 176), "$ reprocut minimize --root ./checkout", font=MONO, fill=INK)
    draw.text((82, 211), "same failure, less project", font=BODY, fill=MUTED)

    draw.text((78, 258), "18", font=NUMBER, fill=INK)
    draw.text((212, 282), "→", font=font(52, bold=True), fill=RUST)
    draw.text((278, 258), str(current).rjust(2, "0"), font=NUMBER, fill=COBALT)
    draw.text((82, 354), "FILES", font=LABEL, fill=MUTED)
    draw.text((278, 354), "RETAINED", font=LABEL, fill=MUTED)

    cell_x, cell_y = 500, 188
    for item in range(18):
        column, row = item % 6, item // 6
        left = cell_x + column * 98
        top = cell_y + row * 74
        kept = item < current
        final = item < 3
        color = COBALT if final else (INK if kept else GRID)
        fill = "#e8ecfb" if final else ("#f5f3ed" if kept else "#eceae4")
        draw.rounded_rectangle(
            (left, top, left + 76, top + 50), radius=7, fill=fill, outline=color, width=3
        )
        draw.line((left + 14, top + 16, left + 60, top + 16), fill=color, width=2)
        draw.line((left + 14, top + 27, left + 48, top + 27), fill=color, width=2)
        if not kept:
            draw.line((left + 9, top + 9, left + 67, top + 41), fill=RUST, width=2)

    bar_left, bar_top, bar_right = 82, 418, 1120
    draw.rounded_rectangle((bar_left, bar_top, bar_right, bar_top + 14), radius=7, fill="#dfe3e5")
    draw.rounded_rectangle(
        (
            bar_left,
            bar_top,
            bar_left + max(14, int((bar_right - bar_left) * progress)),
            bar_top + 14,
        ),
        radius=7,
        fill=COBALT,
    )
    draw.text(
        (82, 448), f"CANDIDATE {observed_attempt:02d} / {attempts:02d}", font=MONO_SMALL, fill=MUTED
    )
    draw.text((855, 448), "STRICT 3 / 3", font=MONO_SMALL, fill=MUTED)

    draw.line((78, 492, 1122, 492), fill=GRID, width=2)
    kept_files = [entry["path"] for entry in evidence["kept_files"]]
    draw.text((82, 518), "FINAL SNAPSHOT", font=LABEL, fill=MUTED)
    draw.text((82, 547), "  ·  ".join(kept_files), font=MONO, fill=INK)
    draw.text((82, 582), f"sha256:{fingerprint[:20]}…", font=MONO_SMALL, fill=MUTED)

    if index >= FRAME_COUNT - 4:
        draw.rounded_rectangle((850, 526, 1120, 594), radius=10, fill=PROOF)
        draw.text((878, 539), "SAME FAILURE", font=BODY_BOLD, fill=WHITE)
        draw.text((915, 566), "PRESERVED", font=LABEL, fill=WHITE)
    else:
        draw.text((934, 552), "VERIFYING", font=LABEL, fill=RUST)
    return image


def encode(evidence: dict[str, object]) -> None:
    frames = [render_frame(evidence, index) for index in range(FRAME_COUNT)]
    quantized = [frame.quantize(colors=128, method=Image.Quantize.MEDIANCUT) for frame in frames]
    durations = [650] + [95] * (FRAME_COUNT - 2) + [1_200]
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    quantized[0].save(
        OUTPUT,
        format="GIF",
        save_all=True,
        append_images=quantized[1:],
        duration=durations,
        loop=0,
        optimize=True,
        disposal=2,
        comment=f"ReproCut evidence {evidence['failure']['fingerprint_sha256']}".encode(),
    )


def bind_banner(evidence: dict[str, object]) -> None:
    """Atomically keep the static banner bound to the generated failure record."""
    source = BANNER.read_text(encoding="utf-8")
    fingerprint = evidence["failure"]["fingerprint_sha256"]
    bound, replacements = re.subn(
        r"sha256:[0-9a-f]{16}…",
        f"sha256:{fingerprint[:16]}…",
        source,
    )
    if replacements != 1:
        raise RuntimeError(
            f"static banner must contain one evidence fingerprint, found {replacements}"
        )
    BANNER.write_text(bound, encoding="utf-8", newline="\n")


def verify(evidence: dict[str, object]) -> None:
    size = OUTPUT.stat().st_size
    if not 0 < size < MAX_BYTES:
        raise RuntimeError(f"GIF size outside bounded contract: {size} bytes")
    with Image.open(OUTPUT) as animation:
        if animation.format != "GIF" or animation.size != SIZE:
            raise RuntimeError("demo animation format or dimensions changed")
        if animation.n_frames != FRAME_COUNT or animation.info.get("loop") != 0:
            raise RuntimeError("demo animation frame or loop contract changed")
        fingerprint = evidence["failure"]["fingerprint_sha256"].encode()
        if fingerprint not in animation.info.get("comment", b""):
            raise RuntimeError("demo animation is not bound to current evidence")
        print(f"verified GIF: {FRAME_COUNT} frames, {SIZE[0]}x{SIZE[1]}, {size} bytes")


def main() -> int:
    evidence = load_evidence()
    bind_banner(evidence)
    encode(evidence)
    verify(evidence)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"render failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
