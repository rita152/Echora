#!/usr/bin/env python3
"""Transparent stdio tee for a Codex app-server process.

Verification runs point a dedicated instance at this script instead of the
real codex executable. The wrapper spawns the real CLI with the exact argument
list it received and copies bytes in both directions without modifying,
reordering, or delaying them. Every JSONL line is also appended to a
timestamped log so the wire dialogue can be quoted as evidence.

Usage:
    app_server_wire_shim.py --real /path/to/codex --log /path/to/wire.jsonl -- ARGS...

With `P0_WIRE_FAULTS=<file>` the client-to-server direction consults that
JSON file before forwarding each request: `{"<method>": {"delayMs": N}}`
holds the request (and every request after it) back for N ms, `{"<method>": {"error": {"code": ..,
"message": ..}}}` answers it with that error instead of forwarding it. The
file is re-read on every request, so a capture can switch faults on and off
while the instance runs; without the variable nothing is changed.

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
import time

# Serialises writes into one descriptor: a faulted request is answered on the
# app's stdout from the client-side pump while the server pump writes there.
SINK_LOCKS: dict[int, threading.Lock] = {}


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


def load_faults():
    path = os.environ.get("P0_WIRE_FAULTS")
    if not path:
        return {}
    try:
        with open(path, encoding="utf-8") as handle:
            faults = json.load(handle)
    except (OSError, ValueError):
        return {}
    return faults if isinstance(faults, dict) else {}


def apply_fault(raw, log, reply_fd, reply_lock):
    """Returns False when the request was answered here instead of forwarded."""
    if not os.environ.get("P0_WIRE_FAULTS"):
        return True
    try:
        message = json.loads(raw)
    except ValueError:
        return True
    if not isinstance(message, dict) or "id" not in message or "method" not in message:
        return True
    fault = load_faults().get(message["method"])
    if not isinstance(fault, dict):
        return True
    delay = fault.get("delayMs")
    if isinstance(delay, (int, float)) and delay > 0:
        log.write("meta", json.dumps({"fault": "delay", "method": message["method"], "delayMs": delay}))
        time.sleep(delay / 1000)
    error = fault.get("error")
    if isinstance(error, dict):
        reply = json.dumps({"id": message["id"], "error": error}).encode()
        log.write("meta", json.dumps({"fault": "error", "method": message["method"]}))
        log.write("s2c", reply.decode())
        with reply_lock:
            write_all(reply_fd, reply + b"\n")
        return False
    return True


def pump(source_fd, sink_fd, log, direction, fault=None):
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
                if fault is not None and not fault(raw):
                    continue
                with SINK_LOCKS.setdefault(sink_fd, threading.Lock()):
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

    stdout_fd = sys.stdout.fileno()
    reply_lock = SINK_LOCKS.setdefault(stdout_fd, threading.Lock())
    to_child = threading.Thread(
        target=pump,
        args=(sys.stdin.fileno(), child.stdin.fileno(), log, "c2s"),
        kwargs={"fault": lambda raw: apply_fault(raw, log, stdout_fd, reply_lock)},
        daemon=True,
    )
    from_child = threading.Thread(
        target=pump,
        args=(child.stdout.fileno(), stdout_fd, log, "s2c"),
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
