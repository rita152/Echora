#!/usr/bin/env python3
"""Score the native rename panel against the ChatGPT reference.

The reference geometry comes from the CDP capture report
(`scripts/cdp_capture_thread_rename.mjs`); the native panel is located in the
GPUI screenshot by its own edge, so a panel that moved is reported as an anchor
delta instead of being silently re-aligned.

Both captures are composited over the same pane colour first. Neither app's
offscreen/window capture contains the native macOS material behind the
translucent surfaces, and the two encoders disagree about alpha: Chrome writes
straight alpha, GPUI's Metal readback writes premultiplied alpha. Compositing
removes that difference without touching the panel itself.

    python3 scripts/compare_thread_rename.py \
      --reference artifacts/thread-rename/reference \
      --gpui artifacts/thread-rename/gpui \
      --output artifacts/thread-rename/compare
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops


def pane_color(theme: str) -> tuple[int, int, int]:
    """The opaque main pane the dialog overhangs, from the app's own theme."""
    return (255, 255, 255) if theme == "light" else (0x18, 0x18, 0x18)


def composite(rgba: np.ndarray, base: tuple[int, int, int], premultiplied: bool) -> np.ndarray:
    rgb = rgba[..., :3].astype(np.float64)
    alpha = rgba[..., 3:4].astype(np.float64) / 255.0
    if not premultiplied:
        rgb = rgb * alpha
    background = np.array(base, dtype=np.float64).reshape(1, 1, 3)
    return np.clip(rgb + background * (1.0 - alpha), 0, 255)


def aligned_panel_top(
    reference_image: np.ndarray,
    native_image: np.ndarray,
    rect: tuple[int, int, int, int],
    search: int = 8,
) -> tuple[int, float]:
    """Locates the native panel by the vertical offset that best matches it.

    A single edge is not enough here: the transcript behind the dialog has
    edges of its own (a command card's border sits ~20px above the dialog in
    this capture), and the translucent card lets that transcript show through
    in both apps, so neither an edge nor a flat-surface test isolates the card.
    Both shells centre the dialog in the same window, so the honest measurement
    is the small offset -- bounded by `search` device pixels -- that actually
    aligns the two renders, reported as `verticalAnchorDelta`.
    """
    left, top, width, height = rect
    reference_crop = reference_image[top : top + height, left : left + width]
    best_offset = 0
    best_error = None
    for offset in range(-search, search + 1):
        start = top + offset
        if start < 0 or start + height > native_image.shape[0]:
            continue
        candidate = native_image[start : start + height, left : left + width]
        error = float(np.abs(reference_crop - candidate).mean())
        if best_error is None or error < best_error:
            best_error = error
            best_offset = offset
    return top + best_offset, best_error or 0.0


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
    base = pane_color(args.theme)

    sidecar = args.gpui / f"{args.theme}-panel.png.render.json"
    if not sidecar.exists():
        raise SystemExit(f"missing {sidecar}; every capture writes one")
    metadata = json.loads(sidecar.read_text())
    dpr = float(metadata.get("dpr") or 0.0)
    if dpr <= 0.0:
        raise SystemExit(f"{sidecar} does not report a device pixel ratio")
    # The reference has to render at the native capture's device pixel ratio;
    # comparing a 2x build against a 1x reference reports font rasterization as
    # a geometry mismatch.
    reference_dpr = float(geometry.get("viewport", [0, 0, 0])[2] or 0.0)
    if abs(reference_dpr - dpr) > 0.01:
        raise SystemExit(
            f"reference was captured at dpr={reference_dpr}, the native capture at {dpr}; "
            "re-capture the reference with the matching `--dpr`"
        )
    expected = (
        int(metadata["viewportWidth"]),
        int(metadata["viewportHeight"]),
    )
    left, top, width, height = (round(value) for value in geometry["panel"])
    reference_image = load(args.reference / f"{args.theme}-window.png", base, premultiplied=False)
    native_image = load(args.gpui / f"{args.theme}-panel.png", base, premultiplied=True)
    if reference_image.shape[:2] != (int(expected[1] * dpr), int(expected[0] * dpr)):
        raise SystemExit(
            f"reference is {reference_image.shape[1]}x{reference_image.shape[0]}, "
            f"the native capture is {expected[0]}x{expected[1]}@{dpr}"
        )

    # Device-pixel geometry for both sides.
    left = round(left * dpr)
    top = round(top * dpr)
    width = round(width * dpr)
    height = round(height * dpr)

    reference_crop = reference_image[top : top + height, left : left + width]
    native_top, _ = aligned_panel_top(
        reference_image,
        native_image,
        (left, top, width, height),
    )
    native_crop = native_image[native_top : native_top + height, left : left + width]
    if native_crop.shape != reference_crop.shape:
        raise SystemExit("the native panel runs past the captured window")

    difference = np.abs(reference_crop - native_crop)
    worst = difference.max(axis=2)
    total_pixels = reference_crop.shape[0] * reference_crop.shape[1]
    adjusted_error = np.maximum(difference - args.tolerance, 0).sum()
    max_error = total_pixels * 255 * 3
    report_out = {
        "theme": args.theme,
        "referencePanel": [left, top, width, height],
        "nativePanel": [left, native_top, width, height],
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
    reference_png.save(args.output / f"{args.theme}-reference-panel.png")
    native_png.save(args.output / f"{args.theme}-gpui-panel.png")
    ImageChops.difference(reference_png, native_png).save(
        args.output / f"{args.theme}-panel-diff.png"
    )
    stack = Image.new("RGB", (width, height * 2))
    stack.paste(reference_png, (0, 0))
    stack.paste(native_png, (0, height))
    stack.save(args.output / f"{args.theme}-panel-stack.png")
    (args.output / f"{args.theme}-report.json").write_text(
        json.dumps(report_out, indent=1)
    )
    print(json.dumps(report_out, indent=1))
    if not report_out["targetMet"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
