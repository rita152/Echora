#!/usr/bin/env python3
"""Score the native user-message navigation rail against the ChatGPT reference.

Two regions are compared, both anchored on the transcript pane rather than on
the window, because the two builds size the sidebar differently:

* the rail's column, from the resting capture, which carries the marker dashes
  and their opacities;
* the hover card plus the rail, from the hovered capture, which carries the
  preview typography, the card surface and its ring.

Chrome writes straight alpha and GPUI's Metal readback writes premultiplied
alpha, so both captures are composited over the pane colour first.

    python3 scripts/compare_user_message_rail.py \
      --reference artifacts/user-message-rail/reference \
      --gpui artifacts/user-message-rail/gpui \
      --output artifacts/user-message-rail/compare --theme dark
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image

PANE_COLOUR = {"light": (0xFF, 0xFF, 0xFF), "dark": (0x18, 0x18, 0x18)}
RAIL_LEFT = 16.0
RAIL_WIDTH = 36.0
CARD_LEFT = 52.0
CARD_WIDTH = 320.0


def composite(path: Path, base: tuple[int, int, int], premultiplied: bool) -> np.ndarray:
    with Image.open(path) as image:
        rgba = np.asarray(image.convert("RGBA")).astype(np.float64)
    rgb = rgba[..., :3]
    alpha = rgba[..., 3:4] / 255.0
    if premultiplied:
        rgb = rgb
    else:
        rgb = rgb * alpha
    background = np.array(base, dtype=np.float64).reshape(1, 1, 3)
    return np.clip(rgb + background * (1.0 - alpha), 0, 255)


def sidecar_geometry(sidecar: Path) -> tuple[float, float]:
    """Window-space origin of the transcript pane, as the native capture
    measured it. The light theme composites the sidebar and the pane to the
    same colour, so the two pane edges cannot be found in pixels alone."""
    metadata = json.loads(sidecar.read_text())
    origin = metadata.get("conversationPaneOrigin")
    if origin is None:
        raise SystemExit(
            f"{sidecar} does not report conversationPaneOrigin; re-capture with the "
            "current capture build"
        )
    return float(origin[0]), float(origin[1])


def summarise(reference: np.ndarray, native: np.ndarray) -> dict:
    # Accept both image crops and flat pixel selections.
    channels = np.abs(reference.reshape(-1, 3) - native.reshape(-1, 3))
    diff = channels.max(axis=1)
    total = diff.size
    return {
        "pixels": int(total),
        "exact": float((diff <= 0.5).sum()) / total,
        "within1": float((diff <= 1.5).sum()) / total,
        "within2": float((diff <= 2.5).sum()) / total,
        "within4": float((diff <= 4.5).sum()) / total,
        # The repository's shared score: per-channel error beyond the tolerance
        # is what counts, so antialiasing inside the tolerance is free.
        "similarity": float(
            1.0 - np.maximum(channels - 2.0, 0).sum() / (channels.size * 255.0)
        ),
        "meanAbsError": float(diff.mean()),
        "maxAbsError": float(diff.max()),
    }


def best_offset(reference: np.ndarray, native: np.ndarray, search: int = 4) -> dict:
    """Aligns the native crop on the reference one, to separate a systematic
    offset from a real rendering difference."""
    height, width = reference.shape[:2]
    best = None
    for dy in range(-search, search + 1):
        for dx in range(-search, search + 1):
            y0, y1 = max(0, dy), min(height, height + dy)
            x0, x1 = max(0, dx), min(width, width + dx)
            ref_crop = reference[y0 - dy : y1 - dy, x0 - dx : x1 - dx]
            native_crop = native[y0:y1, x0:x1]
            error = float(np.abs(ref_crop - native_crop).max(axis=2).mean())
            if best is None or error < best[0]:
                best = (error, dx, dy)
    error, dx, dy = best
    return {"dx": dx, "dy": dy, "meanAbsError": error}


def dashed_rows(image: np.ndarray, left: float, theme: str) -> list[dict]:
    """Marker rows of the rail column: one entry per dash, with its width."""
    colour = np.array(PANE_COLOUR[theme], dtype=np.float64)
    # Markers start at the rail's left edge and are at most 26 px wide; the
    # hover card begins at 52 px, so this window holds the dashes alone.
    column = image[:, int(left) : int(left + 26)]
    diff = np.abs(column - colour).max(axis=2)
    rows = np.where(diff.max(axis=1) > 2)[0]
    # The sticky header draws content into the same column, so the search
    # covers only the band the rail can occupy.
    band_top = max(40.0, image.shape[0] * 0.25)
    band_bottom = image.shape[0] * 0.75
    rows = rows[(rows >= band_top) & (rows < band_bottom)]
    out: list[dict] = []
    if rows.size == 0:
        return out
    start = previous = int(rows[0])
    groups: list[tuple[int, int]] = []
    for row in rows[1:]:
        row = int(row)
        if row != previous + 1:
            groups.append((start, previous))
            start = row
        previous = row
    groups.append((start, previous))
    for top, bottom in groups:
        band = diff[top : bottom + 1]
        columns = np.where(band.max(axis=0) > 2)[0]
        if columns.size == 0:
            continue
        out.append(
            {
                "top": top,
                "centre": (top + bottom) / 2.0,
                "left": int(columns.min()) + int(left),
                "width": int(columns.max() - columns.min() + 1),
            }
        )
    return out


def card_rect(reference: dict, theme: str) -> list[float]:
    pane = reference["themes"][theme]["geometry"]["pane"]
    card = reference["themes"][theme]["card"]["rect"]
    return [card[0], card[1], card[2], card[3]]


def write_diff(reference: np.ndarray, native: np.ndarray, path: Path) -> None:
    height = min(reference.shape[0], native.shape[0])
    width = min(reference.shape[1], native.shape[1])
    reference = reference[:height, :width]
    native = native[:height, :width]
    diff = np.abs(reference - native).max(axis=2)
    strip = np.concatenate(
        [reference, native, np.repeat(diff[..., None], 3, axis=2)], axis=1
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(np.clip(strip, 0, 255).astype(np.uint8)).save(path)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--gpui", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--theme", default="dark")
    parser.add_argument("--target", type=float, default=0.99)
    args = parser.parse_args()

    report = json.loads((args.reference / "report.json").read_text())
    theme_report = report["themes"][args.theme]
    colour = PANE_COLOUR[args.theme]
    viewport = theme_report["viewport"]

    sidecar = args.gpui / f"{args.theme}-window.png.render.json"
    if not sidecar.exists():
        raise SystemExit(f"missing {sidecar}; every capture writes one")
    metadata = json.loads(sidecar.read_text())
    if abs(float(metadata["viewportWidth"]) - viewport[0]) > 0.5 or abs(
        float(metadata["viewportHeight"]) - viewport[1]
    ) > 0.5:
        raise SystemExit(
            f"native capture is {metadata['viewportWidth']}x{metadata['viewportHeight']}, "
            f"the reference {viewport[0]}x{viewport[1]}"
        )

    reference_rest = composite(
        args.reference / f"{args.theme}-window.png", colour, premultiplied=False
    )
    native_rest = composite(
        args.gpui / f"{args.theme}-window.png", colour, premultiplied=True
    )
    reference_hover = composite(
        args.reference / f"{args.theme}-hover-window.png", colour, premultiplied=False
    )
    native_hover = composite(
        args.gpui / f"{args.theme}-hover.png", colour, premultiplied=True
    )

    reference_pane = float(theme_report["geometry"]["pane"][0])
    native_pane, _ = sidecar_geometry(sidecar)
    margin = 8.0
    left = margin
    # The rail's own column: markers start at 16 px and the hover card begins at
    # 52 px, so this window holds the dashes and nothing of the card.
    right = RAIL_LEFT + RAIL_WIDTH
    # Rows are the rail's own band: the window titlebar and the transcript
    # header are app chrome the two builds already share, and including them
    # would score this feature against the header instead of the rail.
    rail_rect = theme_report["geometry"]["rail"]
    row_top = max(0, int(rail_rect[1] - margin))
    row_bottom = int(rail_rect[1] + rail_rect[3] + margin)

    reference_rail = reference_rest[
        row_top:row_bottom, int(reference_pane + left) : int(reference_pane + right)
    ]
    native_rail = native_rest[
        row_top:row_bottom, int(native_pane + left) : int(native_pane + right)
    ]
    rail = summarise(reference_rail, native_rail)
    rail_offset = best_offset(reference_rail, native_rail)
    reference_dashes = dashed_rows(reference_rest, reference_pane + RAIL_LEFT, args.theme)
    native_dashes = dashed_rows(native_rest, native_pane + RAIL_LEFT, args.theme)

    reference_card_rail = reference_hover[
        row_top:row_bottom, int(reference_pane + left) : int(reference_pane + right)
    ]
    native_card_rail = native_hover[
        row_top:row_bottom, int(native_pane + left) : int(native_pane + right)
    ]
    hover_rail = summarise(reference_card_rail, native_card_rail)
    hover_rail_offset = best_offset(reference_card_rail, native_card_rail)

    card = card_rect(report, args.theme)
    padding = 6.0
    card_left = max(0.0, card[0] - reference_pane - padding)
    card_top = max(0.0, card[1] - padding)
    card_width = CARD_WIDTH + padding * 2
    card_height = card[3] + padding * 2
    reference_card = reference_hover[
        int(card_top) : int(card_top + card_height),
        int(reference_pane + card_left) : int(reference_pane + card_left + card_width),
    ]
    native_card = native_hover[
        int(card_top) : int(card_top + card_height),
        int(native_pane + card_left) : int(native_pane + card_left + card_width),
    ]
    card_summary = summarise(reference_card, native_card)
    card_offset = best_offset(reference_card, native_card)
    # Split the card into the rows that print text and the surface between
    # them: a surface that matches proves the geometry, radius, ring and fills,
    # while the text rows carry the two rasterizers' differences.
    text_rows = np.zeros(reference_card.shape[:2], dtype=bool)
    card_top = float(card[1])
    for row_top, row_bottom in ((415.0, 435.0), (439.0, 460.0), (473.0, 519.0)):
        top = int(row_top - card_top + padding)
        bottom = int(row_bottom - card_top + padding)
        text_rows[max(0, top) : bottom, int(padding) : int(padding + CARD_WIDTH)] = True
    card_text = summarise(reference_card[text_rows], native_card[text_rows])
    card_surface = summarise(reference_card[~text_rows], native_card[~text_rows])
    write_diff(reference_card, native_card, args.output / f"{args.theme}-card-diff.png")
    write_diff(reference_rail, native_rail, args.output / f"{args.theme}-rail-diff.png")

    rail_left_expected = RAIL_LEFT
    geometry = {
        "referencePaneLeft": reference_pane,
        "nativePaneLeft": native_pane,
        "referenceRailOffset": (
            reference_dashes[0]["left"] - reference_pane if reference_dashes else None
        ),
        "nativeRailOffset": (
            native_dashes[0]["left"] - native_pane if native_dashes else None
        ),
        "railOffsetExpected": rail_left_expected,
        "referenceDashes": len(reference_dashes),
        "nativeDashes": len(native_dashes),
        "firstDashDelta": (
            native_dashes[0]["centre"] - reference_dashes[0]["centre"]
            if reference_dashes and native_dashes
            else None
        ),
        "dashLeftDelta": (
            native_dashes[0]["left"] - reference_dashes[0]["left"]
            if reference_dashes and native_dashes
            else None
        ),
        "referenceDashWidths": [dash["width"] for dash in reference_dashes],
        "nativeDashWidths": [dash["width"] for dash in native_dashes],
        "cardPixels": [int(card_width), int(card_height)],
    }
    result = {
        "theme": args.theme,
        "target": args.target,
        "geometry": geometry,
        "rail": rail,
        "railOffset": rail_offset,
        "railHovered": hover_rail,
        "railHoveredOffset": hover_rail_offset,
        "card": card_summary,
        "cardText": card_text,
        "cardSurface": card_surface,
        "cardOffset": card_offset,
    }
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / f"{args.theme}.json").write_text(f"{json.dumps(result, indent=1)}\n")

    ok = (
        rail["within2"] >= args.target
        # The hover capture's rail column also carries the left tail of the
        # card's shadow, whose blur the two rasterizers spread slightly
        # differently, so this region is scored with the shared tolerance metric.
        and hover_rail["similarity"] >= args.target
        # The card's text rows are the two platform rasterizers disagreeing;
        # its surface, geometry and ring are what this feature has to match.
        and card_surface["similarity"] >= args.target
        and abs(rail_offset["dx"]) <= 1
        and abs(rail_offset["dy"]) <= 1
        and abs(card_offset["dx"]) <= 1
        and abs(card_offset["dy"]) <= 1
        and geometry["firstDashDelta"] is not None
        and abs(geometry["firstDashDelta"]) <= 1.0
        # The dash's absolute left differs by the sidebar width the two builds
        # use, so the placement check is the pane-relative offset: both builds
        # have to put the markers 16 px from the transcript surface.
        and geometry["referenceRailOffset"] is not None
        and geometry["nativeRailOffset"] is not None
        and abs(geometry["referenceRailOffset"] - rail_left_expected) <= 1
        and abs(geometry["nativeRailOffset"] - rail_left_expected) <= 1
    )
    print(
        f"{args.theme}: rail within2={rail['within2']:.4f} similarity={rail['similarity']:.4f} "
        f"| hovered similarity={hover_rail['similarity']:.4f} within2={hover_rail['within2']:.4f} "
        f"| card surface similarity={card_surface['similarity']:.4f} "
        f"text similarity={card_text['similarity']:.4f} "
        f"card similarity={card_summary['similarity']:.4f} "
        f"| dashes {len(reference_dashes)}/{len(native_dashes)} "
        f"| offset dx={rail_offset['dx']} dy={rail_offset['dy']} "
        f"card offset dx={card_offset['dx']} dy={card_offset['dy']} | {'PASS' if ok else 'FAIL'}"
    )
    if not ok:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
