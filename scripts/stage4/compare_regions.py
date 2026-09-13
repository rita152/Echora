#!/usr/bin/env python3
"""Machine readable ChatGP/GPUI comparison for stage-4 local regions.

Reports exact and near-match pixel similarity per region, writes diff overlays,
and fails when a region is below the required similarity.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops

# Regions are expressed in the shared 1440x900 logical layout of both clients.
REGIONS: dict[str, tuple[int, int, int, int]] = {
    "plugins_header": (466, 60, 1234, 130),
    "plugins_segments": (466, 148, 1234, 190),
    "mcp_list": (466, 226, 1234, 600),
    "mcp_section_header": (466, 226, 1234, 262),
    "mcp_first_rows": (466, 262, 1234, 470),
    "skills_list": (466, 226, 1234, 320),
    "skills_first_row": (466, 226, 1234, 292),
    "settings_nav": (0, 0, 275, 900),
    "mcp_detail_header": (466, 150, 1234, 230),
    "mcp_detail_fields": (466, 230, 1234, 520),
    "mcp_detail_tools": (466, 520, 1234, 900),
    "oauth_dialog": (500, 300, 940, 620),
}


def region_box(name: str, width: int, height: int) -> tuple[int, int, int, int]:
    left, top, right, bottom = REGIONS[name]
    return (min(left, width), min(top, height), min(right, width), min(bottom, height))


def compare_region(reference: Image.Image, actual: Image.Image, box: tuple[int, int, int, int], tolerance: int) -> dict:
    reference_region = reference.crop(box).convert("RGB")
    actual_region = actual.crop(box).convert("RGB")
    difference = ImageChops.difference(reference_region, actual_region)
    pixels = list(difference.getdata())
    total = len(pixels)
    exact = 0
    near = 0
    absolute = 0
    for red, green, blue in pixels:
        worst = max(red, green, blue)
        absolute += red + green + blue
        if worst == 0:
            exact += 1
            near += 1
        elif worst <= tolerance:
            near += 1
    return {
        "width": box[2] - box[0],
        "height": box[3] - box[1],
        "pixels": total,
        "exact_similarity": round(100.0 * exact / total, 4),
        "near_similarity": round(100.0 * near / total, 4),
        "mean_absolute_error": round(absolute / (total * 3), 4),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--output", type=Path, default=Path("artifacts/pixel-diff"))
    parser.add_argument("--tolerance", type=int, default=2)
    parser.add_argument("--min-similarity", type=float, default=99.5)
    parser.add_argument("--regions", nargs="*", default=sorted(REGIONS))
    args = parser.parse_args()

    reference = Image.open(args.reference).convert("RGB")
    actual = Image.open(args.actual).convert("RGB")
    if reference.size != actual.size:
        raise SystemExit(f"size mismatch: {reference.size} != {actual.size}")
    width, height = reference.size

    args.output.mkdir(parents=True, exist_ok=True)
    results = {}
    failures = []
    for name in args.regions:
        if name not in REGIONS:
            raise SystemExit(f"unknown region: {name}")
        box = region_box(name, width, height)
        metrics = compare_region(reference, actual, box, args.tolerance)
        results[name] = {"box": list(box), **metrics}
        diff = ImageChops.difference(reference.crop(box), actual.crop(box)).convert("L")
        diff.point(lambda value: min(255, value * 6)).save(args.output / f"{name}-diff.png")
        Image.new("RGB", (box[2] - box[0], box[3] - box[1]), "white").paste(
            actual.crop(box), (0, 0)
        )
        actual.crop(box).save(args.output / f"{name}-actual.png")
        reference.crop(box).save(args.output / f"{name}-reference.png")
        if metrics["near_similarity"] < args.min_similarity:
            failures.append((name, metrics["near_similarity"]))

    report = {
        "reference": str(args.reference),
        "actual": str(args.actual),
        "size": [width, height],
        "tolerance": args.tolerance,
        "min_similarity": args.min_similarity,
        "regions": results,
        "failures": [{"region": name, "near_similarity": value} for name, value in failures],
    }
    (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({name: results[name]["near_similarity"] for name in args.regions}, indent=2))
    if failures:
        raise SystemExit(
            "regions below the required similarity: "
            + ", ".join(f"{name}={value}" for name, value in failures)
        )


if __name__ == "__main__":
    main()
