#!/usr/bin/env python3
"""Reference vs Echora comparison for the batch-four captures.

Both sides are 1470x923 @2 windows of the same fixture threads, and the
summary panel sits at the same place on both (52 px from the top, 6 px from
the right), so each pair is compared on one rectangle cropped from both frames
at the same coordinates: nothing is scaled or shifted. The rectangle is the
reference element's own box from its `.dom.json` (the island, the menu, the
toast, the hover card), padded by 8 device pixels, or a fixed region for whole
areas (the sidebar, the chat column, the window).

Writes one sheet per pair (reference above, Echora below) plus report.json
into OUT_DIR, and prints each pair's mean-absolute-difference similarity.

Usage: python3 scripts/compare_batch4_captures.py [DATE] [OUT_DIR]
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageStat

DATE = sys.argv[1] if len(sys.argv) > 1 else "20260929"
OUT = Path(sys.argv[2] if len(sys.argv) > 2 else f"artifacts/batch4-compare-{DATE}")
A = "artifacts"
DPR = 2
PAD = 8

# Fixed regions in CSS pixels (x, y, width, height).
SIDEBAR = (0, 0, 240, 923)
CHAT = (240, 52, 1230 - 306, 871)
TOP_RIGHT = (900, 0, 570, 360)
WINDOW = (0, 0, 1470, 923)

# (topic, name, region): "scope" is the reference element's box.
PAIRS = [
    ("chips", "sidebar", SIDEBAR),
    ("chips", "hover-failing", "scope"),
    ("chips", "hover-merged", "scope"),
    ("panel", "open", "scope"),
    ("panel", "section-hover", "scope"),
    ("panel", "closed", TOP_RIGHT),
    ("panel", "reopened", "scope"),
    ("panel", "pr-row-hover", "scope"),
    ("panel", "pr-actions-menu", "scope"),
    ("panel", "unmatched-pr-hover", "scope"),
    ("background", "section", "scope"),
    ("background", "row-hover", "scope"),
    ("background", "row-focus", "scope"),
    ("background", "stopping", "scope"),
    ("background", "stop-failed", "scope"),
    ("background", "card-running", CHAT),
    ("background", "card-stopped", CHAT),
    ("background", "card-finished", CHAT),
    ("background", "terminal-tab", WINDOW),
]


def region(topic, name, theme, spec):
    if spec == "scope":
        dom = json.loads(Path(f"{A}/batch4-{topic}-{DATE}/reference/{name}-{theme}.png.dom.json").read_text())
        x, y, w, h = dom["scope"]
    else:
        x, y, w, h = spec
    box = [round(x * DPR) - PAD, round(y * DPR) - PAD, round((x + w) * DPR) + PAD, round((y + h) * DPR) + PAD]
    return tuple(max(0, value) for value in box)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    report = []
    for topic, name, spec in PAIRS:
        for theme in ("dark", "light"):
            ref_path = f"{A}/batch4-{topic}-{DATE}/reference/{name}-{theme}.png"
            echora_path = f"{A}/batch4-{topic}-{DATE}/echora/{name}-{theme}.png"
            row = {"name": f"{topic}-{name}-{theme}", "reference": ref_path, "echora": echora_path}
            if not Path(echora_path).exists():
                row["missing"] = "echora"
                report.append(row)
                continue
            ref = Image.open(ref_path).convert("RGB")
            echora = Image.open(echora_path).convert("RGB")
            assert ref.size == echora.size, (ref_path, ref.size, echora.size)
            box = region(topic, name, theme, spec)
            box = (box[0], box[1], min(box[2], ref.width), min(box[3], ref.height))
            a, b = ref.crop(box), echora.crop(box)
            mean = sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3
            row.update(size=ref.size, box=box, meanAbsDiff=round(mean, 3),
                       similarity=round(100 * (1 - mean / 255), 2))
            sheet = Image.new("RGB", (a.width, a.height * 2 + 4), (255, 0, 255))
            sheet.paste(a, (0, 0))
            sheet.paste(b, (0, a.height + 4))
            sheet.save(OUT / f"{topic}-{name}-{theme}.png")
            report.append(row)
    (OUT / "report.json").write_text(json.dumps(report, indent=1))
    for row in report:
        if "missing" in row:
            print(f"{row['name']}: missing {row['missing']} capture")
        else:
            print(f"{row['name']}: similarity {row['similarity']} over {row['box']}")


if __name__ == "__main__":
    main()
