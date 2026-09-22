#!/usr/bin/env python3
"""Compare the native sidebar project hover card with the ChatGPT reference.

The reference geometry comes from the CDP capture report
(`scripts/cdp_capture_project_hover.mjs`); the native geometry is detected from
the screenshot by locating the card itself, so a shifted card is reported as a
position mismatch instead of being silently aligned.

    python3 scripts/compare_project_hover.py \
      --reference artifacts/project-hover/reference \
      --gpui artifacts/project-hover/gpui \
      --output artifacts/project-hover/compare
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops

SCALE = 2


def card_rect_from_reference(report: Path, theme: str) -> tuple[int, int, int, int]:
    data = json.loads((report / "report.json").read_text())
    panel = data["themes"][theme]["geometry"]["panel"]
    return (round(panel[0]), round(panel[1]), round(panel[2]), round(panel[3]))


def detect_card_anchor(image: np.ndarray, probe_x: int = 300, background_x: int = 700) -> tuple[int, int]:
    """Finds the native card's top border and right border against the pane.

    The light theme paints the card on a white pane, so only its border lines
    are visible. The main pane is opaque white behind the card, which is why the
    card's own left edge cannot be separated from the translucent sidebar it
    overlaps; the caller pairs this anchor with the measured card width.
    """
    column = image[:, probe_x * SCALE, :].mean(axis=1)
    background = image[:, background_x * SCALE, :].mean(axis=1)
    difference = np.abs(background - column)
    rows = np.where((difference > 8.0) & (np.arange(difference.size) >= 240))[0]
    if rows.size == 0:
        raise SystemExit("no card border found in the native capture")
    top = int(rows.min())
    # A row near the card's top: the main pane is empty there in both apps, so
    # the only thing that differs from the pane is the card's own border.
    middle = min(top + 20, image.shape[0] - 1)
    horizontal = image[middle, :, :].mean(axis=1)
    columns = np.where(
        (np.abs(horizontal - background[middle]) > 8.0)
        & (np.arange(horizontal.size) >= 400 * SCALE)
    )[0]
    right = int(columns.max()) if columns.size else int(probe_x * SCALE)
    return (round(top / SCALE), round(right / SCALE) + 1)


def divider_offsets(image: np.ndarray, rect: tuple[int, int, int, int]) -> list[float]:
    """Offsets of the card's horizontal section borders, measured from its top.

    Only rows whose ink spans most of the card count: the section borders do,
    while text and icon rows do not.
    """
    x, y, width, height = rect
    band = image[(y + 2) * SCALE : (y + height - 2) * SCALE, (x + 4) * SCALE : (x + width - 4) * SCALE, :]
    rows = band.mean(axis=2)
    base = np.median(rows)
    offsets: list[float] = []
    for index, row in enumerate(rows):
        dark = (np.abs(row - base) > 3.0).mean()
        if dark < 0.6:
            continue
        offset = (index / SCALE) + 2
        if not offsets or offset - offsets[-1] > 3.0:
            offsets.append(offset)
    return [round(offset, 1) for offset in offsets]


def crop(image: Image.Image, rect: tuple[int, int, int, int]) -> Image.Image:
    x, y, width, height = rect
    return image.crop((x * SCALE, y * SCALE, (x + width) * SCALE, (y + height) * SCALE))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--gpui", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--theme", default="light")
    args = parser.parse_args()

    reference_rect = card_rect_from_reference(args.reference, args.theme)
    reference_image = Image.open(args.reference / f"{args.theme}-window.png").convert("RGB")
    native_image = Image.open(args.gpui / f"{args.theme}-card.png").convert("RGB")
    # The reference is captured at DPR 2; a native frame at another scale would
    # be silently compared as if it matched, so require the sidecar to agree.
    sidecar = args.gpui / f"{args.theme}-card.png.render.json"
    if sidecar.exists():
        native_dpr = json.loads(sidecar.read_text()).get("dpr")
        if native_dpr != 2:
            raise SystemExit(
                f"{sidecar} reports dpr={native_dpr}; expected 2 to match the reference"
            )
    else:
        print(f"warning: {sidecar} is missing; cannot verify the native DPR")
    native_top, native_right = detect_card_anchor(np.asarray(native_image))
    native_left = reference_rect[0]
    native_rect = (native_left, native_top, reference_rect[2], reference_rect[3])

    args.output.mkdir(parents=True, exist_ok=True)
    reference_crop = crop(reference_image, reference_rect)
    native_crop = crop(native_image, native_rect)
    reference_crop.save(args.output / f"{args.theme}-reference-card.png")
    native_crop.save(args.output / f"{args.theme}-gpui-card.png")
    if native_crop.size != reference_crop.size:
        native_crop = native_crop.resize(reference_crop.size)
    difference = ImageChops.difference(reference_crop, native_crop)
    difference.save(args.output / f"{args.theme}-card-diff.png")
    stack = Image.new("RGB", (reference_crop.width, reference_crop.height * 2))
    stack.paste(reference_crop, (0, 0))
    stack.paste(native_crop, (0, reference_crop.height))
    stack.save(args.output / f"{args.theme}-card-stack.png")

    left = np.asarray(reference_crop, dtype=np.int16)
    right = np.asarray(native_crop, dtype=np.int16)
    report = {
        "theme": args.theme,
        "referenceCard": reference_rect,
        "nativeCard": native_rect,
        "nativeRightEdge": native_right,
        "referenceRightEdge": reference_rect[0] + reference_rect[2],
        "verticalAnchorDelta": native_top - reference_rect[1],
        "referenceDividerOffsets": divider_offsets(np.asarray(reference_image), reference_rect),
        "nativeDividerOffsets": divider_offsets(np.asarray(native_image), native_rect),
        "meanAbsoluteDifference": float(np.abs(left - right).mean()),
        "pixelsWithinTolerance": float((np.abs(left - right).max(axis=2) <= 12).mean()),
    }
    (args.output / f"{args.theme}-report.json").write_text(json.dumps(report, indent=1))
    print(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
