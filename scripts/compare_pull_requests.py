#!/usr/bin/env python3
"""Per-component pixel comparison for the Pull Requests page.

Compares a reference capture (ChatGPT/Codex desktop app over CDP) with a GPUI
capture of the same state, at the same window size, theme, and DPR, and reports
the similarity of every component the checklist names. Regions are expressed in
absolute window pixels for the 1440x900 reference layout, so the same table can
be applied to both captures without scaling or translation.

Usage:

    python3 scripts/compare_pull_requests.py \
        artifacts/pull-requests-reference/detail-summary-light.png \
        artifacts/pull-requests-gpui/summary-light.png \
        --state detail-summary --theme light \
        --output artifacts/pull-requests-compare/summary-light
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance, ImageFilter


# Absolute-pixel component regions of the 1440x900 reference window. The app
# sidebar (x < 276) is excluded: its copy is localized differently on each end
# and is not part of the Pull Requests page being replicated.
COMPONENTS = {
    "list_toolbar": (276, 0, 794, 46),
    "list_search": (276, 46, 794, 106),
    "list_group_header": (276, 106, 794, 148),
    "list_row_selected": (276, 148, 794, 212),
    "list_row_idle": (276, 212, 794, 340),
    "list_pane": (276, 0, 794, 900),
    "detail_toolbar": (794, 0, 1440, 46),
    "detail_title": (794, 46, 1440, 176),
    "detail_meta": (794, 176, 1440, 348),
    "detail_description_header": (794, 348, 1440, 390),
    "detail_body": (794, 390, 1440, 900),
    "detail_pane": (794, 0, 1440, 900),
    "page": (276, 0, 1440, 900),
}


def compare(reference: Path, actual: Path, output: Path, tolerance: int, scale: float = 2.0) -> dict:
    ref = Image.open(reference).convert("RGB")
    got = Image.open(actual).convert("RGB")
    if ref.size != got.size:
        raise SystemExit(
            f"capture sizes differ: {reference} is {ref.size}, {actual} is {got.size}; "
            "capture both at the same window size and DPR"
        )
    width, height = ref.size
    diff = ImageChops.difference(ref, got)
    pixels = list(diff.get_flattened_data())
    total = len(pixels)
    exact = sum(1 for pixel in pixels if max(pixel) == 0)
    matching = sum(1 for pixel in pixels if max(pixel) <= tolerance)
    absolute_error = sum(sum(pixel[:3]) for pixel in pixels)
    adjusted_error = sum(sum(max(channel - tolerance, 0) for channel in pixel[:3]) for pixel in pixels)
    max_error = total * 255 * 3

    ref_edges = ref.convert("L").filter(ImageFilter.FIND_EDGES)
    got_edges = got.convert("L").filter(ImageFilter.FIND_EDGES)
    edge_mask = Image.frombytes(
        "L",
        ref.size,
        bytes(
            255 if max(a, b) > 12 else 0
            for a, b in zip(ref_edges.get_flattened_data(), got_edges.get_flattened_data())
        ),
    ).filter(ImageFilter.MaxFilter(3))
    edge_flags = list(edge_mask.get_flattened_data())
    edge_total = sum(bool(value) for value in edge_flags)
    edge_matching = sum(
        bool(mask) and max(pixel) <= tolerance for mask, pixel in zip(edge_flags, pixels)
    )

    components = {}
    for name, (left, top, right, bottom) in COMPONENTS.items():
        left, top, right, bottom = (
            int(left * scale),
            int(top * scale),
            int(right * scale),
            int(bottom * scale),
        )
        left, right = max(0, min(left, width)), max(0, min(right, width))
        top, bottom = max(0, min(top, height)), max(0, min(bottom, height))
        if right <= left or bottom <= top:
            continue
        indices = [y * width + x for y in range(top, bottom) for x in range(left, right)]
        region_total = len(indices)
        region_matching = sum(max(pixels[index]) <= tolerance for index in indices)
        region_exact = sum(max(pixels[index]) == 0 for index in indices)
        region_adjusted = sum(
            sum(max(channel - tolerance, 0) for channel in pixels[index][:3])
            for index in indices
        )
        region_edges = [index for index in indices if edge_flags[index]]
        components[name] = {
            "rect": [left, top, right, bottom],
            "pixel_consistency": round(region_matching / region_total * 100, 4),
            "exact_pixel_consistency": round(region_exact / region_total * 100, 4),
            "tolerance_adjusted_similarity": round(
                (1 - region_adjusted / (region_total * 255 * 3)) * 100, 4
            ),
            "edge_pixel_consistency": (
                round(
                    sum(max(pixels[index]) <= tolerance for index in region_edges)
                    / len(region_edges)
                    * 100,
                    4,
                )
                if region_edges
                else 100.0
            ),
            "different_pixels": region_total - region_matching,
            "total_pixels": region_total,
        }

    output.mkdir(parents=True, exist_ok=True)
    ImageEnhance.Contrast(diff.convert("RGB")).enhance(4.0).save(output / "diff.png")
    Image.blend(ref, got, 0.5).save(output / "overlay.png")
    side = Image.new("RGB", (width * 2, height))
    side.paste(ref, (0, 0))
    side.paste(got, (width, 0))
    side.save(output / "side-by-side.png")

    report = {
        "reference": str(reference),
        "actual": str(actual),
        "size": list(ref.size),
        "tolerance": tolerance,
        "whole_page": {
            "pixel_consistency": round(matching / total * 100, 4),
            "exact_pixel_consistency": round(exact / total * 100, 4),
            "tolerance_adjusted_similarity": round((1 - adjusted_error / max_error) * 100, 4),
            "edge_pixel_consistency": round(edge_matching / edge_total * 100, 4) if edge_total else 100.0,
        },
        "components": components,
    }
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--output", type=Path, default=Path("artifacts/pull-requests-compare/latest"))
    parser.add_argument("--tolerance", type=int, default=0)
    parser.add_argument("--scale", type=float, default=2.0, help="capture pixels per window point")
    parser.add_argument("--state", default="")
    parser.add_argument("--theme", default="")
    parser.add_argument("--min-consistency", type=float, default=None)
    args = parser.parse_args()

    report = compare(args.reference, args.actual, args.output, args.tolerance, args.scale)
    report["state"] = args.state
    report["theme"] = args.theme
    (args.output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    if args.min_consistency is not None:
        worst = min(
            (value["pixel_consistency"], name)
            for name, value in report["components"].items()
            if name in {"list_pane", "detail_pane"}
        )
        if worst[0] < args.min_consistency:
            raise SystemExit(f"{worst[1]} pixel consistency {worst[0]} < {args.min_consistency}")


if __name__ == "__main__":
    main()
