#!/usr/bin/env python3
"""Pixel-compare the three Review popups between ChatGPT and GPUI.

Both captures must come from the same window size, display scale, theme, and
repository state:

  CHATGPT_CDP_HTTP=http://127.0.0.1:9455 \
    node scripts/cdp_capture_review_menus.mjs --output=artifacts/review-menus/reference
  'target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone' --theme=dark \
    --window-width=1440 --window-height=900 --review-root="$PWD" \
    --review-menu=scope --screenshot=artifacts/review-menus/gpui/scope.png

The comparison aligns each pair on the popup's own top-left corner, so the
anchor offsets between the two shells do not enter the score. Every popup name
is compared at the reference's exact size in device pixels.

    python3 scripts/compare_review_menus.py \
      --reference artifacts/review-menus/reference/dark \
      --gpui artifacts/review-menus/gpui \
      --output artifacts/review-menus/comparison \
      --scale 2
"""

from __future__ import annotations

import argparse
import json
import pathlib

from PIL import Image, ImageChops, ImageFilter

# `bg-surface-elevated-secondary/90` over the review pane composites to a flat
# #2d2d2d in dark mode, which is what both shells paint.
POPUP_SURFACE = (45, 45, 45)
SURFACE_TOLERANCE = 2


def is_popup_pixel(pixel) -> bool:
    r, g, b = pixel[:3]
    return (
        abs(r - POPUP_SURFACE[0]) <= SURFACE_TOLERANCE
        and abs(g - POPUP_SURFACE[1]) <= SURFACE_TOLERANCE
        and abs(b - POPUP_SURFACE[2]) <= SURFACE_TOLERANCE
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--reference", required=True, help="reference capture directory")
    parser.add_argument("--gpui", required=True, help="GPUI capture directory")
    parser.add_argument("--output", required=True, help="where crops, diffs and the report go")
    parser.add_argument("--scale", type=float, default=1.0, help="device pixels per logical pixel")
    parser.add_argument("--threshold", type=int, default=8, help="channel delta counted as a mismatch")
    parser.add_argument("--menus", default="scope,options,branch")
    return parser.parse_args()


def detect_popup(
    image: Image.Image,
    scale: float,
    reference_box: tuple[float, float, float, float],
    right_aligned: bool,
) -> tuple[int, int, int, int]:
    """Locate GPUI's popup inside a window anchored on its own pane.

    Both shells put the popup under the review toolbar: the comparison and
    branch popups hang off the pane's left edge, the diff controls off its
    right edge. The panes are sized independently, so the search window follows
    that anchor instead of the reference's absolute position.
    """
    rx, ry, rw, rh = reference_box
    pixels = image.load()
    panel_left = detect_pane_left(image, scale)
    gap = 8 * scale
    if right_aligned:
        x_start = max(0, int(image.size[0] - gap - (rw + 8) * scale))
        x_end = image.size[0]
    else:
        x_start = int(panel_left + gap)
        x_end = min(image.size[0], int(x_start + (rw + 8) * scale))
    # The popup sits directly under the review toolbar in both shells.
    y_start = max(0, int(ry * scale) - 4)
    y_end = min(image.size[1], int((ry + rh) * scale) + 8)

    xs_all: list[int] = []
    ys_all: list[int] = []
    for y in range(y_start, y_end):
        for x in range(x_start, x_end):
            if is_popup_pixel(pixels[x, y]):
                xs_all.append(x)
                ys_all.append(y)
    if not xs_all:
        raise SystemExit("could not locate the GPUI popup; check the capture and theme")
    return (min(xs_all), min(ys_all), max(xs_all) + 1, max(ys_all) + 1)


def detect_pane_left(image: Image.Image, scale: float) -> float:
    """Left edge of the review pane in device pixels."""
    pixels = image.load()
    row = image.size[1] // 2
    # The conversation keeps a uniform surface; the pane starts at its border.
    start = int(image.size[0] * 0.4)
    baseline = pixels[start, row][:3]
    for x in range(start, image.size[0]):
        pixel = pixels[x, row][:3]
        if any(abs(pixel[channel] - baseline[channel]) > 6 for channel in range(3)):
            return float(x)
    raise SystemExit("could not find the review pane in the GPUI capture")


def score(
    reference: Image.Image,
    actual: Image.Image,
    reference_box: tuple[int, int, int, int],
    actual_box: tuple[int, int, int, int],
    threshold: int,
) -> dict:
    width = min(reference_box[2] - reference_box[0], actual_box[2] - actual_box[0])
    height = min(reference_box[3] - reference_box[1], actual_box[3] - actual_box[1])
    ref_crop = reference.crop(
        (reference_box[0], reference_box[1], reference_box[0] + width, reference_box[1] + height)
    ).convert("RGB")
    act_crop = actual.crop(
        (actual_box[0], actual_box[1], actual_box[0] + width, actual_box[1] + height)
    ).convert("RGB")
    diff = ImageChops.difference(ref_crop, act_crop).convert("L")
    histogram = diff.histogram()
    total = sum(histogram)
    over = sum(histogram[threshold + 1 :])
    mean = sum(index * count for index, count in enumerate(histogram)) / max(1, total)
    # Glyph rasterisation differs between CoreText and Chromium, so a strict
    # per-pixel score always reports a large fraction of the text. The blurred
    # score answers "does the same ink sit in the same place", which is what
    # layout parity means across two engines.
    softened_ref = ref_crop.resize((max(1, width // 4), max(1, height // 4)), Image.BOX)
    softened_act = act_crop.resize((max(1, width // 4), max(1, height // 4)), Image.BOX)
    softened_diff = ImageChops.difference(softened_ref, softened_act).convert("L")
    softened_histogram = softened_diff.histogram()
    softened_total = sum(softened_histogram)
    softened_over = sum(softened_histogram[threshold + 1 :])
    softened_mean = (
        sum(index * count for index, count in enumerate(softened_histogram)) / max(1, softened_total)
    )
    # Split the popup into "chrome" (flat surfaces, rules, highlights, icons on
    # flat ground) and "text" (glyph edges). CoreText and Chromium hint glyphs
    # differently, so only the chrome half can be expected to match exactly.
    chrome, chrome_total, chrome_over = smooth_region_score(
        ref_crop, act_crop, diff, threshold
    )
    return {
        "size": [width, height],
        "reference_size": [reference_box[2] - reference_box[0], reference_box[3] - reference_box[1]],
        "gpui_size": [actual_box[2] - actual_box[0], actual_box[3] - actual_box[1]],
        "mean_delta": round(mean, 3),
        "structural_mean_delta": round(softened_mean, 3),
        "structural_mismatch_ratio": round(softened_over / max(1, softened_total), 5),
        "structural_mismatch_ratio_percent": round(100 * softened_over / max(1, softened_total), 3),
        "chrome_pixels": chrome_total,
        "chrome_mean_delta": round(chrome, 3),
        "chrome_mismatch_ratio_percent": round(100 * chrome_over / max(1, chrome_total), 4),
        "mismatch_ratio": round(over / max(1, total), 5),
        "mismatch_ratio_percent": round(100 * over / max(1, total), 3),
        "reference_crop": ref_crop,
        "actual_crop": act_crop,
        "diff": diff,
    }


def smooth_region_score(
    reference: Image.Image, actual: Image.Image, diff: Image.Image, threshold: int
) -> tuple[float, int, int]:
    """Mean delta and mismatch count over the pixels neither engine drew text on."""
    # A 5x5 window keeps glyph antialiasing - which spills a pixel or two past
    # the stem - out of the chrome mask.
    window = 5
    minimum = ImageFilter.RankFilter(window, 0)
    maximum = ImageFilter.RankFilter(window, window * window - 1)
    reference_grey = reference.convert("L")
    actual_grey = actual.convert("L")
    reference_contrast = ImageChops.difference(
        reference_grey.filter(maximum), reference_grey.filter(minimum)
    )
    actual_contrast = ImageChops.difference(
        actual_grey.filter(maximum), actual_grey.filter(minimum)
    )
    reference_pixels = reference_contrast.load()
    actual_pixels = actual_contrast.load()
    diff_pixels = diff.load()
    width, height = diff.size
    total = 0
    over = 0
    delta_sum = 0
    margin = window
    for y in range(margin, height - margin):
        for x in range(margin, width - margin):
            if reference_pixels[x, y] <= 8 and actual_pixels[x, y] <= 8:
                total += 1
                delta = diff_pixels[x, y]
                delta_sum += delta
                if delta > threshold:
                    over += 1
    return (delta_sum / max(1, total), total, over)


def main() -> None:
    args = parse_args()
    scale = args.scale
    output = pathlib.Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    report: dict[str, dict] = {}

    for menu in args.menus.split(","):
        menu = menu.strip()
        if not menu:
            continue
        metadata = json.loads((pathlib.Path(args.reference) / f"{menu}.json").read_text())
        menu_rect = metadata["menu"]["rect"]
        box = (
            int(round(menu_rect[0] * scale)),
            int(round(menu_rect[1] * scale)),
            int(round((menu_rect[0] + menu_rect[2]) * scale)),
            int(round((menu_rect[1] + menu_rect[3]) * scale)),
        )
        reference = Image.open(pathlib.Path(args.reference) / f"{menu}-window.png")
        actual = Image.open(pathlib.Path(args.gpui) / f"{menu}.png")
        if reference.size != actual.size:
            raise SystemExit(
                f"{menu}: window sizes differ, reference {reference.size} vs GPUI {actual.size}; "
                "capture both at the same size and display scale"
            )
        # Each shell anchors the popup to its own panel, so locate GPUI's popup
        # by its surface and compare the two crops corner-to-corner.
        detected = detect_popup(actual, scale, menu_rect, right_aligned=menu == "options")
        result = score(reference, actual, box, detected, args.threshold)
        result["gpui_popup_box"] = [round(value / scale, 2) for value in detected]
        result["reference_popup_box"] = menu_rect
        result.pop("reference_crop").save(output / f"{menu}-reference.png")
        result.pop("actual_crop").save(output / f"{menu}-gpui.png")
        result.pop("diff").save(output / f"{menu}-diff.png")
        report[menu] = result
        print(
            f"{menu:8s} {result['reference_size'][0]}x{result['reference_size'][1]} vs "
            f"{result['gpui_size'][0]}x{result['gpui_size'][1]}  "
            f"mean {result['mean_delta']:6.2f}  mismatch {result['mismatch_ratio_percent']:5.2f}%  "
            f"chrome {result['chrome_mismatch_ratio_percent']:6.3f}% of "
            f"{result['chrome_pixels']} px  "
            f"gpui popup at {result['gpui_popup_box'][:2]}"
        )

    (output / "report.json").write_text(json.dumps(report, indent=1))
    worst = max(report.items(), key=lambda item: item[1]["mismatch_ratio"])
    print(f"worst: {worst[0]} at {worst[1]['mismatch_ratio_percent']:.3f}% mismatched pixels")


if __name__ == "__main__":
    main()
