#!/usr/bin/env python3
"""Reference vs Echora comparison for the batch-three captures.

Nothing is scaled or shifted. Each pair gets two kinds of result:

- geometry: on each side independently, the panel (menu, dialog or settings
  card) found along a probe line inside that side's search window, and the
  text bands inside it relative to the panel's top edge. The reference chat
  column sits beside a project panel, so the composer popups and dialogs start
  at different x; the measurement follows each side's own panel instead of
  moving either image.
- pixels: where both frames put the element at the same place (the settings
  pages), the same rectangle is cropped from both and diffed, as
  compare_batch1_captures.py does.

Writes one sheet per pair (reference above, Echora below, each cropped to its
own panel) plus report.json into OUT_DIR.

Usage: python3 scripts/compare_batch3_captures.py [DATE] [OUT_DIR]
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageStat

DATE = sys.argv[1] if len(sys.argv) > 1 else "20260929"
OUT = Path(sys.argv[2] if len(sys.argv) > 2 else f"artifacts/batch3-compare-{DATE}")
A = "artifacts"
# Longer than any corner radius at @2, shorter than the gap to other content.
CORNER_GAP = 40
# Above the composer's upward shadow (at most 9 levels off the page), below
# any panel border (18 or more).
INK = 12

# (name, reference, echora, probe per side (x, y0, y1), same-box or None).
# The batch-three reference has no Chat memories dialog (memories are off in
# the reference profile, so `/mem` never opened it); the memory states are
# measured against batch two's reference of the same dialog and settings card,
# and differ from it by exactly the added status row.
PAIRS = [
    ("review-slash", "review", "slash", "slash", ((1400, 1300, 1612), (1700, 1300, 1612)), None),
    ("review-submenu", "review", "submenu", "submenu", ((1400, 900, 1612), (1700, 900, 1612)), None),
    ("review-submenu-branch", "review", "submenu-branch", "submenu-branch",
     ((1400, 900, 1612), (1700, 900, 1612)), None),
    ("review-submenu-escaped", "review", "submenu-escaped", "submenu-escaped",
     ((1400, 1300, 1612), (1700, 1300, 1612)), None),
    ("review-git-inline", "review", "git-inline", "git-inline",
     ((1500, 240, 880), (1500, 240, 880)), (930, 740, 2490, 870)),
    ("review-git-detached", "review", "git-detached", "git-detached",
     ((1500, 240, 880), (1500, 240, 880)), (930, 740, 2490, 870)),
    ("capabilities-web-search", "capabilities", "web-search", "web-search-unsupported",
     ((1500, 680, 1320), (1500, 480, 1110)), None),
    ("memory-settings", "memory", "settings-on", "settings-ready",
     ((1500, 350, 890), (1500, 350, 890)), (930, 350, 2490, 730)),
    ("memory-dialog-started", "memory", "dialog-started", "dialog-started-ready",
     # The reference dialog sits over chat text; its edges are walked along
     # a row with nothing beside the dialog on either side.
     ((1470, 490, 1370, 600, 700, 2300), (1470, 440, 1370, 600, 700, 2300)), None),
]


def reference_path(topic, name, theme):
    if topic == "memory":
        return f"{A}/batch2-memories-20260928/reference/{name}-{theme}.png"
    return f"{A}/batch3-{topic}-{DATE}/reference/{name}-{theme}.png"


def luminance(image):
    return image.convert("L")


def panel(image, probe):
    """The outermost pixels differing from the page along a vertical probe
    line (top and bottom edges); then, walking out from the probe along the
    panel's top, middle and bottom lines, the last column any of them marks
    (left and right edges; short gaps are the rounded corners, and in the
    light theme the fill equals the page). None when the window holds only
    page. A probe may instead name one clean row and its x range."""
    x, y0, y1, *across = probe
    gray = luminance(image)
    page = gray.getpixel((x, y0))

    def ink(px, py):
        return abs(gray.getpixel((px, py)) - page) > INK

    hits = [y for y in range(y0, y1) if ink(x, y)]
    if not hits:
        return None
    top, bottom = hits[0], hits[-1] + 1
    if across:
        # (row, x0, x1): the first and last ink along one clean row.
        row, x0, x1 = across
        marked = [px for px in range(x0, x1) if ink(px, row)]
        return (marked[0], top, marked[-1] + 1, bottom)
    lines = (top, (top + bottom) // 2, bottom - 1)

    def edge(step):
        last, px, gap = x, x, 0
        while 0 <= px + step < image.width and gap <= CORNER_GAP:
            px += step
            if any(ink(px, py) for py in lines):
                last, gap = px, 0
            else:
                gap += 1
        return last

    return (edge(-1), top, edge(1) + 1, bottom)


def bands(image, box):
    """Text bands inside the panel: rows with a pixel far from the fill."""
    left, top, right, bottom = box
    inner = luminance(image).crop((left + 8, top + 4, right - 8, bottom - 4))
    values = sorted(inner.get_flattened_data())
    fill = values[len(values) // 2]
    rows = []
    for y in range(inner.height):
        line = inner.crop((0, y, inner.width, y + 1)).get_flattened_data()
        rows.append(any(abs(value - fill) > 50 for value in line))
    found, start = [], None
    for y, ink in enumerate(rows + [False]):
        if ink and start is None:
            start = y
        elif not ink and start is not None:
            if y - start >= 6:
                found.append([start + 4, y - start])
            start = None
    return found


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    report = []
    for name, topic, ref_name, echora_name, probes, same_box in PAIRS:
        for theme in ("dark", "light"):
            ref_path = reference_path(topic, ref_name, theme)
            echora_path = f"{A}/batch3-{topic}-{DATE}/echora/{echora_name}-{theme}.png"
            ref = Image.open(ref_path).convert("RGB")
            echora = Image.open(echora_path).convert("RGB")
            assert ref.size == echora.size, (ref_path, ref.size, echora.size)
            row = {
                "name": f"{name}-{theme}",
                "reference": ref_path,
                "echora": echora_path,
                "size": ref.size,
            }
            boxes = [panel(ref, probes[0]), panel(echora, probes[1])]
            for side, image, box in (("reference", ref, boxes[0]), ("echora", echora, boxes[1])):
                row[side] = None if box is None else {
                    "panel": box,
                    "panelSize": [box[2] - box[0], box[3] - box[1]],
                    "bands": bands(image, box),
                }
            if all(boxes):
                row["panelSizeDelta"] = [
                    boxes[1][2] - boxes[1][0] - (boxes[0][2] - boxes[0][0]),
                    boxes[1][3] - boxes[1][1] - (boxes[0][3] - boxes[0][1]),
                ]
                crops = [ref.crop(boxes[0]), echora.crop(boxes[1])]
                width = max(c.width for c in crops)
                sheet = Image.new("RGB", (width, crops[0].height + crops[1].height + 4), (255, 0, 255))
                sheet.paste(crops[0], (0, 0))
                sheet.paste(crops[1], (0, crops[0].height + 4))
                sheet.save(OUT / f"{name}-{theme}.png")
            if same_box:
                a, b = ref.crop(same_box), echora.crop(same_box)
                mean = sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3
                row["sameBox"] = same_box
                row["meanAbsDiff"] = round(mean, 3)
                row["similarity"] = round(100 * (1 - mean / 255), 2)
                sheet = Image.new("RGB", (a.width, a.height * 2 + 4), (255, 0, 255))
                sheet.paste(a, (0, 0))
                sheet.paste(b, (0, a.height + 4))
                sheet.save(OUT / f"{name}-{theme}-same-box.png")
            report.append(row)
    (OUT / "report.json").write_text(json.dumps(report, indent=1))
    for row in report:
        sizes = " vs ".join(
            "none" if row[side] is None else "x".join(map(str, row[side]["panelSize"]))
            for side in ("reference", "echora")
        )
        pitch = " vs ".join(
            "-" if row[side] is None else str(len(row[side]["bands"])) for side in ("reference", "echora")
        )
        similarity = f", same-box similarity {row['similarity']}" if "similarity" in row else ""
        print(f"{row['name']}: panel {sizes}, text bands {pitch}{similarity}")


if __name__ == "__main__":
    main()
