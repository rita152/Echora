#!/usr/bin/env python3
"""Compare the Pull Requests page colors against the reference captures.

Samples the same window coordinates in a reference capture (CDP) and a GPUI
capture of the same state, and reports every surface, border, and text color
that differs. Coordinates come from the reference layout (1440x900, DPR 1).

    python3 scripts/audit_pull_requests_colors.py --state list --theme light
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image


# name -> (box, mode); mode "surface" takes the median color of the box (flat
# fills), "text" takes the darkest pixel (the core of a glyph run).
PROBES = {
    "list": {
        "list_surface": ((500, 600, 520, 620), "surface"),
        "list_divider": ((793, 400, 795, 600), "surface"),
        "tab_selected_fill": ((318, 12, 325, 24), "surface"),
        "tab_idle_text": ((330, 15, 400, 32), "text"),
        "search_fill": ((560, 76, 580, 90), "surface"),
        "search_border": ((400, 66, 700, 68), "text"),
        "filter_button_fill": ((738, 76, 756, 90), "surface"),
        "group_header_text": ((305, 122, 360, 136), "text"),
        "row_title_text": ((337, 158, 640, 178), "text"),
        "row_meta_text": ((337, 184, 430, 200), "text"),
        "row_additions": ((670, 184, 700, 200), "text"),
        "row_deletions": ((706, 184, 740, 200), "text"),
        "row_icon": ((306, 167, 330, 191), "text"),
        "detail_surface": ((1200, 600, 1220, 620), "surface"),
    },
    "summary": {
        "list_surface": ((500, 600, 520, 620), "surface"),
        "selected_row_fill": ((500, 170, 560, 190), "surface"),
        "detail_surface": ((1380, 700, 1400, 720), "surface"),
        "detail_tab_fill": ((936, 12, 948, 24), "surface"),
        "chat_button_fill": ((1300, 12, 1320, 24), "surface"),
        "merge_button_fill": ((1360, 12, 1380, 24), "surface"),
        "title_text": ((820, 70, 1300, 100), "text"),
        "author_text": ((845, 134, 880, 150), "text"),
        "meta_label_text": ((849, 184, 900, 200), "text"),
        "meta_value_text": ((955, 184, 1100, 200), "text"),
        "description_header_text": ((823, 372, 910, 392), "text"),
        "additions_text": ((1330, 178, 1370, 200), "text"),
        "deletions_text": ((1370, 178, 1400, 200), "text"),
    },
    "code": {
        "detail_surface": ((1380, 700, 1400, 720), "surface"),
        "branch_line_text": ((800, 60, 1000, 76), "text"),
        "file_header_surface": ((1150, 96, 1200, 112), "surface"),
        "file_header_text": ((870, 96, 1050, 112), "text"),
        "expander_surface": ((1150, 126, 1300, 140), "surface"),
        "expander_text": ((815, 126, 900, 140), "text"),
        "context_row_surface": ((1150, 156, 1300, 166), "surface"),
        "added_row_surface": ((1150, 186, 1300, 200), "surface"),
        "deleted_row_surface": ((1150, 216, 1300, 230), "surface"),
        "context_number_text": ((810, 154, 840, 168), "text"),
        "added_number_surface": ((800, 186, 840, 200), "surface"),
        "deleted_number_surface": ((800, 216, 840, 230), "surface"),
        "code_text": ((860, 154, 1100, 168), "text"),
    },
}


def measure(
    image: Image.Image, box: tuple[int, int, int, int], mode: str, scale: float = 1.0
) -> tuple[int, int, int]:
    left, top, right, bottom = (round(value * scale) for value in box)
    pixels = [
        image.getpixel((x, y))[:3]
        for y in range(top, bottom)
        for x in range(left, right)
    ]
    if mode == "surface":
        pixels.sort(key=lambda pixel: sum(pixel))
        return pixels[len(pixels) // 2]
    return min(pixels, key=sum)


def sample(image: Image.Image, point: tuple[int, int]) -> tuple[int, int, int]:
    return image.getpixel(point)[:3]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", required=True, choices=sorted(PROBES))
    parser.add_argument("--theme", default="light")
    parser.add_argument("--scale", type=float, default=2.0, help="capture pixels per window point")
    parser.add_argument(
        "--reference",
        type=Path,
        default=Path("artifacts/pull-requests-reference"),
    )
    parser.add_argument("--actual", type=Path, default=Path("artifacts/pull-requests-gpui"))
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("artifacts/pull-requests-compare/colors"),
    )
    args = parser.parse_args()

    reference_name = {
        "list": "list-default",
        "summary": "detail-summary",
        "code": "code-diff",
    }[args.state]
    reference = Image.open(args.reference / f"{reference_name}-{args.theme}.png").convert("RGB")
    actual_name = {"summary": "summary", "code": "code", "list": "list"}[args.state]
    actual = Image.open(args.actual / f"{actual_name}-{args.theme}.png").convert("RGB")

    rows = []
    mismatches = 0
    for name, (box, mode) in PROBES[args.state].items():
        expected = measure(reference, box, mode, args.scale)
        got = measure(actual, box, mode, args.scale)
        delta = max(abs(a - b) for a, b in zip(expected, got))
        status = "ok" if delta <= 3 else "diff"
        if status == "diff":
            mismatches += 1
        rows.append(
            {
                "probe": name,
                "box": list(box),
                "mode": mode,
                "reference": list(expected),
                "gpui": list(got),
                "max_channel_delta": delta,
                "status": status,
            }
        )

    report = {
        "state": args.state,
        "theme": args.theme,
        "reference": str(args.reference / f"{reference_name}-{args.theme}.png"),
        "actual": str(args.actual / f"{actual_name}-{args.theme}.png"),
        "mismatch_count": mismatches,
        "probe_count": len(rows),
        "probes": rows,
    }
    args.output.mkdir(parents=True, exist_ok=True)
    out = args.output / f"{args.state}-{args.theme}.json"
    out.write_text(json.dumps(report, indent=2) + "\n")
    for row in rows:
        print(
            f"{row['probe']:<24} ref={tuple(row['reference'])!s:<18} "
            f"gpui={tuple(row['gpui'])!s:<18} delta={row['max_channel_delta']:>3} {row['status']}"
        )
    print(f"\n{mismatches}/{len(rows)} probes differ by more than 3; report at {out}")


if __name__ == "__main__":
    main()
