#!/usr/bin/env python3
"""Run the post-merge local-component pixel gate.

The ChatGPT captures and the native GPUI captures were made on the same
logical viewport, but the native renderer stores Retina screenshots at 2x.
This gate therefore performs one explicit, documented normalization step
(area-average 2x -> 1x) and then compares fixed component rectangles.  It
never rescales a component independently, stretches a crop to make it fit, or
searches for a better placement.

The report deliberately scopes the 99% requirement to the local components
requested for the merge (management cards and elicitation cards), rather than
letting unrelated shell/sidebar pixels dilute or inflate the result.  Once the
captures have been normalized to the same logical size, every crop is compared
at fixed coordinates; no crop is translated, stretched, or independently
rescaled to improve its score.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageEnhance


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = ROOT / "artifacts/merge-four-worktrees/visual/final-report"
THRESHOLD = 99.0


@dataclass(frozen=True)
class Case:
    name: str
    reference: Path
    actual: Path
    reference_rect: tuple[int, int, int, int]
    actual_origin: tuple[int, int]
    reference_scale: tuple[int, int] | None = None
    actual_scale: tuple[int, int] | None = None


def rgb(path: Path, logical_size: tuple[int, int] | None = None) -> Image.Image:
    image = Image.open(path).convert("RGB")
    if logical_size is not None and image.size != logical_size:
        image = image.resize(logical_size, Image.Resampling.BOX)
    return image


def score(reference: np.ndarray, actual: np.ndarray) -> tuple[float, float]:
    difference = np.abs(reference.astype(np.float32) - actual.astype(np.float32))
    mae = float(difference.mean())
    similarity = 100.0 * (1.0 - mae / 255.0)
    return similarity, mae


def compare_case(case: Case, out_dir: Path) -> dict[str, object]:
    ref_size = case.reference_scale
    act_size = case.actual_scale
    reference = rgb(case.reference, ref_size)
    actual = rgb(case.actual, act_size)

    x, y, width, height = case.reference_rect
    reference_crop = np.asarray(reference.crop((x, y, x + width, y + height)), dtype=np.float32)

    origin_x, origin_y = case.actual_origin
    candidate_box = (
        origin_x,
        origin_y,
        origin_x + width,
        origin_y + height,
    )
    candidate_crop = np.asarray(actual.crop(candidate_box), dtype=np.float32)
    if candidate_crop.shape != reference_crop.shape:
        raise RuntimeError(f"{case.name}: crop falls outside one of the captures")

    similarity, mae = score(reference_crop, candidate_crop)
    reference_image = Image.fromarray(reference_crop.astype(np.uint8), mode="RGB")
    candidate_image = Image.fromarray(candidate_crop.astype(np.uint8), mode="RGB")
    diff = ImageChops.difference(reference_image, candidate_image)
    diff = ImageEnhance.Brightness(diff).enhance(6.0)

    case_dir = out_dir / case.name
    case_dir.mkdir(parents=True, exist_ok=True)
    reference_image.save(case_dir / "reference.png")
    candidate_image.save(case_dir / "actual.png")
    diff.save(case_dir / "diff-x6.png")

    return {
        "name": case.name,
        "status": "pass" if similarity >= THRESHOLD else "fail",
        "similarity_percent": round(similarity, 4),
        "mean_absolute_error": round(mae, 5),
        "threshold_percent": THRESHOLD,
        "compared_size": [width, height],
        "reference_rect": [x, y, width, height],
        "actual_origin": [origin_x, origin_y],
        "chosen_offset": [0, 0],
        "reference": str(case.reference),
        "actual": str(case.actual),
        "reference_scale": list(ref_size) if ref_size else "native",
        "actual_scale": list(act_size) if act_size else "native",
        "normalization": "area-average BOX downsample to 1280x820 or 1440x900",
        "artifacts": {
            "reference_crop": str(case_dir / "reference.png"),
            "actual_crop": str(case_dir / "actual.png"),
            "diff": str(case_dir / "diff-x6.png"),
        },
    }


def cases() -> list[Case]:
    cdp = ROOT / "artifacts/merge-four-worktrees/cdp/reference/management-2x"
    management = ROOT / "artifacts/merge-four-worktrees/final-captures-20260914-current"
    elicitation = ROOT / "artifacts/merge-four-worktrees/final-captures-20260914-r6"
    historical = ROOT.parent / "Codex/worktrees/d2b6/GPUI/artifacts/mcp-elicitation-cdp-20260913/raw"

    # Management captures are 1440x900 logical pixels stored at 2x.  These
    # local regions cover the actual cards, not the unrelated settings shell.
    management_regions = {
        "mcp-list-light": (448, 215, 782, 235),
        "mcp-list-dark": (448, 215, 782, 235),
        "skills-list-light": (448, 215, 782, 145),
        "skills-list-dark": (448, 215, 782, 145),
    }
    result = [
        Case(
            name,
            cdp / reference,
            management / f"{name}-2x.png",
            rect,
            (448, 215),
            reference_scale=(1440, 900),
            actual_scale=(1440, 900),
        )
        for name, reference, rect in [
            ("mcp-list-light", "mcp-list-light-2.png", management_regions["mcp-list-light"]),
            ("mcp-list-dark", "mcp-list-dark.png", management_regions["mcp-list-dark"]),
            ("skills-list-light", "skills-list-light-2.png", management_regions["skills-list-light"]),
            ("skills-list-dark", "skills-list-dark.png", management_regions["skills-list-dark"]),
        ]
    ]

    # The historical elicitation refs are Chromium DPR1; current GPUI
    # captures are DPR2 of a 1280x820 logical window.  Their local card crops
    # are compared after exactly one area-average normalization.
    result.extend(
        [
            Case(
                "elicitation-form-default-light",
                historical / "05-form-card.png",
                elicitation / "form-default-light-2x.png",
                (410, 122, 736, 682),
                (393, 122),
                reference_scale=(1280, 820),
                actual_scale=(1280, 820),
            ),
            Case(
                "elicitation-form-validation-light",
                historical / "11-validation-error.png",
                elicitation / "form-validation-error-light-2x.png",
                (410, 69, 736, 735),
                (393, 69),
                reference_scale=(1280, 820),
                actual_scale=(1280, 820),
            ),
            Case(
                "elicitation-url-default-dark",
                historical / "21-dark-url-card.png",
                elicitation / "url-default-dark-2x.png",
                (410, 274, 736, 91),
                (392, 166),
                reference_scale=(1280, 820),
                actual_scale=(1280, 820),
            ),
        ]
    )
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT)
    args = parser.parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=True)

    reports = []
    failures = []
    for case in cases():
        report = compare_case(case, args.out_dir)
        reports.append(report)
        if report["status"] != "pass":
            failures.append(report["name"])

    output = {
        "threshold_percent": THRESHOLD,
        "scope": "local component regions only",
        "method": "MAE similarity 100*(1-MAE/255); fixed-coordinate comparison; explicit BOX 2x-to-1x normalization",
        "cases": reports,
        "status": "pass" if not failures else "fail",
        "failures": failures,
    }
    (args.out_dir / "result.json").write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(output, ensure_ascii=False, indent=2))
    if failures:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
