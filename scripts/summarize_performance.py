#!/usr/bin/env python3
"""Summarize ECHORA_PERF_LOG JSONL without reading conversation content.

Frame work and submit are CPU times. The intervals between CPU draws and
scroll delivery to draw completion are not physical display/presentation times.
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
    args = parser.parse_args()
    phases = defaultdict(list)
    events = Counter()
    frames = []
    latency = []
    for line in args.log.read_text().splitlines():
        row = json.loads(line)
        if not args.start_ms <= row["t_ms"] <= args.end_ms:
            continue
        phases[row["phase"]].append(row["duration_ms"])
        if row["phase"] == "frame":
            frames.append(row)
            events.update(row["events"])
            if row.get("input_to_draw_end_ms") is not None:
                latency.append(row["input_to_draw_end_ms"])
                phases["scroll_cpu_frame"].append(row["duration_ms"])
    phases["scroll_delivery_to_cpu_draw_end"] = latency
    phases["cpu_draw_interval"] = [b["t_ms"] - a["t_ms"] for a, b in zip(frames, frames[1:])]
    stats = {name: {"count": len(values), "p50_ms": percentile(values, .5),
                    "p95_ms": percentile(values, .95), "p99_ms": percentile(values, .99)}
             for name, values in phases.items()}
    work = phases["frame"]
    print(json.dumps({"phases": stats, "frames_over_16_7_ms": sum(x > 1000 / 60 for x in work),
                      "over_budget_fraction": sum(x > 1000 / 60 for x in work) / len(work) if work else None,
                      "viewport": {k: frames[-1][k] for k in ["width", "height", "dpr"]} if frames else None,
                      "events": dict(events.most_common())}, indent=2))


if __name__ == "__main__":
    main()
