#!/usr/bin/env python3
"""Score the GPUI sidebar activity view against the ChatGPT reference.

`scripts/cdp_capture_activity_view.mjs` writes, for every state and theme, a
screenshot and a spec of the landmarks it measured; `scripts/
capture_activity_view_gpui.sh` writes the same states from the native app. For
each landmark this script finds the translation that best aligns the GPUI edge
map to the reference's inside that box (refined to a tenth of a pixel), so a
landmark that sits where the reference puts it reports 0/0.

The sidebar material is translucent in GPUI's offscreen capture, so raw pixel
differences over the sidebar are not meaningful; edges are.

    python3 scripts/compare_activity_view.py \
        --reference artifacts/activity-view-26917/reference \
        --gpui artifacts/activity-view-26917/gpui \
        --output artifacts/activity-view-26917/compare
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageChops, ImageFilter

STATES = ["default", "hover", "tooltip", "options", "scroll-100", "scroll-200"]


def edges(image: Image.Image) -> np.ndarray:
    return np.asarray(image.convert("L").filter(ImageFilter.FIND_EDGES), dtype=np.float32)


def landmarks(spec: dict, state: str) -> list[tuple[str, list[float]]]:
    found: list[tuple[str, list[float]]] = []

    def add(name: str, rect):
        if rect and rect[2] > 0 and rect[3] > 0:
            found.append((name, rect))

    add("bell", spec["bell"]["rect"])
    if spec["options"]["rect"] and spec["options"]["rect"][1] > 110:
        add("options", spec["options"]["rect"])
    if spec["empty"]["text"] and spec["empty"]["text"][1] > 110:
        add("empty state", spec["empty"]["text"])
    for heading in spec["headings"]:
        title = heading["title"]
        if title and 125 < title[1] < spec["viewport"][1] - 60:
            add(f"heading {heading['text']}", title)
    for index, row in enumerate(spec["rows"][:8]):
        if not row["title"] or row["rect"][1] < 150 or row["rect"][1] > spec["viewport"][1] - 110:
            continue
        add(f"row {index + 1} title", row["title"])
        add(f"row {index + 1} detail", row["detail"])
        if state == "hover" and index == 0:
            add("row 1 box", row["rect"])
            for action_index, action in enumerate(row["actions"]):
                add(f"row 1 action {action_index + 1}", action)
    if spec.get("tooltip"):
        add("tooltip", spec["tooltip"]["rect"])
        add("tooltip label", spec["tooltip"]["label"])
        add("tooltip shortcut", spec["tooltip"]["kbd"])
    if spec.get("menu"):
        add("menu", spec["menu"]["rect"])
        for item in spec["menu"]["items"]:
            add(f"menu {item['text']}", item["label"])
    return found


def alignment(reference: np.ndarray, gpui: np.ndarray, rect, radius: int):
    left, top, width, height = (int(round(value)) for value in rect)
    left, top = left - 2, top - 2
    width, height = width + 4, height + 4
    errors: dict[tuple[int, int], float] = {}
    for dy in range(-radius, radius + 1):
        for dx in range(-radius, radius + 1):
            ref_crop = reference[top : top + height, left : left + width]
            got_crop = gpui[top + dy : top + height + dy, left + dx : left + width + dx]
            if ref_crop.shape != got_crop.shape or ref_crop.size == 0:
                continue
            errors[(dx, dy)] = float(np.mean(np.abs(ref_crop - got_crop)))
    if not errors:
        return None
    (dx, dy), error = min(errors.items(), key=lambda item: (item[1], abs(item[0][0]) + abs(item[0][1])))

    def refine(minus, center, plus):
        denominator = minus - 2 * center + plus
        return 0.0 if denominator <= 0 else max(-0.5, min(0.5, (minus - plus) / (2 * denominator)))

    fx = refine(errors.get((dx - 1, dy), error), error, errors.get((dx + 1, dy), error))
    fy = refine(errors.get((dx, dy - 1), error), error, errors.get((dx, dy + 1), error))
    return dx + fx, dy + fy, error


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True)
    parser.add_argument("--gpui", required=True)
    parser.add_argument("--output")
    parser.add_argument("--theme", action="append")
    parser.add_argument("--radius", type=int, default=6)
    arguments = parser.parse_args()

    reference_dir = Path(arguments.reference)
    gpui_dir = Path(arguments.gpui)
    output = Path(arguments.output) if arguments.output else None
    if output:
        output.mkdir(parents=True, exist_ok=True)
    report = []
    exact = total = 0
    for theme in arguments.theme or ["dark", "light"]:
        for state in STATES:
            name = f"{theme}-{state}"
            reference_png = reference_dir / f"{name}.png"
            gpui_png = gpui_dir / f"{name}.png"
            if not reference_png.exists() or not gpui_png.exists():
                print(f"{name}: missing capture")
                continue
            spec = json.loads((reference_dir / f"{name}.json").read_text())
            reference_image = Image.open(reference_png).convert("RGB")
            gpui_image = Image.open(gpui_png).convert("RGB")
            if reference_image.size != gpui_image.size:
                print(f"{name}: size mismatch {reference_image.size} vs {gpui_image.size}")
                continue
            reference_edges, gpui_edges = edges(reference_image), edges(gpui_image)
            print(f"\n== {name}")
            for label, rect in landmarks(spec, state):
                result = alignment(reference_edges, gpui_edges, rect, arguments.radius)
                if result is None:
                    continue
                dx, dy, error = result
                total += 1
                if abs(dx) < 0.5 and abs(dy) < 0.5:
                    exact += 1
                report.append({"capture": name, "landmark": label, "rect": rect, "dx": dx, "dy": dy, "edge_error": error})
                flag = "" if abs(dx) < 0.5 and abs(dy) < 0.5 else "  <-"
                print(f"  {label:<28} dx={dx:+5.1f} dy={dy:+5.1f} edge={error:6.2f}{flag}")
            if output:
                width = 480
                crop = (0, 0, width, reference_image.height)
                left = reference_image.crop(crop)
                right = gpui_image.crop(crop)
                diff = ImageChops.difference(edges_image(left), edges_image(right))
                sheet = Image.new("RGB", (width * 3 + 20, reference_image.height), (255, 0, 0))
                sheet.paste(left, (0, 0))
                sheet.paste(right, (width + 10, 0))
                sheet.paste(diff.convert("RGB"), (2 * width + 20, 0))
                sheet.save(output / f"{name}.png")
    print(f"\n{exact}/{total} landmarks align with the reference to within half a pixel")
    if output:
        (output / "landmarks.json").write_text(json.dumps(report, indent=1) + "\n")
    return 0


def edges_image(image: Image.Image) -> Image.Image:
    return image.convert("L").filter(ImageFilter.FIND_EDGES)


if __name__ == "__main__":
    raise SystemExit(main())
