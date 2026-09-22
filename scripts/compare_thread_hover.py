#!/usr/bin/env python3
"""Score the native sidebar task hover card against the ChatGPT reference.

The reference geometry comes from the CDP capture report
(`scripts/cdp_capture_thread_hover.mjs`); the native card is located in the
GPUI screenshot by its own edge, so a card that moved is reported as an anchor
delta instead of being silently re-aligned.

Both captures are composited over the same pane color first. Neither app's
offscreen/window capture contains the native macOS material behind the
translucent sidebar, and the two encoders disagree about alpha: Chrome writes
straight alpha, GPUI's Metal readback writes premultiplied alpha. Compositing
removes that difference without touching the card itself, which is opaque.

    python3 scripts/compare_thread_hover.py \
      --reference artifacts/thread-hover/reference \
      --gpui artifacts/thread-hover/gpui \
      --output artifacts/thread-hover/compare
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops


def pane_color(theme: str) -> tuple[int, int, int]:
    """The opaque main pane the card overhangs, from the app's own theme."""
    return (255, 255, 255) if theme == "light" else (0x18, 0x18, 0x18)


def composite(rgba: np.ndarray, base: tuple[int, int, int], premultiplied: bool) -> np.ndarray:
    """Composites an RGBA capture over an opaque backdrop."""
    rgb = rgba[..., :3].astype(np.float64)
    alpha = rgba[..., 3:4].astype(np.float64) / 255.0
    if not premultiplied:
        rgb = rgb * alpha
    background = np.array(base, dtype=np.float64).reshape(1, 1, 3)
    return np.clip(rgb + background * (1.0 - alpha), 0, 255)


def card_top(image: np.ndarray, probe: tuple[int, int]) -> int:
    """Finds the card's first row by the step at its top edge.

    The reference paints a 0.5 px ring *outside* the card, so the strongest
    early step over the card's width is the ring's own row; the card starts on
    the row after it.
    """
    left, right = probe
    column = image[:, left:right].mean(axis=(1, 2))
    for y in range(210, column.size - 2):
        if abs(column[y] - column[y - 1]) > 4.0:
            return y + 1
    raise SystemExit("could not find the native card's top edge")


def load(path: Path, base: tuple[int, int, int], premultiplied: bool) -> np.ndarray:
    with Image.open(path) as image:
        return composite(np.asarray(image.convert("RGBA")), base, premultiplied)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--gpui", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--theme", default="light")
    parser.add_argument("--tolerance", type=int, default=0)
    parser.add_argument("--target", type=float, default=0.99)
    args = parser.parse_args()

    report = json.loads((args.reference / "report.json").read_text())
    geometry = report["themes"][args.theme]["geometry"]
    left, top, width, height = (round(value) for value in geometry["panel"])
    base = pane_color(args.theme)

    reference_image = load(args.reference / f"{args.theme}-window.png", base, premultiplied=False)
    native_image = load(args.gpui / f"{args.theme}-card.png", base, premultiplied=True)

    # The native capture must be the same size and DPR as the reference, or the
    # score silently compares different rasterizations.
    sidecar = args.gpui / f"{args.theme}-card.png.render.json"
    if not sidecar.exists():
        raise SystemExit(f"missing {sidecar}; every capture writes one")
    metadata = json.loads(sidecar.read_text())
    if metadata.get("dpr") != 1.0:
        raise SystemExit(
            f"{sidecar} reports dpr={metadata.get('dpr')}; capture the reference with "
            "`--dpr=1` at the same window size instead"
        )
    expected = (
        int(metadata["viewportWidth"]),
        int(metadata["viewportHeight"]),
    )
    if reference_image.shape[:2] != (expected[1], expected[0]):
        raise SystemExit(
            f"reference is {reference_image.shape[1]}x{reference_image.shape[0]}, "
            f"the native capture is {expected[0]}x{expected[1]}"
        )

    native_top = card_top(native_image, (left + 30, left + width - 30))
    reference_crop = reference_image[top : top + height, left : left + width]
    native_crop = native_image[native_top : native_top + height, left : left + width]
    if native_crop.shape != reference_crop.shape:
        raise SystemExit("the native card runs past the captured window")

    difference = np.abs(reference_crop - native_crop)
    worst = difference.max(axis=2)
    # Same accounting as the other card comparisons in this repository: the
    # headline is the tolerance-adjusted similarity, a per-channel error budget,
    # because two renderers never agree exactly on glyph and path coverage.
    total_pixels = reference_crop.shape[0] * reference_crop.shape[1]
    adjusted_error = np.maximum(difference - args.tolerance, 0).sum()
    max_error = total_pixels * 255 * 3
    report_out = {
        "theme": args.theme,
        "referenceCard": [left, top, width, height],
        "nativeCard": [left, native_top, width, height],
        "verticalAnchorDelta": native_top - top,
        "tolerance": args.tolerance,
        "pixelConsistency": float((worst <= args.tolerance).mean()),
        "exactPixelConsistency": float((worst == 0).mean()),
        "toleranceAdjustedSimilarity": float(1.0 - adjusted_error / max_error),
        "pixelsWithin2": float((worst <= 2).mean()),
        "pixelsWithin12": float((worst <= 12).mean()),
        "meanAbsoluteDifference": float(difference.mean()),
        "maxDifference": int(worst.max()),
    }
    report_out["similarity"] = report_out["toleranceAdjustedSimilarity"]
    report_out["targetMet"] = report_out["similarity"] >= args.target

    args.output.mkdir(parents=True, exist_ok=True)
    reference_png = Image.fromarray(reference_crop.astype(np.uint8))
    native_png = Image.fromarray(native_crop.astype(np.uint8))
    reference_png.save(args.output / f"{args.theme}-reference-card.png")
    native_png.save(args.output / f"{args.theme}-gpui-card.png")
    ImageChops.difference(reference_png, native_png).save(
        args.output / f"{args.theme}-card-diff.png"
    )
    stack = Image.new("RGB", (width, height * 2))
    stack.paste(reference_png, (0, 0))
    stack.paste(native_png, (0, height))
    stack.save(args.output / f"{args.theme}-card-stack.png")
    (args.output / f"{args.theme}-report.json").write_text(
        json.dumps(report_out, indent=1)
    )
    print(json.dumps(report_out, indent=1))
    if not report_out["targetMet"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
