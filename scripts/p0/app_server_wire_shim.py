#!/usr/bin/env python3
"""Transparent stdio tee for a Codex app-server process.

Verification runs point a dedicated instance at this script instead of the
real codex executable. The wrapper spawns the real CLI with the exact argument
list it received and copies bytes in both directions without modifying,
reordering, or delaying them. Every JSONL line is also appended to a
timestamped log so the wire dialogue can be quoted as evidence.

Usage:
    app_server_wire_shim.py --real /path/to/codex --log /path/to/wire.jsonl -- ARGS...

The log holds one JSON object per line:
{"at": ..., "dir": "c2s"|"s2c", "origin": ..., "line": "..."}.
stderr of the child is forwarded unchanged and never mixed into the log.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import signal
import subprocess
import sys
import threading


def stamp():
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="milliseconds")


class WireLog:
    def __init__(self, path, origin):
        self.path = path
        self.origin = origin
        self.lock = threading.Lock()
        directory = os.path.dirname(path)
        if directory:
            os.makedirs(directory, exist_ok=True)
        self.handle = open(path, "a", encoding="utf-8", buffering=1)
        self.debug = bool(os.environ.get("P0_WIRE_DEBUG"))

    def note(self, direction, detail):
        if not self.debug:
            return
        record = {"at": stamp(), "dir": direction, "origin": self.origin, "debug": detail}
        with self.lock:
            self.handle.write(json.dumps(record, ensure_ascii=False) + "\n")

    def write(self, direction, line):
        record = {
            "at": stamp(),
            "dir": direction,
            "origin": self.origin,
            "line": line,
        }
        with self.lock:
            self.handle.write(json.dumps(record, ensure_ascii=False) + "\n")

    def close(self):
        with self.lock:
            self.handle.close()


def write_all(fd, data):
    view = memoryview(data)
    while view:
        written = os.write(fd, view)
        view = view[written:]


def pump(source_fd, sink_fd, log, direction):
    """Copy one direction until EOF; JSONL text is also logged line by line.

    The raw file descriptors are used on purpose: a buffered reader would block
    until the requested byte count arrived, which would stall a live
    app-server dialogue that arrives one short line at a time.
    """
    buffer = b""
    try:
        while True:
            try:
                chunk = os.read(source_fd, 65536)
            except OSError:
                break
            if not chunk:
                log.note("eof", direction)
                break
            log.note("chunk", direction + " " + str(len(chunk)) + " " + repr(chunk[:120]))
            buffer += chunk
            while b"\n" in buffer:
                raw, buffer = buffer.split(b"\n", 1)
                for part in raw.split(b"\r"):
                    log.write(direction, part.decode("utf-8", "replace"))
                write_all(sink_fd, raw + b"\n")
        if buffer:
            log.write(direction, buffer.decode("utf-8", "replace"))
            write_all(sink_fd, buffer)
    except (BrokenPipeError, ValueError, OSError):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--real", required=True)
    parser.add_argument("--log", required=True)
    parser.add_argument("--origin", default="codex")
    parser.add_argument("args", nargs=argparse.REMAINDER)
    parsed = parser.parse_args()
    forwarded = [value for value in parsed.args if value != "--"]

    log = WireLog(parsed.log, parsed.origin)
    log.write(
        "meta",
        json.dumps({"argv": forwarded, "cwd": os.getcwd(), "pid": os.getpid()}),
    )

    child = subprocess.Popen(
        [parsed.real] + forwarded,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
        bufsize=0,
    )

    to_child = threading.Thread(
        target=pump,
        args=(sys.stdin.fileno(), child.stdin.fileno(), log, "c2s"),
        daemon=True,
    )
    from_child = threading.Thread(
        target=pump,
        args=(child.stdout.fileno(), sys.stdout.fileno(), log, "s2c"),
        daemon=True,
    )
    to_child.start()
    from_child.start()

    def forward(signum, _frame):
        try:
            child.send_signal(signum)
        except OSError:
            pass

    for name in ("SIGINT", "SIGTERM", "SIGHUP"):
        if hasattr(signal, name):
            signal.signal(getattr(signal, name), forward)

    status = child.wait()
    to_child.join(timeout=2)
    from_child.join(timeout=2)
    log.write("meta", json.dumps({"exit": status}))
    log.close()
    return status


if __name__ == "__main__":
    sys.exit(main())
