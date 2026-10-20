#!/usr/bin/env python3
"""Summarize ECHORA_PERF_LOG JSONL without reading conversation content.

Frame work and submit are CPU times. Metal presentation rows use the drawable's
display timestamp; CPU draw intervals remain a separate metric. Display
intervals include intentional idle gaps and must not be interpreted as FPS.
"""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path


def percentile(values, quantile):
    values = sorted(values)
    return values[round((len(values) - 1) * quantile)] if values else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("--start-ms", type=float, default=0)
    parser.add_argument("--end-ms", type=float, default=float("inf"))
    parser.add_argument("--workload", type=Path, help="Native workload.jsonl with measured perfMs boundaries")
    args = parser.parse_args()
    if args.workload:
        boundaries = {row["event"]: row for row in map(json.loads, args.workload.read_text().splitlines())}
        args.start_ms = boundaries["start"]["perfMs"]
        args.end_ms = boundaries["end"]["perfMs"]
        if args.start_ms is None or args.end_ms is None:
            parser.error("workload needs --perf-log to record perfMs boundaries")
    phases = defaultdict(list)
    events = Counter()
    frames = []
    latency = []
    presented = 0
    unavailable = 0
    with args.log.open() as log:
        for line in log:
            row = json.loads(line)
            # perfMs marks the last row already written when input starts.
            # Exclude that row (notably the capture preceding an idle run).
            if not args.start_ms < row["t_ms"] <= args.end_ms:
                continue
            if "duration_ms" in row:
                phases[row["phase"]].append(row["duration_ms"])
            if row["phase"] == "presentation":
                presented += 1
                if row.get("interval_ms") is not None:
                    phases["display_interval"].append(row["interval_ms"])
                if row.get("input_delivery_to_present_ms") is not None:
                    phases["input_delivery_to_display"].append(row["input_delivery_to_present_ms"])
            elif row["phase"] == "presentation_unavailable":
                unavailable += 1
            if row["phase"] == "frame":
                frames.append(row)
                events.update(row["events"])
                if row.get("input_to_draw_end_ms") is not None:
                    latency.append(row["input_to_draw_end_ms"])
                    phases["input_cpu_frame"].append(row["duration_ms"])
    phases["input_delivery_to_cpu_draw_end"] = latency
    phases["cpu_draw_interval"] = [b["t_ms"] - a["t_ms"] for a, b in zip(frames, frames[1:])]
    stats = {name: {"count": len(values), "p50_ms": percentile(values, .5),
                    "p95_ms": percentile(values, .95), "p99_ms": percentile(values, .99),
                    "max_ms": max(values) if values else None}
             for name, values in phases.items()}
    work = phases["frame"]
    print(json.dumps({"phases": stats, "presented_drawables": presented,
                      "unavailable_drawables": unavailable,
                      "frames_over_16_7_ms": sum(x > 1000 / 60 for x in work),
                      "over_budget_fraction": sum(x > 1000 / 60 for x in work) / len(work) if work else None,
                      "viewport": {k: frames[-1][k] for k in ["width", "height", "dpr"]} if frames else None,
                      "events": dict(events.most_common())}, indent=2))


if __name__ == "__main__":
    main()
