#!/usr/bin/env python3
"""Score the native rail's motion against the reference recording.

Both recordings come from the same pointer script:
`scripts/cdp_probe_user_message_rail_motion.mjs` drives the reference over CDP
and `--user-message-rail-motion` drives the packaged native build. This reads
the key moments out of both timelines (card open and close, taper settle, the
skip-delay reopen, smooth-scroll durations, the highlight, the scrub's
`aria-current` sequence, and wheel routing) and prints them side by side.

Usage:
  scripts/compare_user_message_rail_motion.py \
    --reference artifacts/user-message-rail-motion/reference/motion.json \
    --gpui artifacts/user-message-rail-motion/gpui/motion.json \
    [--output artifacts/user-message-rail-motion/compare/report.json]
"""

import argparse
import json
import math
import re
import sys
from pathlib import Path

# Frame-to-frame jitter plus one frame of render latency on either side.
TIMING_TOLERANCE_MS = 40.0
# Chromium's smooth-scroll end is read off a curve that flattens out.
SCROLL_DURATION_TOLERANCE = 0.15
HIGHLIGHT_REST = 0.05


def mark(scenario, name):
    return next(m["t"] for m in scenario["marks"] if m["name"] == name)


def first_frame(scenario, after, predicate):
    for frame in scenario["frames"]:
        if frame["t"] >= after and predicate(frame):
            return frame["t"]
    return None


def card_open(frame):
    return frame["card"] is not None


def hovered(frame):
    return [i for i, colour in enumerate(frame["colours"]) if colour.endswith("@1")]


def settled(target):
    return lambda frame: all(
        abs(width - want) < 0.1 for width, want in zip(frame["widths"], target)
    )


def taper(index, count):
    widths = [26.0, 20.0, 14.0, 10.0]
    return [widths[abs(i - index)] if abs(i - index) < 4 else 6.0 for i in range(count)]


def since(value, origin):
    return None if value is None else round(value - origin, 1)


def highlight_mix(value):
    """The highlight as its `color-mix` share of the text colour."""
    if value is None:
        return None
    if isinstance(value, str):
        match = re.search(r"/ ([0-9.]+)\)", value)
        return float(match.group(1)) if match else None
    # The native build records the opacity of its overlay layer.
    return 1.0 - (1.0 - value) * (1.0 - HIGHLIGHT_REST)


def hover_metrics(scenarios):
    out = {}
    enter = scenarios["hoverEnter"]
    count = len(enter["frames"][0]["widths"])
    origin = mark(enter, "enter-marker-4")
    out["open after entering (ms)"] = since(first_frame(enter, origin, card_open), origin)
    out["taper settled (ms)"] = since(first_frame(enter, origin, settled(taper(3, count))), origin)

    sweep = scenarios["hoverSweep"]
    origin = mark(sweep, "enter-marker-2")
    out["open while sweeping (ms)"] = since(first_frame(sweep, origin, card_open), origin)
    labels = []
    for frame in sweep["frames"]:
        label = frame["card"]["label"][:12] if frame["card"] else None
        if label and (not labels or labels[-1] != label):
            labels.append(label)
    out["card follows markers"] = labels

    leave = scenarios["hoverLeaveAndReturn"]
    closed = lambda frame: frame["card"] is None
    origin = mark(leave, "leave-left")
    out["close after leaving (ms)"] = since(first_frame(leave, origin, closed), origin)
    origin = mark(leave, "reenter-after-250ms")
    out["reopen inside skip window (ms)"] = since(first_frame(leave, origin, card_open), origin)
    origin = mark(leave, "reenter-after-700ms")
    out["reopen after skip window (ms)"] = since(first_frame(leave, origin, card_open), origin)

    to_card = scenarios.get("hoverToCard")
    if to_card:
        travel = mark(to_card, "travel-to-card")
        leave_card = mark(to_card, "leave-card-right")
        out["stays open on the way to the card"] = all(
            frame["card"] is not None
            for frame in to_card["frames"]
            if travel <= frame["t"] < leave_card
        )
        out["close after leaving the card (ms)"] = since(
            first_frame(to_card, leave_card, closed), leave_card
        )

    gap = scenarios["hoverStopInGap"]
    origin = mark(gap, "leave-right-into-gap")
    out["close when resting in the gap (ms)"] = since(first_frame(gap, origin, closed), origin)
    return out


def click_metrics(scenarios):
    out = []
    for click in scenarios["clickNear"]:
        origin = click["marks"][0]["t"]
        offsets = [(frame["t"], frame["paneScrollTop"]) for frame in click["frames"]]
        start_offset, end_offset = offsets[0][1], offsets[-1][1]
        distance = end_offset - start_offset
        moving = [t for t, offset in offsets if abs(offset - start_offset) > 0.5]
        ended = next((t for t, offset in offsets if abs(offset - end_offset) < 0.6 and t >= (moving[0] if moving else 0)), None)
        mixes = [(frame["t"], highlight_mix(frame["flash"])) for frame in click["frames"]]
        lit = [t for t, mix in mixes if mix is not None and mix > HIGHLIGHT_REST + 1e-3]
        out.append(
            {
                "click": f"{click['from'] + 1} -> {click['to'] + 1}",
                "distance": round(abs(distance)),
                "duration (ms)": round(ended - moving[0], 1) if moving and ended else 0.0,
                "chromium sqrt(|d|)/60 (ms)": round(math.sqrt(abs(distance)) / 60 * 1000, 1),
                "highlight (ms)": round(lit[-1] - origin, 1) if lit else None,
                "card": next((frame["card"]["label"][:12] for frame in click["frames"] if frame["card"]), None),
            }
        )
    return out


def scrub_metrics(scenarios):
    scrub = scenarios["scrub"]
    currents, labels = [], []
    for frame in scrub["frames"]:
        if not currents or currents[-1] != frame["current"]:
            currents.append(frame["current"])
        label = frame["card"]["label"][:12] if frame["card"] else None
        if label and (not labels or labels[-1] != label):
            labels.append(label)
    release = mark(scrub, "release")
    return {
        "aria-current sequence": currents,
        "card sequence": labels,
        "close after release outside (ms)": since(
            first_frame(scrub, release, lambda frame: frame["card"] is None), release
        ),
    }


def card_metrics(scenarios):
    """Each marker's card height, from the sweep over every marker."""
    sweep = scenarios.get("cardSweep")
    if sweep is None:
        return None
    marks = sweep["marks"]
    heights = []
    for position, current in enumerate(marks):
        end = marks[position + 1]["t"] if position + 1 < len(marks) else math.inf
        frames = [f for f in sweep["frames"] if current["t"] <= f["t"] < end and f["card"]]
        heights.append(round(frames[-1]["card"]["rect"][3]) if frames else None)
    return heights


def keyboard_metrics(scenarios):
    """The prompt at the top after each Alt+arrow step, with its offset."""
    steps = scenarios.get("altArrows")
    if steps is None:
        return None
    out = []
    for step in steps:
        near = [
            (index + 1, round(top))
            for index, top in enumerate(step["tops"])
            if top is not None and -30 <= top <= 60
        ]
        out.append(f"{step['key']}: {near}")
    return out


def wheel_metrics(scenarios):
    wheel = scenarios["wheelOverRail"]
    return {"transcript scrolled": wheel["paneAfter"] != wheel["paneBefore"]}


def summarize(report):
    scenarios = report["scenarios"]
    return {
        "hover": hover_metrics(scenarios),
        "clicks": click_metrics(scenarios),
        "scrub": scrub_metrics(scenarios),
        "cards": card_metrics(scenarios),
        "keyboard": keyboard_metrics(scenarios),
        "wheel": wheel_metrics(scenarios),
    }


def compare(reference, native):
    failures = []

    def timing(section, key):
        want, got = reference[section][key], native[section][key]
        if want is None or got is None or abs(want - got) > TIMING_TOLERANCE_MS:
            failures.append(f"{section}: {key} reference {want} native {got}")

    for key, value in reference["hover"].items():
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            timing("hover", key)
        elif reference["hover"][key] != native["hover"].get(key):
            failures.append(f"hover: {key} reference {value} native {native['hover'].get(key)}")
    for want, got in zip(reference["clicks"], native["clicks"]):
        smooth_want, smooth_got = want["duration (ms)"] > 0, got["duration (ms)"] > 0
        if smooth_want != smooth_got:
            failures.append(f"click {want['click']}: smooth reference {smooth_want} native {smooth_got}")
        elif smooth_want:
            for build in (want, got):
                nominal = build["chromium sqrt(|d|)/60 (ms)"]
                if abs(build["duration (ms)"] - nominal) > nominal * SCROLL_DURATION_TOLERANCE + 40:
                    failures.append(f"click {build['click']}: {build['duration (ms)']} ms vs {nominal} ms")
        if want["highlight (ms)"] is None or got["highlight (ms)"] is None or abs(want["highlight (ms)"] - got["highlight (ms)"]) > 60:
            failures.append(f"click {want['click']}: highlight reference {want['highlight (ms)']} native {got['highlight (ms)']}")
    if reference["scrub"]["aria-current sequence"] != native["scrub"]["aria-current sequence"]:
        failures.append("scrub: aria-current sequence differs")
    timing("scrub", "close after release outside (ms)")
    if reference["cards"] is not None and reference["cards"] != native["cards"]:
        failures.append(f"cards: reference {reference['cards']} native {native['cards']}")
    if reference["keyboard"] is not None and reference["keyboard"] != native["keyboard"]:
        failures.append(f"keyboard: reference {reference['keyboard']} native {native['keyboard']}")
    if reference["wheel"] != native["wheel"]:
        failures.append(f"wheel: reference {reference['wheel']} native {native['wheel']}")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--reference", required=True)
    parser.add_argument("--gpui", required=True)
    parser.add_argument("--output")
    args = parser.parse_args()
    reference = summarize(json.loads(Path(args.reference).read_text()))
    native = summarize(json.loads(Path(args.gpui).read_text()))
    failures = compare(reference, native)

    width = max(len(key) for key in reference["hover"])
    print(f"{'hover'.ljust(width)}  reference  native")
    for key, value in reference["hover"].items():
        print(f"{key.ljust(width)}  {value!s:>9}  {native['hover'].get(key)!s}")
    print()
    for want, got in zip(reference["clicks"], native["clicks"]):
        print(f"click {want['click']}: reference {want['distance']} px in {want['duration (ms)']} ms, "
              f"highlight {want['highlight (ms)']} ms | native {got['distance']} px in {got['duration (ms)']} ms, "
              f"highlight {got['highlight (ms)']} ms")
    print()
    same = reference["scrub"]["aria-current sequence"] == native["scrub"]["aria-current sequence"]
    print(f"scrub aria-current sequence: {'identical' if same else 'DIFFERENT'} "
          f"({len(reference['scrub']['aria-current sequence'])} states)")
    print(f"scrub close after release: reference {reference['scrub']['close after release outside (ms)']} ms, "
          f"native {native['scrub']['close after release outside (ms)']} ms")
    if reference["cards"] is not None:
        same = reference["cards"] == native["cards"]
        print(f"card heights: {'identical' if same else 'DIFFERENT'} "
              f"(reference {reference['cards']}, native {native['cards']})")
    if reference["keyboard"] is not None:
        same = reference["keyboard"] == native["keyboard"]
        print(f"Alt+arrow steps: {'identical' if same else 'DIFFERENT'} ({'; '.join(native['keyboard'] or [])})")
    print(f"wheel over rail scrolls the transcript: reference {reference['wheel']['transcript scrolled']}, "
          f"native {native['wheel']['transcript scrolled']}")
    print()
    print("PASS" if not failures else "FAIL\n  " + "\n  ".join(failures))

    if args.output:
        Path(args.output).parent.mkdir(parents=True, exist_ok=True)
        Path(args.output).write_text(
            json.dumps({"reference": reference, "native": native, "failures": failures}, ensure_ascii=False, indent=1) + "\n"
        )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
