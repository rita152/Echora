#!/usr/bin/env python3
"""Paste the in-app browser's native page into a GPUI capture.

GPUI's `render_to_image` only rasterizes GPUI's own scene; the Browser's page
is a WKWebView above it. A capture run with `--browser-page-snapshot=<page>`
writes that page as `<page>` plus `<page>.json` (its frame in window points,
the radius of its bottom corners, and the holes GPUI overlays cut into it).
This script composites the two:

    scripts/compose_browser_capture.py <capture.png> <page.png> <out.png>
"""

import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    capture_path, page_path, out_path = map(Path, sys.argv[1:])
    capture = Image.open(capture_path).convert("RGBA")
    page = Image.open(page_path).convert("RGBA")
    frame = json.loads(Path(f"{page_path}.json").read_text())
    meta = json.loads(Path(f"{capture_path}.render.json").read_text())
    scale = float(meta.get("dpr", 1.0))
    x, y = round(frame["x"] * scale), round(frame["y"] * scale)
    width, height = round(frame["width"] * scale), round(frame["height"] * scale)
    page = page.resize((width, height), Image.LANCZOS)
    radius = round(frame.get("bottomCornerRadius", 0) * scale)
    mask = Image.new("L", (width, height), 255)
    if radius > 0:
        draw = ImageDraw.Draw(mask)
        # Square top corners, rounded bottom ones, as the native clip view.
        draw.rectangle((0, 0, width, height), fill=0)
        draw.rounded_rectangle((0, 0, width - 1, height - 1), radius=radius, fill=255)
        draw.rectangle((0, 0, width - 1, height - 1 - radius), fill=255)
    # GPUI overlays (menus, the address suggestions, the find bar) cut holes
    # into the page, as the native view's mask does.
    draw = ImageDraw.Draw(mask)
    for hole in frame.get("holes", []):
        left = round(hole["x"] * scale) - x
        top = round(hole["y"] * scale) - y
        right = left + round(hole["width"] * scale) - 1
        bottom = top + round(hole["height"] * scale) - 1
        corner = round(hole.get("cornerRadius", 0) * scale)
        draw.rounded_rectangle((left, top, right, bottom), radius=corner, fill=0)
    capture.paste(page, (x, y), mask)
    capture.convert("RGB").save(out_path)
    print(out_path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
