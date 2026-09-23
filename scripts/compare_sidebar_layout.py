#!/usr/bin/env python3
"""Score the GPUI sidebar against the ChatGPT reference, row by row.

The reference capture writes a spec of every landmark it measured (see
`scripts/cdp_capture_sidebar_layout.mjs`). This script reuses those rectangles:
for each one it looks for the translation that best aligns the GPUI pixels to
the reference pixels inside that window, and prints the offset. A row that sits
exactly where the reference puts it reports 0/0, so the numbers say which part
of the sidebar is still off and by how far — no eyeballing required.

    python3 scripts/compare_sidebar_layout.py \
        --reference artifacts/sidebar-layout/reference/dark-window.png \
        --gpui artifacts/sidebar-layout/gpui/dark.png \
        --spec artifacts/sidebar-layout/reference/dark-spec.json
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

# Landmarks worth aligning, in the order the sidebar renders them. Each entry is
# (spec path, display name); the spec path walks `landmarks` then `lists`.
LANDMARKS = [
    ("landmarks.brandButton", "brand"),
    ("landmarks.searchButton", "search"),
    ("landmarks.activityButton", "activity"),
    ("landmarks.newChatRow", "new chat"),
    ("landmarks.pullRequestsRow", "pull requests"),
    ("landmarks.scheduledRow", "scheduled"),
    ("landmarks.pluginsRow", "plugins"),
    ("landmarks.projectsHeadingRow", "projects heading row"),
    ("landmarks.recentsHeadingRow", "recents heading row"),
    ("landmarks.profileButton", "profile"),
    ("landmarks.helpButton", "help"),
    ("lists.projectRows.0", "project row 1"),
    ("lists.projectRows.1", "project row 2"),
    ("lists.threadRows.0", "task row 1"),
    ("lists.threadRows.1", "task row 2"),
    ("lists.threadRows.2", "task row 3"),
    ("lists.threadRows.5", "task row 6"),
    ("lists.threadRows.6", "task row 7"),
    ("lists.threadRows.10", "task row 11"),
]


def resolve(spec: dict, path: str):
    node = spec
    for part in path.split("."):
        if node is None:
            return None
        node = node[int(part)] if part.isdigit() else node.get(part)
    return node


def edges(path: Path) -> np.ndarray:
    image = Image.open(path).convert("L").filter(ImageFilter.FIND_EDGES)
    return np.asarray(image, dtype=np.float32)


def alignment(reference: np.ndarray, gpui: np.ndarray, rect, radius: int):
    left, top, width, height = (int(round(value)) for value in rect)
    best = None
    for dy in range(-radius, radius + 1):
        for dx in range(-radius, radius + 1):
            ref_crop = reference[top : top + height, left : left + width]
            got_crop = gpui[top + dy : top + height + dy, left + dx : left + width + dx]
            if ref_crop.shape != got_crop.shape or ref_crop.size == 0:
                continue
            error = float(np.mean(np.abs(ref_crop - got_crop)))
            candidate = (error, abs(dx) + abs(dy), dx, dy)
            if best is None or candidate < best:
                best = candidate
    return best


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--gpui", required=True)
    parser.add_argument("--spec", required=True)
    parser.add_argument("--radius", type=int, default=6)
    parser.add_argument(
        "--scale",
        type=float,
        default=1.0,
        help="device pixels per CSS pixel in the captures (the spec rects are CSS px)",
    )
    parser.add_argument("--output")
    arguments = parser.parse_args()

    spec = json.loads(Path(arguments.spec).read_text())
    reference = edges(Path(arguments.reference))
    gpui = edges(Path(arguments.gpui))
    if reference.shape != gpui.shape:
        raise SystemExit(f"size mismatch: {reference.shape} vs {gpui.shape}")

    results = []
    print(f"{'landmark':<20} {'reference rect':>28} {'dx':>4} {'dy':>4} {'edge err':>9}")
    for path, name in LANDMARKS:
        landmark = resolve(spec, path)
        if landmark is None:
            print(f"{name:<20} {'(missing in reference)':>28}")
            continue
        rect = landmark["rect"]
        scaled = [value * arguments.scale for value in rect]
        best = alignment(reference, gpui, scaled, arguments.radius)
        if best is None:
            print(f"{name:<20} {'(no overlap)':>28}")
            continue
        error, _, dx, dy = best
        results.append({"name": name, "rect": scaled, "dx": dx, "dy": dy, "edge_error": error})
        print(
            f"{name:<20} {str([round(v) for v in scaled]):>28} {dx:>4} {dy:>4} {error:>9.2f}"
        )

    overlapping = [row for row in results if row["dx"] == 0 and row["dy"] == 0]
    print(
        f"\n{len(overlapping)}/{len(results)} landmarks already sit exactly on the reference box"
    )
    if arguments.output:
        Path(arguments.output).write_text(json.dumps(results, indent=1) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
