#!/usr/bin/env python3
"""Sample macOS process CPU (100% = one core), footprint and pageins.

Only process counters are read. No sampling profiler runs beside the workload.
"""
import argparse
import ctypes
from datetime import datetime, timezone
import json
from pathlib import Path
import signal
import sys
import time


class Usage(ctypes.Structure):
    # Darwin's rusage_info_v2; CPU fields use Mach absolute-time ticks.
    _fields_ = [("uuid", ctypes.c_ubyte * 16)] + [
        (name, ctypes.c_uint64) for name in (
            "user", "system", "pkg_wakes", "interrupt_wakes", "pageins", "wired",
            "resident", "footprint", "start", "exit", "child_user", "child_system",
            "child_pkg", "child_interrupt", "child_pageins", "child_elapsed",
            "disk_read", "disk_write",
        )
    ]


class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", required=True, type=int)
    parser.add_argument("--seconds", type=float, default=60)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if sys.platform != "darwin" or args.seconds <= 0:
        parser.error("requires macOS and a positive duration")
    lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    base = Timebase()
    ctypes.CDLL("/usr/lib/libSystem.B.dylib").mach_timebase_info(ctypes.byref(base))
    start = time.monotonic()
    wall_start = datetime.now(timezone.utc).isoformat()
    rows = []
    interrupted = False

    def stop(_signum, _frame):
        nonlocal interrupted
        interrupted = True

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    while not rows or (not interrupted and time.monotonic() - start < args.seconds):
        usage = Usage()
        if lib.proc_pid_rusage(args.pid, 2, ctypes.byref(usage)) != 0:
            raise OSError(ctypes.get_errno(), "proc_pid_rusage failed")
        now = time.monotonic() - start
        cpu_ns = (usage.user + usage.system) * base.numer / base.denom
        row = dict(t=now, cpu_ns=cpu_ns, footprint=usage.footprint,
                   rss=usage.resident, pageins=usage.pageins)
        if rows:
            row["cpu_pct"] = (cpu_ns - rows[-1]["cpu_ns"]) / 1e9 / (now - rows[-1]["t"]) * 100
        rows.append(row)
        time.sleep(min(.2, max(0, args.seconds - now)))
    summary = dict(
        mean_cpu_pct=(rows[-1]["cpu_ns"] - rows[0]["cpu_ns"]) / 1e9 / (rows[-1]["t"] - rows[0]["t"]) * 100 if len(rows) > 1 else None,
        peak_200ms_cpu_pct=max(row.get("cpu_pct", 0) for row in rows),
        footprint_mib=rows[-1]["footprint"] / 1048576,
        pageins_delta=rows[-1]["pageins"] - rows[0]["pageins"],
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(dict(pid=args.pid, start_utc=wall_start,
        seconds=rows[-1]["t"], requested_seconds=args.seconds, interrupted=interrupted,
        timebase=dict(numer=base.numer, denom=base.denom),
        summary=summary, samples=rows), indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
