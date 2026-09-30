#!/usr/bin/env python3
"""Observe the baseline CLI's batch-four methods.

Covers `thread/attachment/add|list|remove`, the `thread/attachment/updated`
notification, and `thread/backgroundTerminals/clean` (with
`thread/backgroundTerminals/list` only as a comparison).

Like scripts/batch3_app_server_probe.py, it reuses the app-server driver and
fake Responses endpoint of scripts/batch1_app_server_probe.py: the `codex
app-server` on PATH runs against an isolated CODEX_HOME whose only provider is
served from this process on 127.0.0.1, so no real model request is made and
nothing under ~/.codex is read or written. The notification fan-out scenario
runs one app-server on a loopback WebSocket so several connections share it.
Every JSON-RPC line is recorded, so the output is both the wire evidence and
the fixture data of the regression tests.

Usage:
    python3 scripts/batch4_app_server_probe.py --output artifacts/batch4-baseline-<date>
    python3 scripts/batch4_app_server_probe.py --output DIR --scenario attachments
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import shutil
import socket
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from batch1_app_server_probe import (  # noqa: E402 - shared probe driver
    FakeResponses,
    Server,
    start_thread,
    text_input,
)
from batch3_app_server_probe import notes_since, wait_turn  # noqa: E402

UNKNOWN_THREAD = "00000000-0000-0000-0000-000000000000"

# The reference's own `thread/attachment/add` for a pull request, as its wire
# log records it (artifacts/batch4-reference-<date>/wire). The probe replays the
# same bytes to confirm that two clients adding the same pull request converge
# on one attachment.
REFERENCE_PR_URL = "https://github.com/openai/codex/pull/1"
# identityKey is JSON.stringify([host, owner, repo, number]) with the host,
# owner and repository lowercased.
REFERENCE_PR_IDENTITY = '["github.com","openai","codex",1]'
REFERENCE_PR_PAYLOAD = {"url": REFERENCE_PR_URL, "root": None, "headBranch": None}

# A background terminal: the command keeps running after the tool call yields,
# so the turn ends while its item is still in progress.
BACKGROUND_COMMAND = "echo bg-start; sleep 30; echo bg-end"
# One that outlives a whole reference capture session.
LONG_BACKGROUND_COMMAND = "echo bg-start; sleep 1800; echo bg-end"
# One that ends by itself a little after its turn, printing on the way.
SHORT_BACKGROUND_COMMAND = "echo bg-start; sleep 2; echo bg-mid; sleep 1; echo bg-end"


class BackgroundResponses(FakeResponses):
    """FakeResponses, plus a model that starts one long `exec_command` when the
    user text contains `BGTERM` and answers `ok` once the tool output is back."""

    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(length) or b"{}")
        inputs = body.get("input", [])
        last = inputs[-1] if inputs else {}
        if last.get("type") != "message" or "BGTERM" not in json.dumps(last):
            # Delegate to the shared endpoint with the body already consumed.
            self.rfile = _Replay(json.dumps(body).encode())
            self.headers.replace_header("content-length", str(len(json.dumps(body).encode())))
            return FakeResponses.do_POST(self)
        FakeResponses.requests.append({"path": self.path, "body": body})
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        response_id = f"resp_{len(FakeResponses.requests)}"

        def event(kind: str, payload: dict):
            data = json.dumps({"type": kind, **payload})
            self.wfile.write(f"event: {kind}\ndata: {data}\n\n".encode())
            self.wfile.flush()

        event("response.created", {"response": {"id": response_id}})
        text = json.dumps(last)
        if "BGTERM-SHORT" in text:
            command = SHORT_BACKGROUND_COMMAND
        elif "BGTERM-LONG" in text:
            command = LONG_BACKGROUND_COMMAND
        else:
            command = BACKGROUND_COMMAND
        item = {
            "type": "function_call",
            "id": f"fc_{response_id}",
            "call_id": f"call_{response_id}",
            "name": "exec_command",
            "arguments": json.dumps({"cmd": command, "yield_time_ms": 500, "tty": True}),
        }
        event("response.output_item.added", {"item": item, "output_index": 0})
        event("response.output_item.done", {"item": item, "output_index": 0})
        event(
            "response.completed",
            {
                "response": {
                    "id": response_id,
                    "usage": {
                        "input_tokens": 10,
                        "input_tokens_details": {"cached_tokens": 0},
                        "output_tokens": 1,
                        "output_tokens_details": {"reasoning_tokens": 0},
                        "total_tokens": 11,
                    },
                }
            },
        )


class _Replay:
    def __init__(self, data: bytes):
        self.data = data

    def read(self, _length: int = -1) -> bytes:
        data, self.data = self.data, b""
        return data


def index_of(server: Server, predicate) -> int | None:
    for index, entry in enumerate(server.log):
        if predicate(entry["message"]):
            return index
    return None


def request_with_order(server: Server, method: str, params: dict, settle: float = 0.4) -> dict:
    """Sends one request and records the notifications it caused, each with
    whether it reached the wire before or after the response."""
    start_log = len(server.log)
    start_notes = len(server.notifications)
    response = server.request(method, params)
    time.sleep(settle)
    entries = server.log[start_log:]
    response_at = next(
        (i for i, entry in enumerate(entries) if entry["dir"] == "s2c" and entry["message"].get("id") == response.get("id")),
        None,
    )
    ordered = []
    for i, entry in enumerate(entries):
        message = entry["message"]
        if entry["dir"] != "s2c" or "id" in message:
            continue
        if not message.get("method", "").startswith("thread/attachment"):
            continue
        ordered.append({"notification": message, "beforeResponse": response_at is not None and i < response_at})
    return {
        "response": response,
        "attachmentNotifications": ordered,
        "otherNotifications": [
            n["method"] for n in server.notifications[start_notes:] if not n["method"].startswith("thread/attachment")
        ],
    }


def add(server: Server, thread_id: str, kind: str, key: str, payload) -> dict:
    return request_with_order(
        server,
        "thread/attachment/add",
        {"threadId": thread_id, "attachmentType": kind, "identityKey": key, "payload": payload},
    )


def remove(server: Server, thread_id: str, kind: str, key: str) -> dict:
    return request_with_order(
        server,
        "thread/attachment/remove",
        {"threadId": thread_id, "attachmentType": kind, "identityKey": key},
    )


def list_all(server: Server, thread_id: str, limit=None) -> dict:
    pages = []
    cursor = None
    seen = set()
    while len(pages) < 20:
        params = {"threadId": thread_id, "cursor": cursor}
        if limit != "omit":
            params["limit"] = limit
        page = server.request("thread/attachment/list", params)
        pages.append(page)
        cursor = page.get("result", {}).get("nextCursor")
        if not cursor or cursor in seen:
            break
        seen.add(cursor)
    return {"pages": pages}


def run_turn(server: Server, thread_id: str, text: str = "hello") -> dict:
    response = server.request("turn/start", {"threadId": thread_id, "input": text_input(text)})
    return wait_turn(server, thread_id, response["result"]["turn"]["id"])


def scenario_attachments(server: Server, project: Path, home: Path) -> dict:
    """Add, list and remove on one connection: idempotence, validation, payload
    shapes, ordering, paging, persistence and fork."""
    results: dict = {}
    thread_id = start_thread(server, project)
    results["threadId"] = thread_id
    results["freshThreadAdd"] = add(
        server, thread_id, "pull_request", "github.com/openai/codex#7", {"url": "https://github.com/openai/codex/pull/7"}
    )
    run_turn(server, thread_id)
    pr_payload = {"url": "https://github.com/openai/codex/pull/42", "root": str(project), "headBranch": "feature"}
    results["addCreated"] = add(server, thread_id, "pull_request", "github.com/openai/codex#42", pr_payload)
    time.sleep(1.1)
    changed = {"url": "https://github.com/openai/codex/pull/42", "root": None, "headBranch": "renamed"}
    results["addExistingChangedPayload"] = add(server, thread_id, "pull_request", "github.com/openai/codex#42", changed)
    results["addExistingSamePayload"] = add(server, thread_id, "pull_request", "github.com/openai/codex#42", pr_payload)
    results["addSameKeyOtherType"] = add(server, thread_id, "worktree", "github.com/openai/codex#42", {"root": "/x", "workspaceRoot": "/y"})
    results["addWorktree"] = add(
        server, thread_id, "worktree", str(project), {"root": str(project), "workspaceRoot": str(project)}
    )
    results["addReference"] = add(server, thread_id, "pull_request", REFERENCE_PR_IDENTITY, REFERENCE_PR_PAYLOAD)
    results["addReferenceAgain"] = add(server, thread_id, "pull_request", REFERENCE_PR_IDENTITY, REFERENCE_PR_PAYLOAD)
    # Payload shapes are opaque JSON.
    for name, payload in {
        "Null": None,
        "Array": [1, "two", {"three": [3]}],
        "Deep": {"a": {"b": {"c": {"d": [None, True, 1.5, "e"]}}}},
        "String": "plain",
        "Number": 12345678901234,
    }.items():
        results[f"payload{name}"] = add(server, thread_id, "custom_probe", f"payload-{name.lower()}", payload)
    # Validation.
    for name, (kind, key) in {
        "emptyType": ("", "k"),
        "blankType": ("   ", "k"),
        "emptyKey": ("pull_request", ""),
        "blankKey": ("pull_request", "  "),
        "paddedKey": ("pull_request", " padded "),
        "upperType": ("PULL_REQUEST", "k-upper"),
    }.items():
        results[name] = add(server, thread_id, kind, key, {})
    results["missingPayload"] = server.request(
        "thread/attachment/add", {"threadId": thread_id, "attachmentType": "pull_request", "identityKey": "no-payload"}
    )
    results["unknownThread"] = add(server, UNKNOWN_THREAD, "pull_request", "k", {})
    results["malformedThread"] = add(server, "not-a-thread", "pull_request", "k", {})
    # Listing.
    results["listDefault"] = list_all(server, thread_id, limit="omit")
    results["listNullLimit"] = list_all(server, thread_id, limit=None)
    results["listLimit0"] = server.request("thread/attachment/list", {"threadId": thread_id, "limit": 0})
    results["listLimit1"] = list_all(server, thread_id, limit=1)
    results["listLimit3"] = list_all(server, thread_id, limit=3)
    first_cursor = results["listLimit1"]["pages"][0].get("result", {}).get("nextCursor")
    results["listRepeatCursor"] = [
        server.request("thread/attachment/list", {"threadId": thread_id, "limit": 1, "cursor": first_cursor})
        for _ in range(2)
    ]
    results["listBadCursor"] = server.request("thread/attachment/list", {"threadId": thread_id, "cursor": "not-a-cursor"})
    results["listEmptyCursor"] = server.request("thread/attachment/list", {"threadId": thread_id, "cursor": ""})
    results["listUnknownThread"] = server.request("thread/attachment/list", {"threadId": UNKNOWN_THREAD})
    other = start_thread(server, project)
    run_turn(server, other)
    results["otherThreadId"] = other
    results["listCursorOfOtherThread"] = server.request(
        "thread/attachment/list", {"threadId": other, "limit": 1, "cursor": first_cursor}
    )
    # Removing.
    results["removeExisting"] = remove(server, thread_id, "pull_request", "github.com/openai/codex#42")
    results["removeAgain"] = remove(server, thread_id, "pull_request", "github.com/openai/codex#42")
    results["removeMissing"] = remove(server, thread_id, "pull_request", "never-added")
    results["removeWrongType"] = remove(server, thread_id, "worktree", REFERENCE_PR_IDENTITY)
    results["removeUnknownThread"] = remove(server, UNKNOWN_THREAD, "pull_request", "k")
    results["removeBlankKey"] = remove(server, thread_id, "pull_request", " ")
    results["cursorAfterRemoval"] = server.request(
        "thread/attachment/list", {"threadId": thread_id, "limit": 1, "cursor": first_cursor}
    )
    results["reAddRemoved"] = add(server, thread_id, "pull_request", "github.com/openai/codex#42", pr_payload)
    # Default and largest page sizes.
    bulk = start_thread(server, project)
    run_turn(server, bulk)
    for index in range(130):
        server.request(
            "thread/attachment/add",
            {"threadId": bulk, "attachmentType": "custom_probe", "identityKey": f"bulk-{index:03}", "payload": index},
        )
    page_sizes = {}
    for label, limit in (("omitted", "omit"), ("null", None), ("50", 50), ("99", 99), ("100", 100), ("101", 101), ("1000", 1000)):
        params = {"threadId": bulk}
        if limit != "omit":
            params["limit"] = limit
        page = server.request("thread/attachment/list", params).get("result", {})
        page_sizes[label] = {"count": len(page.get("data", [])), "nextCursor": page.get("nextCursor")}
    results["pageSizes"] = page_sizes
    # Thread lifecycle.
    results["unsubscribe"] = server.request("thread/unsubscribe", {"threadId": thread_id})
    time.sleep(0.3)
    results["listAfterUnsubscribe"] = list_all(server, thread_id, limit="omit")
    results["addAfterUnsubscribe"] = add(server, thread_id, "custom_probe", "after-unsubscribe", {})
    results["resume"] = server.request("thread/resume", {"threadId": thread_id})
    results["listAfterResume"] = list_all(server, thread_id, limit="omit")
    results["fork"] = server.request("thread/fork", {"threadId": thread_id})
    fork_id = results["fork"].get("result", {}).get("thread", {}).get("id")
    results["listForked"] = list_all(server, fork_id, limit="omit") if fork_id else None
    results["readThread"] = server.request("thread/read", {"threadId": thread_id, "includeTurns": False})
    archived = start_thread(server, project)
    run_turn(server, archived)
    add(server, archived, "pull_request", "before-archive", {})
    results["archive"] = server.request("thread/archive", {"threadId": archived})
    results["archivedAdd"] = add(server, archived, "pull_request", "after-archive", {})
    results["archivedList"] = list_all(server, archived, limit="omit")
    results["archivedRemove"] = remove(server, archived, "pull_request", "before-archive")
    ephemeral = server.request(
        "thread/start", {"cwd": str(project), "ephemeral": True, "historyMode": "paginated", "serviceName": "batch4-probe"}
    )["result"]["thread"]["id"]
    results["ephemeralThreadId"] = ephemeral
    results["ephemeralAdd"] = add(server, ephemeral, "pull_request", "ephemeral", {})
    results["ephemeralList"] = list_all(server, ephemeral, limit="omit")
    deleted = start_thread(server, project)
    run_turn(server, deleted)
    add(server, deleted, "pull_request", "before-delete", {})
    results["delete"] = request_with_order(server, "thread/delete", {"threadId": deleted})
    results["deletedList"] = list_all(server, deleted, limit="omit")
    return results


def scenario_restart(server: Server, project: Path, home: Path) -> dict:
    """Attachments written by one app-server process, read by the next one."""
    results: dict = {}
    thread_id = start_thread(server, project)
    run_turn(server, thread_id)
    results["add"] = add(server, thread_id, "pull_request", "github.com/openai/codex#9", {"url": "https://github.com/openai/codex/pull/9"})
    results["listBefore"] = list_all(server, thread_id, limit="omit")
    server.close()
    log2: list[dict] = []
    second = Server(home, project, log2)
    try:
        second.request(
            "initialize", {"clientInfo": {"name": "batch4_probe", "version": "1"}, "capabilities": {"experimentalApi": True}}
        )
        second.send({"method": "initialized"})
        results["listAfterRestartUnloaded"] = list_all(second, thread_id, limit="omit")
        results["addAfterRestartUnloaded"] = add(second, thread_id, "pull_request", "github.com/openai/codex#9", {"url": "x"})
        results["resume"] = second.request("thread/resume", {"threadId": thread_id})
        results["listAfterRestartResumed"] = list_all(second, thread_id, limit="omit")
        results["secondLog"] = log2
    finally:
        second.close()
    # The caller closes `server` again; make that a no-op.
    server.close = lambda: None
    return results


class WsClient:
    """One JSON-RPC connection to a WebSocket app-server."""

    def __init__(self, url: str, name: str, log: list[dict]):
        import websocket  # websocket-client

        self.name = name
        self.log = log
        self.ws = websocket.create_connection(url, timeout=30, suppress_origin=True)
        self.queue: queue.Queue = queue.Queue()
        self.notifications: list[dict] = []
        self.next_id = 0
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        while True:
            try:
                raw = self.ws.recv()
            except Exception:  # noqa: BLE001 - closed
                return
            message = json.loads(raw)
            self.log.append({"at": round(time.time(), 3), "conn": self.name, "dir": "s2c", "message": message})
            if "method" in message and "id" not in message:
                self.notifications.append(message)
            self.queue.put(message)

    def send(self, message: dict):
        self.log.append({"at": round(time.time(), 3), "conn": self.name, "dir": "c2s", "message": message})
        self.ws.send(json.dumps(message))

    def request(self, method: str, params: dict, timeout: float = 30.0) -> dict:
        self.next_id += 1
        request_id = self.next_id
        self.send({"id": request_id, "method": method, "params": params})
        deadline = time.monotonic() + timeout
        while True:
            message = self.queue.get(timeout=max(0.01, deadline - time.monotonic()))
            if message.get("id") == request_id and "method" not in message:
                return message
            if "method" in message and "id" in message:
                self.send({"id": message["id"], "error": {"code": -32601, "message": "probe"}})

    def wait_for(self, predicate, timeout: float = 30.0):
        deadline = time.monotonic() + timeout
        seen = 0
        while time.monotonic() < deadline:
            while seen < len(self.notifications):
                note = self.notifications[seen]
                seen += 1
                if predicate(note):
                    return note
            time.sleep(0.02)
        raise TimeoutError("notification not observed")

    def initialize(self):
        self.request("initialize", {"clientInfo": {"name": self.name, "version": "1"}, "capabilities": {"experimentalApi": True}})
        self.send({"method": "initialized"})

    def close(self):
        self.ws.close()


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def scenario_fanout(home: Path, project: Path, log: list[dict]) -> dict:
    """Which connections hear `thread/attachment/updated`: the writer, one
    subscribed to the same thread, one subscribed to another thread, and one
    never subscribed to anything."""
    port = free_port()
    env = os.environ.copy()
    env["CODEX_HOME"] = str(home)
    stderr = (home / "app-server.stderr.log").open("w")
    process = subprocess.Popen(
        ["codex", "app-server", "--listen", f"ws://127.0.0.1:{port}"], stderr=stderr, stdout=subprocess.DEVNULL, env=env, cwd=project
    )
    results: dict = {}
    clients: list[WsClient] = []
    try:
        deadline = time.monotonic() + 20
        while True:
            try:
                writer = WsClient(f"ws://127.0.0.1:{port}", "writer", log)
                break
            except Exception:  # noqa: BLE001 - not listening yet
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.2)
        same = WsClient(f"ws://127.0.0.1:{port}", "sameThread", log)
        other = WsClient(f"ws://127.0.0.1:{port}", "otherThread", log)
        idle = WsClient(f"ws://127.0.0.1:{port}", "unsubscribed", log)
        clients = [writer, same, other, idle]
        for client in clients:
            client.initialize()
        # Attachments need a thread with persisted history.
        thread_id = start_thread(writer, project)
        run_turn(writer, thread_id)
        other_thread = start_thread(other, project)
        run_turn(other, other_thread)
        results["threadId"] = thread_id
        results["otherThreadId"] = other_thread
        results["sameResume"] = same.request("thread/resume", {"threadId": thread_id})
        marks = {client.name: len(client.notifications) for client in clients}
        results["add"] = writer.request(
            "thread/attachment/add",
            {"threadId": thread_id, "attachmentType": "pull_request", "identityKey": "fanout#1", "payload": {"url": "u"}},
        )
        results["addExisting"] = writer.request(
            "thread/attachment/add",
            {"threadId": thread_id, "attachmentType": "pull_request", "identityKey": "fanout#1", "payload": {"url": "u"}},
        )
        results["remove"] = writer.request(
            "thread/attachment/remove", {"threadId": thread_id, "attachmentType": "pull_request", "identityKey": "fanout#1"}
        )
        results["removeMissing"] = writer.request(
            "thread/attachment/remove", {"threadId": thread_id, "attachmentType": "pull_request", "identityKey": "fanout#1"}
        )
        # Written by a connection that is not subscribed to the thread.
        results["addByUnsubscribed"] = idle.request(
            "thread/attachment/add",
            {"threadId": thread_id, "attachmentType": "worktree", "identityKey": "/w", "payload": {"root": "/w", "workspaceRoot": "/w"}},
        )
        time.sleep(0.8)
        results["received"] = {
            client.name: [n for n in client.notifications[marks[client.name]:] if n["method"].startswith("thread/attachment")]
            for client in clients
        }
    finally:
        for client in clients:
            client.close()
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
        stderr.close()
    return results


def command_items(server: Server, start: int, thread_id: str) -> list[dict]:
    out = []
    for note in server.notifications[start:]:
        item = (note.get("params") or {}).get("item") or {}
        if note["method"] in ("item/started", "item/completed") and item.get("type") == "commandExecution":
            if note["params"]["threadId"] == thread_id:
                out.append({"method": note["method"], "turnId": note["params"].get("turnId"), "item": item})
    return out


def start_background(server: Server, thread_id: str, text: str = "BGTERM start") -> dict:
    """A model turn whose exec_command outlives it."""
    start = len(server.notifications)
    response = server.request("turn/start", {"threadId": thread_id, "input": text_input(text)})
    turn_id = response["result"]["turn"]["id"]
    completed = wait_turn(server, thread_id, turn_id, timeout=40)
    time.sleep(0.5)
    return {
        "turnId": turn_id,
        "turnCompleted": completed,
        "commandItems": command_items(server, start, thread_id),
        "notifications": notes_since(server, start, thread_id),
    }


def scenario_background(server: Server, project: Path, _home: Path) -> dict:
    """A background terminal outliving its turn, `clean`, and the edge cases."""
    results: dict = {}
    thread_id = start_thread(server, project)
    results["threadId"] = thread_id
    results["background"] = start_background(server, thread_id)
    results["listWhileRunning"] = server.request("thread/backgroundTerminals/list", {"threadId": thread_id})
    results["readWhileRunning"] = server.request(
        "thread/turns/list", {"threadId": thread_id, "limit": 5, "sortDirection": "desc", "itemsView": "full"}
    )
    mark = len(server.notifications)
    log_mark = len(server.log)
    results["clean"] = server.request("thread/backgroundTerminals/clean", {"threadId": thread_id})
    time.sleep(2.0)
    results["afterClean"] = {
        "commandItems": command_items(server, mark, thread_id),
        "notifications": notes_since(server, mark, thread_id),
        "wire": [entry for entry in server.log[log_mark:] if entry["message"].get("method") not in ("thread/tokenUsage/updated",)],
    }
    results["listAfterClean"] = server.request("thread/backgroundTerminals/list", {"threadId": thread_id})
    results["historyAfterClean"] = server.request(
        "thread/turns/list", {"threadId": thread_id, "limit": 5, "sortDirection": "desc", "itemsView": "full"}
    )
    results["cleanAgain"] = server.request("thread/backgroundTerminals/clean", {"threadId": thread_id})
    idle = start_thread(server, project)
    results["cleanFreshThread"] = server.request("thread/backgroundTerminals/clean", {"threadId": idle})
    run_turn(server, idle)
    results["cleanIdleNoTerminals"] = server.request("thread/backgroundTerminals/clean", {"threadId": idle})
    results["cleanUnknownThread"] = server.request("thread/backgroundTerminals/clean", {"threadId": UNKNOWN_THREAD})
    results["cleanMalformedThread"] = server.request("thread/backgroundTerminals/clean", {"threadId": "nope"})
    # With a model turn in progress and a background terminal from before.
    results["secondBackground"] = start_background(server, thread_id)
    mark = len(server.notifications)
    response = server.request("turn/start", {"threadId": thread_id, "input": text_input("SLOW busy")})
    busy_turn = response["result"]["turn"]["id"]
    time.sleep(0.5)
    results["cleanDuringTurn"] = server.request("thread/backgroundTerminals/clean", {"threadId": thread_id})
    wait_turn(server, thread_id, busy_turn, timeout=40)
    time.sleep(1.0)
    results["duringTurn"] = {
        "busyTurnId": busy_turn,
        "commandItems": command_items(server, mark, thread_id),
        "notifications": notes_since(server, mark, thread_id),
    }
    # Interrupting a later turn: does the earlier background terminal survive?
    results["thirdBackground"] = start_background(server, thread_id)
    mark = len(server.notifications)
    response = server.request("turn/start", {"threadId": thread_id, "input": text_input("SLOW to interrupt")})
    stop_turn = response["result"]["turn"]["id"]
    time.sleep(0.8)
    results["interrupt"] = server.request("turn/interrupt", {"threadId": thread_id, "turnId": stop_turn})
    wait_turn(server, thread_id, stop_turn, timeout=40)
    time.sleep(1.0)
    results["listAfterInterrupt"] = server.request("thread/backgroundTerminals/list", {"threadId": thread_id})
    results["cleanAfterInterrupt"] = server.request("thread/backgroundTerminals/clean", {"threadId": thread_id})
    time.sleep(1.5)
    results["afterInterrupt"] = {
        "stopTurnId": stop_turn,
        "commandItems": command_items(server, mark, thread_id),
        "notifications": notes_since(server, mark, thread_id),
    }
    # Natural exit: a short background command that ends by itself, with
    # output arriving after its turn completed.
    natural = start_thread(server, project)
    results["natural"] = start_background(server, natural, "BGTERM-SHORT start")
    mark = len(server.notifications)
    time.sleep(5)
    results["naturalAfter"] = {
        "raw": [
            n
            for n in server.notifications[mark:]
            if n["method"] not in ("thread/tokenUsage/updated", "account/rateLimits/updated")
        ],
    }
    results["naturalList"] = server.request("thread/backgroundTerminals/list", {"threadId": natural})
    return results


SCENARIOS = {
    "attachments": scenario_attachments,
    "restart": scenario_restart,
    "background": scenario_background,
}
STANDALONE = {"fanout": scenario_fanout}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scenario", choices=sorted([*SCENARIOS, *STANDALONE]), action="append")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scenarios = args.scenario or sorted([*SCENARIOS, *STANDALONE])

    FakeResponses.reply_for = staticmethod(lambda _last: "ok")
    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", 0), BackgroundResponses)
    httpd.daemon_threads = True
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()

    version = subprocess.run(["codex", "--version"], capture_output=True, text=True).stdout.strip()
    summary = {"cli": version, "scenarios": {}}
    for name in scenarios:
        root = Path(tempfile.mkdtemp(prefix=f"batch4-{name}-")).resolve()
        home = root / "home"
        project = root / "project"
        home.mkdir()
        project.mkdir()
        subprocess.run(["git", "init", "-q", str(project)], check=True)
        lines = [
            'model = "fake-model"',
            'model_provider = "fake"',
            'approval_policy = "never"',
            'sandbox_mode = "danger-full-access"',
            "[model_providers.fake]",
            'name = "fake"',
            f'base_url = "http://127.0.0.1:{port}/v1"',
            'wire_api = "responses"',
            "request_max_retries = 0",
            "stream_max_retries = 0",
            f"[projects.{json.dumps(str(project))}]",
            'trust_level = "trusted"',
            "",
        ]
        (home / "config.toml").write_text("\n".join(lines))
        log: list[dict] = []
        before = len(FakeResponses.requests)
        if name in STANDALONE:
            try:
                summary["scenarios"][name] = STANDALONE[name](home, project, log)
            except Exception as error:  # noqa: BLE001 - recorded as evidence
                summary["scenarios"][name] = {"error": repr(error)}
        else:
            server = Server(home, project, log)
            try:
                server.request(
                    "initialize",
                    {"clientInfo": {"name": "batch4_probe", "version": "1"}, "capabilities": {"experimentalApi": True}},
                )
                server.send({"method": "initialized"})
                try:
                    summary["scenarios"][name] = SCENARIOS[name](server, project, home)
                except Exception as error:  # noqa: BLE001 - recorded as evidence
                    summary["scenarios"][name] = {"error": repr(error)}
            finally:
                server.close()
        summary["scenarios"][name]["fakeModelRequests"] = len(FakeResponses.requests) - before

        # Temporary paths are the only machine-specific values in the logs.
        def redact(text: str) -> str:
            for prefix in (str(root), str(root).replace("/private", "", 1)):
                text = text.replace(prefix, "$PROBE")
            return text.replace(tempfile.gettempdir(), "$TMPDIR")

        (output / f"{name}.wire.json").write_text(redact(json.dumps(log, ensure_ascii=False, indent=1)))
        stderr_log = home / "app-server.stderr.log"
        if stderr_log.exists():
            (output / f"{name}.stderr.log").write_text(redact(stderr_log.read_text()))
        summary["scenarios"][name] = json.loads(redact(json.dumps(summary["scenarios"][name], ensure_ascii=False)))
        shutil.rmtree(root, ignore_errors=True)
    httpd.shutdown()
    summary["fakeModelRequests"] = len(FakeResponses.requests)
    text = json.dumps(summary, ensure_ascii=False, indent=1)
    (output / "summary.json").write_text(text)
    print(text[:2000])


if __name__ == "__main__":
    main()
