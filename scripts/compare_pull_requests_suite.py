#!/usr/bin/env python3
"""Compare every reference Pull Requests capture with its native counterpart.

Both ends are captured at the same window size (1440x900 logical), the same
DPR (2), the same theme, the same content, and the same scroll position. The
reference is the ChatGPT/Codex Electron app driven over CDP; the native end is this
worktree's packaged capture bundle (`scripts/gpui_capture_binary.sh`)
rasterized by `render_to_image`. Neither side is rescaled or
translated.

Metrics per component (all in percent, higher is closer):

* `identical` - pixels that match exactly. Chromium and CoreText distribute
  glyph coverage differently, so this is only ever high on flat regions.
* `mae` - 100 * (1 - mean absolute channel error / 255).
* `cons24` - pixels whose worst channel differs by at most 24.
* `blur2_mae` - the same as `mae` after blurring both sides with a 2px Gaussian.
  This removes single-pixel antialiasing noise and leaves layout, colour, and
  glyph-weight differences, which are what a replication has to get right.
* `ink_ratio` - total ink (255 - luminance) of the native capture divided by the
  reference's. 1.0 means the same amount of glyph coverage was drawn.

Usage:

    python3 scripts/compare_pull_requests_suite.py \
        --output artifacts/pull-requests-compare/suite.json
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter


# Reference capture name -> native capture name, per Pull Requests state.
STATES = {
    "list": ("list-default", "list"),
    "list-reviewing": ("list-reviewing", "list-reviewing"),
    "list-authored": ("list-authored", "list-authored"),
    "list-search": ("list-search", "list-search"),
    "list-search-empty": ("list-search-empty", "list-search-empty"),
    "list-group-collapsed": ("list-group-collapsed", "list-group-collapsed"),
    "list-filter-menu": ("list-filter-menu", "list-filter-menu"),
    "list-filter-status": ("list-filter-status", "list-filter-status"),
    "list-filter-repository": ("list-filter-repository", "list-filter-repository"),
    "summary": ("detail-summary", "summary"),
    "summary-title-edit": ("detail-title-edit", "summary-title-edit"),
    "summary-status-menu": ("detail-status-menu", "summary-status-menu"),
    "summary-description-menu": ("detail-description-menu", "summary-description-menu"),
    "summary-reviewers": ("detail-reviewers-dialog", "summary-reviewers"),
    "code": ("code-diff", "code"),
    "code-review-options": ("code-review-options", "code-review-options"),
    "code-file-tree": ("code-file-tree", "code-tree"),
    "review-tab": ("review-tab", "review-tab"),
    "activity": ("detail-activity", "activity"),
}

# Component regions in absolute window pixels of the 1440x900 layout. The app
# sidebar (x < 276) is the shared application shell, so it is excluded and the
# comparison covers the Pull Requests surface itself.
COMPONENTS = {
    "page": (276, 0, 1440, 900),
    "list_pane": (276, 0, 794, 900),
    "list_tabs": (276, 0, 794, 46),
    "list_search": (276, 46, 794, 106),
    "list_group_header": (276, 106, 794, 148),
    "list_row_selected": (276, 148, 794, 212),
    "list_row_idle": (276, 212, 794, 340),
    "detail_pane": (794, 0, 1440, 900),
    "detail_toolbar": (794, 0, 1440, 46),
    "detail_title": (794, 46, 1440, 176),
    "detail_meta": (794, 176, 1440, 348),
    "detail_description_header": (794, 348, 1440, 390),
    "detail_body": (794, 390, 1440, 900),
    "diff_toolbar": (794, 46, 1440, 140),
    "diff_body": (794, 140, 1440, 900),
    "file_tree": (1220, 140, 1440, 900),
}

# Which components belong to which state, so a report never scores a region the
# state does not show.
STATE_COMPONENTS = {
    "list": ["page", "list_pane", "list_tabs", "list_search", "list_group_header", "list_row_selected", "list_row_idle", "detail_pane"],
    "summary": ["page", "list_pane", "detail_pane", "detail_toolbar", "detail_title", "detail_meta", "detail_description_header", "detail_body"],
    "code": ["page", "list_pane", "detail_pane", "diff_toolbar", "diff_body"],
    "code-review-options": ["page", "detail_pane", "diff_toolbar"],
    "code-file-tree": ["page", "detail_pane", "diff_toolbar", "diff_body", "file_tree"],
    "review-tab": ["page", "detail_pane", "diff_toolbar", "diff_body"],
    "activity": ["page", "detail_pane", "detail_body"],
}
DEFAULT_COMPONENTS = ["page"]


def region_metrics(reference: Image.Image, actual: Image.Image, box, scale: float) -> dict:
    left, top, right, bottom = (
        int(box[0] * scale),
        int(box[1] * scale),
        int(box[2] * scale),
        int(box[3] * scale),
    )
    ref = np.asarray(reference.crop((left, top, right, bottom)), dtype=np.int16)
    got = np.asarray(actual.crop((left, top, right, bottom)), dtype=np.int16)
    delta = np.abs(ref - got)
    worst = delta.max(axis=2)
    blurred_ref = np.asarray(
        reference.crop((left, top, right, bottom)).filter(ImageFilter.GaussianBlur(2)),
        dtype=np.int16,
    )
    blurred_got = np.asarray(
        actual.crop((left, top, right, bottom)).filter(ImageFilter.GaussianBlur(2)),
        dtype=np.int16,
    )
    blurred_delta = np.abs(blurred_ref - blurred_got)

    def ink(image):
        luminance = 0.2126 * image[:, :, 0] + 0.7152 * image[:, :, 1] + 0.0722 * image[:, :, 2]
        return float((255.0 - luminance).sum())

    reference_ink = ink(ref)
    return {
        "rect": [left, top, right, bottom],
        "pixels": int(worst.size),
        "identical": round(float((worst == 0).mean()) * 100, 4),
        "mae": round(float(1 - delta.mean() / 255) * 100, 4),
        "cons24": round(float((worst <= 24).mean()) * 100, 4),
        "cons48": round(float((worst <= 48).mean()) * 100, 4),
        "blur2_mae": round(float(1 - blurred_delta.mean() / 255) * 100, 4),
        "ink_ratio": round(ink(got) / reference_ink, 5) if reference_ink else None,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference-dir", type=Path, default=Path("artifacts/pull-requests-reference"))
    parser.add_argument("--actual-dir", type=Path, default=Path("artifacts/pull-requests-gpui"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/pull-requests-compare/suite.json"))
    parser.add_argument("--scale", type=float, default=2.0)
    parser.add_argument("--themes", default="light,dark")
    args = parser.parse_args()

    themes = [theme for theme in args.themes.split(",") if theme]
    report = {"scale": args.scale, "themes": themes, "states": {}}
    missing = []
    for state, (reference_name, actual_name) in STATES.items():
        entry = {}
        for theme in themes:
            reference_path = args.reference_dir / f"{reference_name}-{theme}.png"
            actual_path = args.actual_dir / f"{actual_name}-{theme}.png"
            if not reference_path.exists() or not actual_path.exists():
                missing.append(f"{state}/{theme}")
                continue
            reference = Image.open(reference_path).convert("RGB")
            actual = Image.open(actual_path).convert("RGB")
            if reference.size != actual.size:
                missing.append(f"{state}/{theme} (size {reference.size} vs {actual.size})")
                continue
            names = STATE_COMPONENTS.get(state, DEFAULT_COMPONENTS)
            entry[theme] = {
                name: region_metrics(reference, actual, COMPONENTS[name], args.scale)
                for name in names
            }
        if entry:
            report["states"][state] = entry

    report["missing"] = missing
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")

    print(f"{'state':24s} {'theme':5s} {'component':26s} {'identical':>9s} {'mae':>8s} {'cons24':>8s} {'blur2_mae':>10s} {'ink':>7s}")
    for state, themes_entry in report["states"].items():
        for theme, components in themes_entry.items():
            for name, values in components.items():
                print(
                    f"{state:24s} {theme:5s} {name:26s} {values['identical']:9.2f} {values['mae']:8.2f} "
                    f"{values['cons24']:8.2f} {values['blur2_mae']:10.2f} "
                    f"{values['ink_ratio'] if values['ink_ratio'] is not None else float('nan'):7.3f}"
                )
    if missing:
        print("\nmissing:", ", ".join(missing))


if __name__ == "__main__":
    main()
