#!/usr/bin/env python3
"""Compare the phase-two account surfaces against the ChatGPT reference.

Both sides are compared as equal-size crops of same-size captures (1440x900 at
DPR 1). Nothing is scaled, translated, or resized: every region uses the pixel
rectangle the capture recorded, and a size mismatch is a hard failure.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops

# name: (reference rect, GPUI rect) inside the 1440x900 captures. The account
# menu and the logout confirmation share one rect because both applications
# place those surfaces at the same coordinates; the billing card sits lower in
# the GPUI settings layout, so each side measures its own card band.
REGIONS: dict[str, dict[str, object]] = {
    "account_menu": {
        # The reference menu capture is already cropped to the menu band.
        "reference_rect": None,
        "actual_rect": (0, 637, 256, 871),
        "reference_file": "account-menu-{theme}.png",
        "actual_file": "account-menu-{theme}.png",
    },
    "logout_confirm": {
        # The reference dialog capture is already cropped to the dialog band.
        "reference_rect": None,
        "actual_rect": (506, 341, 934, 559),
        "reference_file": "logout-confirm-{theme}.png",
        "actual_file": "logout-confirm-{theme}.png",
    },
    "usage_quota_card": {
        "reference_rect": (440, 556, 1230, 634),
        "actual_rect": (440, 496, 1230, 574),
        "reference_file": "settings-usage-{theme}.png",
        "actual_file": "usage-{theme}.png",
    },
}

TOLERANCE = 8


def score(reference: Image.Image, actual: Image.Image) -> dict[str, float | int]:
    if reference.size != actual.size:
        raise SystemExit(
            f"region size mismatch: reference={reference.size} actual={actual.size}"
        )
    diff = ImageChops.difference(reference.convert("RGB"), actual.convert("RGB"))
    pixels = list(diff.getdata())
    total = len(pixels)
    exact = sum(pixel == (0, 0, 0) for pixel in pixels)
    within = sum(max(pixel) <= TOLERANCE for pixel in pixels)
    absolute_error = sum(sum(pixel) for pixel in pixels)
    max_error = 255 * 3 * total
    adjusted_error = sum(
        sum(max(channel - TOLERANCE, 0) for channel in pixel) for pixel in pixels
    )
    return {
        "pixels": total,
        "normalized_similarity": round((1 - absolute_error / max_error) * 100, 6),
        "tolerance_adjusted_similarity": round(
            (1 - adjusted_error / max_error) * 100, 6
        ),
        "exact_match_ratio": round(exact / total * 100, 6),
        "tolerance_match_ratio": round(within / total * 100, 6),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--reference", default="artifacts/account-phase/chatgpt-reference"
    )
    parser.add_argument("--actual", default="artifacts/account-phase/ui-validation/gpui")
    parser.add_argument("--output", default="artifacts/account-phase/ui-validation/compare")
    parser.add_argument(
        "--min-similarity",
        type=float,
        default=0.0,
        help="Fail when a region scores below this normalized similarity.",
    )
    args = parser.parse_args()

    reference_dir = Path(args.reference)
    actual_dir = Path(args.actual)
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)

    report: dict[str, object] = {
        "tolerance": TOLERANCE,
        "regions": {},
        "failures": [],
    }
    for theme in ("light", "dark"):
        for name, region in REGIONS.items():
            reference_rect = region["reference_rect"]
            actual_rect = region["actual_rect"]
            reference_source = reference_dir / region["reference_file"].format(theme=theme)
            actual_source = actual_dir / region["actual_file"].format(theme=theme)
            reference = Image.open(reference_source)
            if reference_rect is not None:
                reference = reference.crop(reference_rect)
            actual = Image.open(actual_source).crop(actual_rect)
            result = score(reference, actual)
            reference.save(output / f"{name}-{theme}-reference.png")
            actual.save(output / f"{name}-{theme}-gpui.png")
            diff = ImageChops.difference(reference.convert("RGB"), actual.convert("RGB"))
            diff.save(output / f"{name}-{theme}-diff.png")
            report["regions"][f"{name}-{theme}"] = {
                **result,
                "reference": str(reference_source),
                "actual": str(actual_source),
                "reference_rect": reference_rect,
                "actual_rect": actual_rect,
            }
            if result["normalized_similarity"] < args.min_similarity:
                report["failures"].append(f"{name}-{theme}")

    path = output / "account-phase-similarity.json"
    path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    for key, value in report["regions"].items():
        print(f"{key:28} {value['normalized_similarity']:>9.4f}%  exact {value['exact_match_ratio']:>7.3f}%")
    print(f"wrote {path}")
    if report["failures"]:
        raise SystemExit(f"regions below the threshold: {', '.join(report['failures'])}")


if __name__ == "__main__":
    main()
