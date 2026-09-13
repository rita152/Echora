#!/usr/bin/env python3
"""Region comparison for the MCP elicitation card.

Compares the ChatGPT reference card region with the same region rendered by
the native GPUI card. Images are never scaled, translated, or cropped to a
different aspect ratio: the actual card is located by its own outline and both
regions are compared pixel by pixel over their overlapping area, starting at
each card's top-left corner.

Usage:
  python3 scripts/compare_mcp_elicitation_pixels.py +    --reference artifacts/.../raw/05-form-card.png +    --reference-rect 410,122,736,682 +    --actual artifacts/.../gpui/gpui-form-default-light.png +    --label form-default-light +    --out-dir artifacts/.../compare
"""

from __future__ import annotations

import argparse
import json
import os

from PIL import Image, ImageChops


def detect_card_rect(image: Image.Image, search_x0: int = 260) -> tuple[int, int, int, int]:
    """Locate the card by its 1px outline inside the right pane."""
    pixels = image.convert("RGB").load()
    width, height = image.size
    rows = []
    for y in range(80, height):
        run_start = None
        longest = 0
        best = None
        for x in range(search_x0, width):
            r, g, b = pixels[x, y]
            # The outline is a light neutral ring over white: darker than the
            # page but far lighter than any text or control.
            is_ring = 225 <= r <= 250 and 225 <= g <= 250 and 225 <= b <= 250
            if is_ring:
                if run_start is None:
                    run_start = x
                run_length = x - run_start + 1
                if run_length > longest:
                    longest = run_length
                    best = (run_start, run_length)
            else:
                run_start = None
        if longest >= 600:
            rows.append((y, best))
    if not rows:
        raise SystemExit("could not locate the card outline in the actual capture")
    # The top border row spans the full card width: nothing inside the card can
    # interrupt it the way body content can.
    top, (x0, run) = rows[0]
    bottom = top
    for y, (row_x, row_run) in rows:
        if abs(row_x - x0) <= 3 and row_run >= run * 0.8:
            bottom = y
    return (x0, top, run, bottom - top + 1)


def compare(
    reference: Image.Image,
    actual: Image.Image,
    reference_rect: tuple[int, int, int, int],
    actual_rect: tuple[int, int, int, int],
    tolerance: int,
) -> dict:
    rx, ry, rw, rh = reference_rect
    ax, ay, aw, ah = actual_rect
    width = min(rw, aw)
    height = min(rh, ah)
    reference_crop = reference.convert("RGB").crop((rx, ry, rx + width, ry + height))
    actual_crop = actual.convert("RGB").crop((ax, ay, ax + width, ay + height))
    diff = ImageChops.difference(reference_crop, actual_crop)
    histogram = diff.convert("L").histogram()
    total = width * height
    within = sum(histogram[: tolerance + 1])
    sum_squares = 0
    sum_values = 0
    for value, count in enumerate(histogram):
        sum_values += value * count
        sum_squares += value * value * count
    return {
        "width": width,
        "height": height,
        "compared_pixels": total,
        "tolerance": tolerance,
        "similarity": round(within / total, 6),
        "matching_pixels": within,
        "mean_absolute_error": round(sum_values / total, 4),
        "root_mean_square_error": round((sum_squares / total) ** 0.5, 4),
        "reference_size": [rw, rh],
        "actual_size": [aw, ah],
        "size_delta": [aw - rw, ah - rh],
    }, diff


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--reference-rect", required=True)
    parser.add_argument("--actual", required=True)
    parser.add_argument("--actual-rect", default=None)
    parser.add_argument("--label", required=True)
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--tolerance", type=int, default=8)
    arguments = parser.parse_args()

    reference = Image.open(arguments.reference)
    actual = Image.open(arguments.actual)
    reference_rect = tuple(int(value) for value in arguments.reference_rect.split(","))
    if arguments.actual_rect:
        actual_rect = tuple(int(value) for value in arguments.actual_rect.split(","))
    else:
        actual_rect = detect_card_rect(actual)

    os.makedirs(arguments.out_dir, exist_ok=True)
    result, diff = compare(reference, actual, reference_rect, actual_rect, arguments.tolerance)
    result["label"] = arguments.label
    result["reference"] = os.path.basename(arguments.reference)
    result["actual"] = os.path.basename(arguments.actual)
    result["reference_rect"] = list(reference_rect)
    result["actual_rect"] = list(actual_rect)

    diff_path = os.path.join(arguments.out_dir, arguments.label + "-diff.png")
    diff.save(diff_path)
    reference.convert("RGB").crop(
        (
            reference_rect[0],
            reference_rect[1],
            reference_rect[0] + result["width"],
            reference_rect[1] + result["height"],
        )
    ).save(os.path.join(arguments.out_dir, arguments.label + "-reference-crop.png"))
    actual.convert("RGB").crop(
        (
            actual_rect[0],
            actual_rect[1],
            actual_rect[0] + result["width"],
            actual_rect[1] + result["height"],
        )
    ).save(os.path.join(arguments.out_dir, arguments.label + "-actual-crop.png"))
    with open(os.path.join(arguments.out_dir, arguments.label + ".json"), "w") as handle:
        json.dump(result, handle, indent=2)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
