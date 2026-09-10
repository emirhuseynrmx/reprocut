#!/usr/bin/env python3
"""Render ReproCut launch assets from the checked-in verified demo evidence."""

from __future__ import annotations

import json
import math
import re
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont, PngImagePlugin
from release.schema_versions import EVIDENCE_SCHEMA

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / "demo" / "result" / "reduction.json"
GIF_OUTPUT = ROOT / "assets" / "reprocut-demo.gif"
LAUNCH_OUTPUT = ROOT / "assets" / "reprocut-launch.png"
BANNER = ROOT / "assets" / "reprocut-banner.svg"
GIF_SIZE = (800, 450)
LAUNCH_SIZE = (1920, 1080)
FRAME_COUNT = 24
MAX_GIF_BYTES = 8 * 1024 * 1024

NAVY = "#050814"
NAVY_2 = "#0B1024"
PANEL = "#111730"
PANEL_2 = "#151B37"
LINE = "#35406B"
MUTED = "#8D99BE"
WHITE = "#F7F8FF"
VIOLET = "#8268FF"
LILAC = "#B79DFF"
BLUE = "#6EA8FF"
ORANGE = "#FF876B"
GREEN = "#82E8BE"


def font(size: int, *, mono: bool = False, bold: bool = False) -> ImageFont.FreeTypeFont:
    """Resolve a predictable local font without adding a font artifact to the release."""
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
        except OSError:  # noqa: PERF203 - each fallback is an independent platform probe
            continue
    raise RuntimeError(f"no usable TrueType font found for {names}")


def load_evidence() -> dict[str, object]:
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    if evidence["schema_version"] != EVIDENCE_SCHEMA:
        raise RuntimeError("demo evidence schema does not match the release contract")
    if evidence["failure"]["same_failure"] is not True:
        raise RuntimeError("demo evidence does not preserve the stabilized failure")
    if evidence["measurements"]["original"]["files"] != 18:
        raise RuntimeError("launch assets require the checked-in 18-file fixture")
    if evidence["measurements"]["retained"]["files"] != 3:
        raise RuntimeError("launch assets require the checked-in three-file result")
    for digest in (
        evidence.get("source_snapshot_sha256"),
        evidence["failure"].get("fingerprint_sha256"),
        evidence["failure"].get("oracle_spec_sha256"),
        evidence["preparation"].get("contract_sha256"),
    ):
        if not isinstance(digest, str) or len(digest) != 64:
            raise RuntimeError("demo evidence contains an invalid integrity digest")
    return evidence


def backdrop(size: tuple[int, int]) -> Image.Image:
    """Create the shared navy backdrop and restrained technical grid."""
    width, height = size
    image = Image.new("RGB", size, NAVY)
    pixels = image.load()
    for y in range(height):
        ratio = y / max(1, height - 1)
        for x in range(width):
            side = x / max(1, width - 1)
            pixels[x, y] = (
                round(5 + 13 * ratio + 3 * side),
                round(8 + 7 * ratio),
                round(20 + 25 * ratio + 9 * side),
            )
    draw = ImageDraw.Draw(image)
    spacing = max(36, width // 28)
    for x in range(0, width, spacing):
        draw.line((x, 0, x, height), fill="#101934", width=1)
    for y in range(0, height, spacing):
        draw.line((0, y, width, y), fill="#101934", width=1)
    return image


def add_glow(
    image: Image.Image,
    center: tuple[int, int],
    radius: int,
    color: tuple[int, int, int],
    strength: int,
) -> None:
    layer = Image.new("RGBA", image.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    x, y = center
    draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=(*color, strength))
    layer = layer.filter(ImageFilter.GaussianBlur(radius // 2))
    image.paste(layer, (0, 0), layer)


def draw_mark(draw: ImageDraw.ImageDraw, center: tuple[int, int], scale: float = 1.0) -> None:
    """Draw the ReproCut package mark as three crisp isometric faces."""
    cx, cy = center
    top = [
        (cx, cy - 68 * scale),
        (cx + 64 * scale, cy - 32 * scale),
        (cx, cy + 4 * scale),
        (cx - 64 * scale, cy - 32 * scale),
    ]
    left = [
        (cx - 64 * scale, cy - 24 * scale),
        (cx - 8 * scale, cy + 8 * scale),
        (cx - 8 * scale, cy + 79 * scale),
        (cx - 64 * scale, cy + 45 * scale),
    ]
    right = [
        (cx, cy + 8 * scale),
        (cx + 64 * scale, cy - 28 * scale),
        (cx + 64 * scale, cy + 44 * scale),
        (cx, cy + 79 * scale),
    ]
    draw.polygon(top, fill=BLUE, outline=LILAC)
    draw.polygon(left, fill=VIOLET, outline=LILAC)
    draw.polygon(right, fill=ORANGE, outline="#FFB095")
    draw.line(
        (cx - 64 * scale, cy - 24 * scale, cx, cy + 12 * scale, cx + 64 * scale, cy - 24 * scale),
        fill=NAVY,
        width=max(2, round(7 * scale)),
    )
    draw.line(
        (cx - 8 * scale, cy + 8 * scale, cx - 8 * scale, cy + 79 * scale),
        fill=NAVY,
        width=max(2, round(7 * scale)),
    )


def rounded_panel(
    draw: ImageDraw.ImageDraw, box: tuple[int, int, int, int], radius: int = 20
) -> None:
    draw.rounded_rectangle(box, radius=radius, fill=PANEL, outline=LINE, width=2)


def draw_file_grid(
    draw: ImageDraw.ImageDraw,
    origin: tuple[int, int],
    retained: int,
    *,
    cell: int,
    gap: int,
) -> None:
    left, top = origin
    for item in range(18):
        row, column = divmod(item, 6)
        x = left + column * (cell + gap)
        y = top + row * (cell + gap)
        kept = item < retained
        final = item < 3
        fill = "#1B2142" if kept else "#0B1021"
        outline = VIOLET if final else ("#5D6898" if kept else "#2B3150")
        draw.rounded_rectangle(
            (x, y, x + cell, y + cell), radius=7, fill=fill, outline=outline, width=2
        )
        draw.line((x + 9, y + 12, x + cell - 9, y + 12), fill=outline, width=2)
        draw.line((x + 9, y + 21, x + cell - 18, y + 21), fill=outline, width=2)
        if not kept:
            draw.line((x + 7, y + 7, x + cell - 7, y + cell - 7), fill=ORANGE, width=2)
        elif final:
            draw.ellipse((x + cell - 13, y + cell - 13, x + cell - 7, y + cell - 7), fill=GREEN)


def render_launch(evidence: dict[str, object]) -> None:
    image = backdrop(LAUNCH_SIZE)
    add_glow(image, (1390, 340), 380, (111, 74, 255), 100)
    add_glow(image, (1260, 850), 280, (255, 108, 72), 70)
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((28, 28, 1892, 1052), radius=32, outline=LINE, width=2)

    draw.text(
        (110, 105), "OPEN SOURCE FAILURE REDUCTION", font=font(20, mono=True, bold=True), fill=MUTED
    )
    draw.text((110, 170), "repro", font=font(104, bold=True), fill=WHITE)
    repro_width = draw.textlength("repro", font=font(104, bold=True))
    draw.text((110 + repro_width, 170), "cut", font=font(104, bold=True), fill=LILAC)
    draw.rounded_rectangle((112, 300, 320, 344), radius=22, fill="#11172F", outline="#6959C9")
    draw.text((133, 312), "0.1.0-alpha.1", font=font(17, mono=True, bold=True), fill="#CBC7FF")

    draw.text((110, 425), "Same failure.", font=font(68, bold=True), fill=WHITE)
    draw.text((110, 505), "Less project.", font=font(68, bold=True), fill=LILAC)
    draw.text(
        (114, 610),
        "Diagnose  →  Reduce  →  Verify",
        font=font(25, mono=True, bold=True),
        fill="#D8DBEF",
    )
    draw.text(
        (114, 665),
        "A smaller reproduction another engineer can rerun",
        font=font(25),
        fill="#A9B1CE",
    )
    draw.text(
        (114, 701), "without trusting the reduction that created it.", font=font(25), fill="#A9B1CE"
    )

    panel = (880, 118, 1790, 860)
    rounded_panel(draw, panel, 28)
    draw.text((930, 160), "VERIFIED REDUCTION", font=font(18, mono=True, bold=True), fill=MUTED)
    draw.text((1540, 160), "RC / 0.1", font=font(17, mono=True, bold=True), fill=LILAC)
    draw.line((930, 205, 1740, 205), fill=LINE, width=2)

    add_glow(image, (1335, 355), 150, (117, 86, 255), 85)
    draw = ImageDraw.Draw(image)
    draw_mark(draw, (1335, 345), 1.45)
    draw.text((1000, 520), "18", font=font(88, mono=True, bold=True), fill=WHITE)
    draw.text((1165, 542), "→", font=font(52, bold=True), fill=ORANGE)
    draw.text((1285, 520), "03", font=font(88, mono=True, bold=True), fill=LILAC)
    draw.text((1005, 615), "FILES", font=font(17, mono=True, bold=True), fill=MUTED)
    draw.text((1292, 615), "RETAINED", font=font(17, mono=True, bold=True), fill=MUTED)

    draw.rounded_rectangle((942, 675, 1718, 796), radius=16, fill="#0A1022", outline="#34416B")
    draw.ellipse((974, 706, 992, 724), fill=GREEN)
    draw.text((1014, 698), "STABLE FAILURE", font=font(15, mono=True, bold=True), fill=WHITE)
    draw.text(
        (1014, 725),
        "24 candidates · strict 3 / 3 verification",
        font=font(17, mono=True),
        fill=MUTED,
    )
    fingerprint = evidence["failure"]["fingerprint_sha256"]
    draw.text((1014, 754), f"sha256:{fingerprint[:20]}…", font=font(15, mono=True), fill="#7F89AD")

    draw.rounded_rectangle((110, 875, 1790, 965), radius=18, fill="#0B1021", outline="#30395E")
    facts = ["WHOLE PROJECT", "8 LANGUAGES", "CRASH-SAFE RESUME", "VERIFIABLE ARTIFACT"]
    positions = [150, 525, 900, 1275]
    for x, fact in zip(positions, facts, strict=True):
        draw.ellipse((x, 916, x + 12, 928), fill=GREEN)
        draw.text((x + 28, 907), fact, font=font(17, mono=True, bold=True), fill="#CFD4EA")

    metadata = PngImagePlugin.PngInfo()
    metadata.add_text("reprocut_failure_sha256", fingerprint)
    metadata.add_text("reprocut_version", "0.1.0-alpha.1")
    LAUNCH_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    image.save(LAUNCH_OUTPUT, format="PNG", optimize=True, pnginfo=metadata)


def ease(value: float) -> float:
    return 1.0 - (1.0 - value) ** 3


def retained_count(frame: int, stages: list[int]) -> int:
    position = ease(frame / 10) * (len(stages) - 1)
    left = min(int(position), len(stages) - 1)
    right = min(left + 1, len(stages) - 1)
    blend = position - left
    return round(stages[left] * (1 - blend) + stages[right] * blend)


def gif_header(draw: ImageDraw.ImageDraw, stage: str, active: int) -> None:
    draw.text((34, 26), "repro", font=font(30, bold=True), fill=WHITE)
    width = draw.textlength("repro", font=font(30, bold=True))
    draw.text((34 + width, 26), "cut", font=font(30, bold=True), fill=LILAC)
    draw.text((645, 35), "0.1.0-alpha.1", font=font(11, mono=True, bold=True), fill=MUTED)
    draw.line((34, 70, 766, 70), fill=LINE, width=1)
    labels = ["DIAGNOSE", "REDUCE", "VERIFY"]
    xs = [40, 300, 550]
    for index, (x, label) in enumerate(zip(xs, labels, strict=True)):
        color = WHITE if index == active else "#626E94"
        draw.ellipse((x, 91, x + 10, 101), fill=GREEN if index < active else color)
        draw.text((x + 22, 85), label, font=font(13, mono=True, bold=True), fill=color)
        if index < 2:
            draw.line((x + 118, 96, xs[index + 1] - 14, 96), fill="#394365", width=2)
    draw.text((34, 417), stage, font=font(11, mono=True, bold=True), fill="#737FA5")


def render_frame(evidence: dict[str, object], index: int) -> Image.Image:
    image = backdrop(GIF_SIZE)
    draw = ImageDraw.Draw(image)
    fingerprint = evidence["failure"]["fingerprint_sha256"]

    if index <= 4:
        gif_header(draw, "01 / PREFLIGHT", 0)
        rounded_panel(draw, (35, 128, 765, 386), 18)
        draw.text(
            (62, 153),
            "$ reprocut doctor --root ./failing-project -- python bug.py",
            font=font(14, mono=True),
            fill="#CDD2E6",
        )
        draw.line((62, 187, 738, 187), fill="#30395D")
        completed = min(3, index + 1)
        for run in range(3):
            y = 218 + run * 43
            ready = run < completed
            draw.ellipse((64, y + 4, 76, y + 16), fill=GREEN if ready else "#30395D")
            draw.text(
                (92, y),
                f"baseline run {run + 1}",
                font=font(15, mono=True),
                fill=WHITE if ready else MUTED,
            )
            draw.text(
                (590, y),
                "exit 1" if ready else "waiting",
                font=font(14, mono=True),
                fill=GREEN if ready else MUTED,
            )
        if completed == 3:
            draw.text(
                (62, 352),
                "ready: stable TypeError: currency",
                font=font(15, mono=True, bold=True),
                fill=GREEN,
            )
    elif index <= 15:
        gif_header(draw, "02 / REDUCTION", 1)
        rounded_panel(draw, (35, 128, 765, 386), 18)
        progress = index - 5
        stages = evidence["search"]["accepted_file_sizes"]
        current = retained_count(progress, stages)
        draw.text((62, 152), "PROJECT MATERIAL", font=font(13, mono=True, bold=True), fill=MUTED)
        draw.text(
            (570, 149), f"18 → {current:02d} files", font=font(18, mono=True, bold=True), fill=LILAC
        )
        draw_file_grid(draw, (62, 198), current, cell=42, gap=13)
        attempts = evidence["search"]["attempts"]
        observed = min(attempts, max(1, math.ceil((progress + 1) / 11 * attempts)))
        draw.text(
            (62, 356),
            f"candidate {observed:02d} / {attempts:02d}",
            font=font(13, mono=True),
            fill=MUTED,
        )
        draw.text((578, 356), "same failure", font=font(13, mono=True, bold=True), fill=GREEN)
    elif index <= 20:
        gif_header(draw, "03 / FINAL VERIFICATION", 2)
        rounded_panel(draw, (35, 128, 765, 386), 18)
        draw.text(
            (62, 153),
            "THE REDUCED PROJECT MUST FAIL THE SAME WAY",
            font=font(14, mono=True, bold=True),
            fill=MUTED,
        )
        completed = min(3, index - 15)
        for run in range(3):
            x = 62 + run * 221
            ready = run < completed
            draw.rounded_rectangle(
                (x, 215, x + 190, 305),
                radius=14,
                fill="#0B1022",
                outline=GREEN if ready else LINE,
                width=2,
            )
            draw.text(
                (x + 18, 235), f"RUN {run + 1}", font=font(14, mono=True, bold=True), fill=MUTED
            )
            draw.text(
                (x + 18, 268),
                "PRESERVED" if ready else "WAITING",
                font=font(18, mono=True, bold=True),
                fill=GREEN if ready else MUTED,
            )
        if completed == 3:
            draw.text(
                (62, 342),
                "strict 3 / 3 · no new diagnostic line",
                font=font(14, mono=True, bold=True),
                fill=GREEN,
            )
    else:
        gif_header(draw, "04 / VERIFIED ARTIFACT", 2)
        rounded_panel(draw, (35, 128, 765, 386), 18)
        add_glow(image, (173, 255), 85, (117, 86, 255), 95)
        draw = ImageDraw.Draw(image)
        draw_mark(draw, (173, 245), 0.65)
        draw.text((288, 165), "SAME FAILURE.", font=font(28, bold=True), fill=WHITE)
        draw.text((288, 202), "LESS PROJECT.", font=font(28, bold=True), fill=LILAC)
        draw.text(
            (288, 261),
            "18 files  →  3 retained",
            font=font(17, mono=True, bold=True),
            fill="#D8DBEF",
        )
        draw.text(
            (288, 296), "report · ledger · reproduce scripts", font=font(14, mono=True), fill=MUTED
        )
        draw.text((288, 330), f"sha256:{fingerprint[:18]}…", font=font(13, mono=True), fill=GREEN)
    progress_right = 34 + round(732 * (index + 1) / FRAME_COUNT)
    draw.line((34, 400, 766, 400), fill="#202946", width=2)
    draw.line((34, 400, progress_right, 400), fill=VIOLET, width=2)
    return image


def encode_gif(evidence: dict[str, object]) -> None:
    frames = [render_frame(evidence, index) for index in range(FRAME_COUNT)]
    quantized = [frame.quantize(colors=128, method=Image.Quantize.MEDIANCUT) for frame in frames]
    durations = (
        [650] + [180] * 3 + [900] + [110] * 10 + [700] + [300] * 4 + [950] + [350, 350, 1600]
    )
    quantized[0].save(
        GIF_OUTPUT,
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
    source = BANNER.read_text(encoding="utf-8")
    fingerprint = evidence["failure"]["fingerprint_sha256"]
    bound, replacements = re.subn(
        r"sha256:[0-9a-f]{16}…",
        f"sha256:{fingerprint[:16]}…",
        source,
    )
    if replacements != 1:
        raise RuntimeError(f"banner must contain one evidence fingerprint, found {replacements}")
    BANNER.write_text(bound, encoding="utf-8", newline="\n")


def verify(evidence: dict[str, object]) -> None:
    size = GIF_OUTPUT.stat().st_size
    if not 0 < size < MAX_GIF_BYTES:
        raise RuntimeError(f"GIF size outside bounded contract: {size} bytes")
    with Image.open(GIF_OUTPUT) as animation:
        if animation.format != "GIF" or animation.size != GIF_SIZE:
            raise RuntimeError("demo animation format or dimensions changed")
        if animation.n_frames != FRAME_COUNT or animation.info.get("loop") != 0:
            raise RuntimeError("demo animation frame or loop contract changed")
        fingerprint = evidence["failure"]["fingerprint_sha256"].encode()
        if fingerprint not in animation.info.get("comment", b""):
            raise RuntimeError("demo animation is not bound to current evidence")
    with Image.open(LAUNCH_OUTPUT) as launch:
        if launch.format != "PNG" or launch.size != LAUNCH_SIZE:
            raise RuntimeError("launch image format or dimensions changed")
        if launch.info.get("reprocut_failure_sha256") != evidence["failure"]["fingerprint_sha256"]:
            raise RuntimeError("launch image is not bound to current evidence")
    print(f"verified GIF: {FRAME_COUNT} frames, {GIF_SIZE[0]}x{GIF_SIZE[1]}, {size} bytes")
    print(f"verified launch image: {LAUNCH_SIZE[0]}x{LAUNCH_SIZE[1]}")


def main() -> int:
    evidence = load_evidence()
    bind_banner(evidence)
    render_launch(evidence)
    encode_gif(evidence)
    verify(evidence)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"render failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
