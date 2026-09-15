#!/usr/bin/env python3
"""P0 component pixel gate: reference ChatGPT captures vs native GPUI captures.

The reference captures come from the dedicated debug instance at the same
logical viewport (1440x900, DPR 1). The native capture bundle writes Retina
screenshots at 2x, so each pair goes through one documented normalization step
(area-average 2x -> 1x BOX downscale) before fixed component rectangles are
compared. No crop is ever rescaled, translated, or searched to improve a score:
the gate score is always the fixed rectangle, and a best-offset value is
reported next to it purely as diagnostic evidence.

Usage:
    python3 scripts/p0/compare_p0_components.py --output artifacts/p0-stage/diff
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image


ROOT = Path(__file__).resolve().parents[2]
THRESHOLD = 99.0
LOGICAL_SIZE = (1440, 900)


@dataclass(frozen=True)
class Case:
    name: str
    theme: str
    reference: Path
    actual: Path
    # Fixed rectangles in logical pixels, taken from the reference DOM
    # measurement for this state (see artifacts/p0-stage/reference/<theme>/
    # measurements.json).
    rects: dict[str, tuple[int, int, int, int]] = field(default_factory=dict)


REFERENCE_DIR = ROOT / "artifacts/p0-stage/reference"
ACTUAL_DIR = ROOT / "artifacts/p0-stage/actual"

# Measured reference rectangles (x, y, width, height) for the states below.
DIALOG_RECT = (460, 198, 520, 102)
FILE_ROW_RECT = (465, 270, 510, 24)
EDIT_FORM_RECT = (333, 77, 734, 100)
EDIT_FORM_RECT_LIST = (333, 254, 734, 100)


def normalize(image: Image.Image) -> Image.Image:
    """One documented 2x -> 1x BOX normalization for Retina captures."""
    if image.size == LOGICAL_SIZE:
        return image.convert("RGB")
    return image.convert("RGB").resize(LOGICAL_SIZE, Image.Resampling.BOX)


def load(path: Path) -> Image.Image:
    return normalize(Image.open(path))


def score(reference: np.ndarray, actual: np.ndarray) -> float:
    difference = np.abs(reference.astype(np.float32) - actual.astype(np.float32))
    return float(100.0 * (1.0 - difference.mean() / 255.0))


def best_offset(reference: np.ndarray, actual: np.ndarray, limit: int = 40) -> tuple[int, int, float]:
    best = (0, 0, -1.0)
    height, width = reference.shape[:2]
    for dy in range(-limit, limit + 1, 2):
        for dx in range(-limit, limit + 1, 2):
            y0 = max(0, dy)
            x0 = max(0, dx)
            y1 = min(actual.shape[0], height + dy)
            x1 = min(actual.shape[1], width + dx)
            if y1 - y0 < height - 4 or x1 - x0 < width - 4:
                continue
            crop = actual[y0:y1, x0:x1]
            if crop.shape != reference.shape:
                continue
            value = score(reference, crop)
            if value > best[2]:
                best = (dx, dy, value)
    return best



def detect_edit_form(image: Image.Image, probe_x: int = 340, top: int = 46, bottom: int = 860) -> tuple[int, int, int, int] | None:
    """Locates the inline editor by its background band.

    The reference does not scroll to a fixed offset between runs, so the
    component is anchored to its own measured top edge: the form is the first
    wide band of the message ink at 5% alpha below the transcript header. The
    detected height must be 100 logical pixels; the comparison always uses the
    detected rectangle without rescaling or offset search.
    """
    pixels = np.asarray(image, dtype=np.int16)
    column = pixels[top:bottom, probe_x]
    background = pixels[top - 4, probe_x] if top >= 4 else column[0]
    mask = np.abs(column - background).sum(axis=1) > 6
    runs = []
    start = None
    for index, value in enumerate(mask):
        if value and start is None:
            start = index
        elif not value and start is not None:
            runs.append((start + top, index + top))
            start = None
    if start is not None:
        runs.append((start + top, bottom))
    for begin, end in runs:
        if end - begin >= 96:
            height = 100
            row = pixels[min(end - 2, begin + height // 2)]
            outside = pixels[min(end - 2, begin + height // 2), 200]
            changed = np.abs(row.astype(np.int16) - outside.astype(np.int16)).sum(axis=1)
            inside = np.where(changed > 6)[0]
            left = int(inside.min()) if inside.size else 0
            right = int(inside.max()) if inside.size else 0
            width = right - left + 1
            if 700 <= width <= 760:
                return (left, begin, width, height)
    return None

def crop(image: Image.Image, rect: tuple[int, int, int, int]) -> np.ndarray:
    x, y, width, height = rect
    return np.asarray(image.crop((x, y, x + width, y + height)), dtype=np.uint8)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default=str(ROOT / "artifacts/p0-stage/diff"))
    parser.add_argument("--diagnostic-offset", type=int, default=0)
    args = parser.parse_args()
    out_dir = Path(args.output)
    out_dir.mkdir(parents=True, exist_ok=True)

    cases = [
        Case("files-empty", "dark", REFERENCE_DIR / "dark/files-empty.png", ACTUAL_DIR / "dark-files-empty.png", {"dialog": DIALOG_RECT}),
        Case("files-results", "dark", REFERENCE_DIR / "dark/files-results.png", ACTUAL_DIR / "dark-files-results.png", {"dialog": DIALOG_RECT, "row": FILE_ROW_RECT}),
        Case("files-none", "dark", REFERENCE_DIR / "dark/files-none.png", ACTUAL_DIR / "dark-files-none.png", {"dialog": (460, 198, 520, 48)}),
        Case("message-edit", "dark", REFERENCE_DIR / "dark/edit-state.png", ACTUAL_DIR / "dark-message-edit.png", {}),
        Case("files-empty", "light", REFERENCE_DIR / "light/files-empty.png", ACTUAL_DIR / "light-files-empty.png", {"dialog": DIALOG_RECT}),
        Case("files-results", "light", REFERENCE_DIR / "light/files-results.png", ACTUAL_DIR / "light-files-results.png", {"dialog": DIALOG_RECT, "row": FILE_ROW_RECT}),
        Case("files-none", "light", REFERENCE_DIR / "light/files-none.png", ACTUAL_DIR / "light-files-none.png", {"dialog": (460, 198, 520, 48)}),
        Case("message-edit", "light", REFERENCE_DIR / "light/edit-state.png", ACTUAL_DIR / "light-message-edit.png", {}),
    ]

    report: dict[str, object] = {"threshold": THRESHOLD, "logical_size": list(LOGICAL_SIZE), "components": []}
    failures = []
    for case in cases:
        if not case.reference.exists() or not case.actual.exists():
            print(f"skip {case.theme}/{case.name}: missing capture")
            continue
        reference = load(case.reference)
        actual = load(case.actual)
        rects = dict(case.rects)
        if case.name == "message-edit":
            reference_form = detect_edit_form(reference)
            actual_form = detect_edit_form(actual)
            if reference_form is None or actual_form is None:
                print(f"skip {case.theme}/{case.name}: inline editor not located")
                continue
            rects["form"] = actual_form
            report["components"].append({
                "component": f"{case.theme}/{case.name}/geometry",
                "reference_rect": list(reference_form),
                "actual_rect": list(actual_form),
                "size_match": [reference_form[2] == actual_form[2], reference_form[3] == actual_form[3]],
                "passed": reference_form[2:] == actual_form[2:],
            })
            if reference_form[2:] != actual_form[2:]:
                failures.append(f"{case.theme}/{case.name}/geometry")
        for label, rect in rects.items():
            reference_crop = crop(reference, rect)
            actual_crop = crop(actual, rect)
            similarity = score(reference_crop, actual_crop)
            entry = {
                "component": f"{case.theme}/{case.name}/{label}",
                "rect": list(rect),
                "similarity": round(similarity, 3),
                "passed": similarity >= THRESHOLD,
            }
            if args.diagnostic_offset:
                dx, dy, best = best_offset(reference_crop, actual_crop, args.diagnostic_offset)
                entry["best_offset"] = [dx, dy]
                entry["best_offset_similarity"] = round(best, 3)
            report["components"].append(entry)
            print(f"{entry['component']:<34} {similarity:6.2f}% {rect}")
            if not entry["passed"]:
                failures.append(entry["component"])
            # Keep the crops for review.
            stem = f"{case.theme}-{case.name}-{label}"
            reference.crop((rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3])).save(out_dir / f"reference-{stem}.png")
            actual.crop((rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3])).save(out_dir / f"actual-{stem}.png")

    report["failed_components"] = failures
    report["passed"] = not failures
    (out_dir / "report.json").write_text(json.dumps(report, indent=2))
    print("gate:", "PASS" if not failures else f"FAIL ({len(failures)} components below {THRESHOLD}%)")
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())

