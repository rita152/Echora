#!/usr/bin/env python3
"""Side-by-side and pixel comparison of batch1 reference vs Echora captures.

Both sides are cropped with the same rectangle in their own full-window
coordinates; nothing is scaled or shifted. Writes one PNG per pair plus
report.json with the viewports and the mean absolute channel difference.

Usage: python3 scripts/compare_batch1_captures.py OUT_DIR ref.png:echora.png:x0,y0,x1,y1[:name] ...
"""
import json
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageStat


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    report = []
    for spec in sys.argv[2:]:
        parts = spec.split(":")
        ref_path, echora_path, box = parts[0], parts[1], parts[2]
        name = parts[3] if len(parts) > 3 else Path(echora_path).stem
        box = tuple(int(v) for v in box.split(","))
        ref = Image.open(ref_path).convert("RGB")
        echora = Image.open(echora_path).convert("RGB")
        a, b = ref.crop(box), echora.crop(box)
        diff = ImageChops.difference(a, b)
        mean = sum(ImageStat.Stat(diff).mean) / 3
        sheet = Image.new("RGB", (a.width, a.height * 2 + 4), (255, 0, 255))
        sheet.paste(a, (0, 0))
        sheet.paste(b, (0, a.height + 4))
        sheet.save(out / f"{name}.png")
        report.append({
            "name": name,
            "reference": ref_path,
            "echora": echora_path,
            "referenceSize": ref.size,
            "echoraSize": echora.size,
            "box": box,
            "meanAbsDiff": round(mean, 3),
            "similarity": round(100 * (1 - mean / 255), 2),
        })
    (out / "report.json").write_text(json.dumps(report, indent=1))
    for row in report:
        print(f"{row['name']}: similarity {row['similarity']} (mean diff {row['meanAbsDiff']})")


if __name__ == "__main__":
    main()
