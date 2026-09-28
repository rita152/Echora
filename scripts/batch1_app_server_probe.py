#!/usr/bin/env python3
"""Observe the baseline CLI's queue, goal and collaboration-mode behaviour.

Runs the `codex app-server` on PATH (the baseline in
docs/APP_SERVER_INTEGRATION.md) against an isolated CODEX_HOME whose only model
provider is a fake Responses endpoint served from this process on 127.0.0.1.
No real model request is made and nothing under ~/.codex is read or written.

Every JSON-RPC line is recorded with a timestamp, so the output doubles as the
wire evidence for the scenarios below and as fixture data for regression tests.

Usage:
    python3 scripts/batch1_app_server_probe.py --output artifacts/batch1-baseline-<date>
    python3 scripts/batch1_app_server_probe.py --output DIR --scenario queue
"""

from __future__ import annotations

import argparse
import http.server
import json
import os
import queue
import shutil
import socketserver
import subprocess
import tempfile
import threading
import time
from pathlib import Path


GUARDIAN_MARKER = '"outcome"'
DENY_VERDICT = {
    "risk_level": "high",
    "user_authorization": "low",
    "outcome": "deny",
    "rationale": "Probe: escalated command denied by the fake reviewer.",
}


class FakeResponses(http.server.BaseHTTPRequestHandler):
    """Minimal streaming Responses API.

    A request whose last user text contains `SLOW` waits `slow_seconds` before
    it completes; everything else answers immediately with `ok`. For the live
    guardian scenario, a user text containing `ESCALATE` makes the model call
    `exec_command` with `require_escalated`, and an automatic approval review
    request (recognised by its strict-JSON verdict prompt) answers with the
    next verdict from `guardian_verdicts` (allow once they run out).
    """

    slow_seconds = 4.0
    requests: list[dict] = []
    guardian_verdicts: list[dict] = []

    def log_message(self, *_args):
        return

    def do_GET(self):
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(length) or b"{}")
        FakeResponses.requests.append({"path": self.path, "body": body})
        text = json.dumps(body.get("input", []))[-4000:]
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        response_id = f"resp_{len(FakeResponses.requests)}"

        def event(kind: str, payload: dict):
            data = json.dumps({"type": kind, **payload})
            self.wfile.write(f"event: {kind}\ndata: {data}\n\n".encode())
            self.wfile.flush()

        event("response.created", {"response": {"id": response_id}})
        if "SLOW" in text:
            time.sleep(FakeResponses.slow_seconds)
        inputs = body.get("input", [])
        last = inputs[-1] if inputs else {}
        whole = json.dumps(body)
        if GUARDIAN_MARKER in whole and "ESCALATE" not in json.dumps(last):
            verdict = (
                FakeResponses.guardian_verdicts.pop(0)
                if FakeResponses.guardian_verdicts
                else {"outcome": "allow"}
            )
            reply = json.dumps(verdict)
        else:
            reply = "ok"
        if last.get("type") == "message" and "ESCALATE" in json.dumps(last) and GUARDIAN_MARKER not in whole:
            item = {
                "type": "function_call",
                "id": f"fc_{response_id}",
                "call_id": f"call_{response_id}",
                "name": "exec_command",
                "arguments": json.dumps(
                    {
                        "cmd": "echo probe-escalated",
                        "sandbox_permissions": "require_escalated",
                        "justification": "Probe: run a harmless echo outside the sandbox.",
                    }
                ),
            }
            event("response.output_item.added", {"item": item, "output_index": 0})
            event("response.output_item.done", {"item": item, "output_index": 0})
        else:
            message = {
                "type": "message",
                "role": "assistant",
                "id": f"msg_{response_id}",
                "content": [{"type": "output_text", "text": reply}],
            }
            event("response.output_item.added", {"item": {**message, "content": []}, "output_index": 0})
            event("response.output_item.done", {"item": message, "output_index": 0})
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


class Server:
    def __init__(self, home: Path, cwd: Path, log: list[dict]):
        self.log = log
        self.queue: queue.Queue = queue.Queue()
        self.notifications: list[dict] = []
        self.next_id = 0
        env = os.environ.copy()
        env["CODEX_HOME"] = str(home)
        self.stderr = (home / "app-server.stderr.log").open("w")
        self.process = subprocess.Popen(
            ["codex", "app-server", "--stdio"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.stderr,
            text=True,
            env=env,
            cwd=cwd,
        )
        threading.Thread(target=self._read, daemon=True).start()

    def _record(self, direction: str, message: dict):
        self.log.append({"at": round(time.time(), 3), "dir": direction, "message": message})

    def _read(self):
        for line in self.process.stdout:
            message = json.loads(line)
            self._record("s2c", message)
            if "method" in message and "id" not in message:
                self.notifications.append(message)
            self.queue.put(message)

    def send(self, message: dict):
        self._record("c2s", message)
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

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
                # Answer server requests so turns never hang on the probe.
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

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
        self.stderr.close()


def text_input(text: str) -> list[dict]:
    return [{"type": "text", "text": text, "text_elements": []}]


def start_thread(server: Server, project: Path) -> str:
    response = server.request(
        "thread/start",
        {"cwd": str(project), "ephemeral": False, "historyMode": "paginated", "serviceName": "batch1-probe"},
    )
    return response["result"]["thread"]["id"]


def turn_completed(thread_id: str):
    return lambda note: note.get("method") == "turn/completed" and note["params"]["threadId"] == thread_id


def scenario_collaboration(server: Server, _project: Path) -> dict:
    return {"collaborationMode/list": server.request("collaborationMode/list", {})}


def scenario_queue(server: Server, project: Path) -> dict:
    thread_id = start_thread(server, project)
    results: dict = {"threadId": thread_id}
    results["listNotLoadedOk"] = server.request("thread/queue/list", {"threadId": thread_id})
    server.request(
        "turn/start",
        {"threadId": thread_id, "input": text_input("SLOW first"), "clientUserMessageId": "probe-first"},
    )
    server.wait_for(lambda n: n.get("method") == "turn/started")
    added = []
    for name in ("A", "B", "C"):
        added.append(
            server.request(
                "thread/queue/add",
                {"threadId": thread_id, "input": text_input(f"queued {name}"), "clientUserMessageId": f"probe-{name}"},
            )
        )
    results["add"] = added
    ids = [entry["result"]["queuedSubmission"]["id"] for entry in added]
    results["update"] = server.request(
        "thread/queue/update",
        {"threadId": thread_id, "queuedSubmissionId": ids[1], "input": text_input("queued B edited")},
    )
    results["reorder"] = server.request(
        "thread/queue/reorder", {"threadId": thread_id, "queuedSubmissionIds": [ids[2], ids[0], ids[1]]}
    )
    results["reorderPartial"] = server.request(
        "thread/queue/reorder", {"threadId": thread_id, "queuedSubmissionIds": [ids[0]]}
    )
    results["listPage1"] = server.request("thread/queue/list", {"threadId": thread_id, "limit": 1})
    cursor = results["listPage1"].get("result", {}).get("nextCursor")
    if cursor:
        results["listPage2"] = server.request("thread/queue/list", {"threadId": thread_id, "limit": 1, "cursor": cursor})
    results["deleteC"] = server.request("thread/queue/delete", {"threadId": thread_id, "queuedSubmissionId": ids[2]})
    results["deleteCAgain"] = server.request("thread/queue/delete", {"threadId": thread_id, "queuedSubmissionId": ids[2]})
    # The first turn ends; observe whether the server starts the next queued
    # submission by itself.
    server.wait_for(turn_completed(thread_id), timeout=30)
    results["autoAdvanceStarted"] = server.wait_for(
        lambda n: n.get("method") == "turn/started" and n["params"]["threadId"] == thread_id, timeout=10
    )
    time.sleep(1.5)
    results["listAfterAdvance"] = server.request("thread/queue/list", {"threadId": thread_id})
    # Interrupt with a queued submission left, then check whether the queue
    # keeps advancing after the interrupt.
    server.request(
        "thread/queue/add",
        {"threadId": thread_id, "input": text_input("SLOW D"), "clientUserMessageId": "probe-D"},
    )
    server.request(
        "thread/queue/add",
        {"threadId": thread_id, "input": text_input("queued E"), "clientUserMessageId": "probe-E"},
    )
    started = server.wait_for(
        lambda n: n.get("method") == "item/started"
        and n["params"]["threadId"] == thread_id
        and n["params"]["item"].get("clientId") == "probe-D",
        timeout=30,
    )
    results["interrupt"] = server.request(
        "turn/interrupt", {"threadId": thread_id, "turnId": started["params"]["turnId"]}
    )
    time.sleep(3)
    results["listAfterInterrupt"] = server.request("thread/queue/list", {"threadId": thread_id})
    remaining = results["listAfterInterrupt"].get("result", {}).get("data", [])
    if remaining:
        results["startIdle"] = server.request(
            "thread/queue/start", {"threadId": thread_id, "queuedSubmissionId": remaining[0]["id"]}
        )
        server.wait_for(turn_completed(thread_id), timeout=30)
    results["startEmpty"] = server.request("thread/queue/start", {"threadId": thread_id})
    results["addIdle"] = server.request(
        "thread/queue/add",
        {"threadId": thread_id, "input": text_input("queued while idle"), "clientUserMessageId": "probe-idle"},
    )
    time.sleep(2)
    results["listAfterIdleAdd"] = server.request("thread/queue/list", {"threadId": thread_id})
    return results


def scenario_goal(server: Server, project: Path) -> dict:
    thread_id = start_thread(server, project)
    results: dict = {"threadId": thread_id}
    results["getEmpty"] = server.request("thread/goal/get", {"threadId": thread_id})
    results["setActive"] = server.request(
        "thread/goal/set", {"threadId": thread_id, "objective": "Say ok", "status": "active"}
    )
    # Observe whether an active goal on an idle thread starts a turn by itself.
    try:
        results["autoTurn"] = server.wait_for(
            lambda n: n.get("method") == "turn/started" and n["params"]["threadId"] == thread_id, timeout=8
        )
        server.wait_for(turn_completed(thread_id), timeout=30)
    except TimeoutError:
        results["autoTurn"] = None
    time.sleep(1)
    results["getAfter"] = server.request("thread/goal/get", {"threadId": thread_id})
    results["setPaused"] = server.request("thread/goal/set", {"threadId": thread_id, "status": "paused"})
    results["setObjectiveOnly"] = server.request(
        "thread/goal/set", {"threadId": thread_id, "objective": "Say ok twice"}
    )
    results["setBudget"] = server.request("thread/goal/set", {"threadId": thread_id, "tokenBudget": 1000})
    results["setComplete"] = server.request("thread/goal/set", {"threadId": thread_id, "status": "complete"})
    results["clear"] = server.request("thread/goal/clear", {"threadId": thread_id})
    results["clearAgain"] = server.request("thread/goal/clear", {"threadId": thread_id})
    results["getCleared"] = server.request("thread/goal/get", {"threadId": thread_id})
    results["setNoObjective"] = server.request("thread/goal/set", {"threadId": thread_id, "status": "active"})
    return results


def scenario_guardian(server: Server, project: Path) -> dict:
    thread_id = start_thread(server, project)
    return {
        "threadId": thread_id,
        "approveEmptyEvent": server.request(
            "thread/approveGuardianDeniedAction", {"threadId": thread_id, "event": {}}
        ),
    }


SNAKE = {"inProgress": "in_progress", "timedOut": "timed_out", "unifiedExec": "unified_exec"}


def denial_event(params: dict) -> dict:
    """The reference's GuardianAssessmentEvent for a denied review, for the
    command and execve actions a shell denial carries (same mapping as
    src/agent/codex/auto_approval.rs `denial_event`)."""
    review = params["review"]
    action = params["action"]
    event: dict = {}
    for key, field in (("id", "reviewId"), ("target_item_id", "targetItemId"), ("turn_id", "turnId")):
        if field in params:
            event[key] = params[field]
    event["status"] = SNAKE.get(review["status"], review["status"])
    for key, field in (("risk_level", "riskLevel"), ("user_authorization", "userAuthorization"), ("rationale", "rationale")):
        if field in review:
            event[key] = review[field]
    event["decision_source"] = params.get("decisionSource")
    kind = action["type"]
    if kind == "command":
        mapped = {"type": "command", "command": action["command"], "cwd": action["cwd"]}
        if "source" in action:
            mapped["source"] = SNAKE.get(action["source"], action["source"])
    elif kind == "execve":
        mapped = {"type": "execve", "program": action["program"], "argv": action["argv"], "cwd": action["cwd"]}
        if "source" in action:
            mapped["source"] = SNAKE.get(action["source"], action["source"])
    else:
        raise ValueError(f"probe maps only shell denials, got {kind}")
    event["action"] = mapped
    return event


def scenario_guardian_live(server: Server, project: Path) -> dict:
    """A real guardian denial against the baseline CLI, then its approval.

    The fake reviewer denies the first escalated `echo`; the scenario approves
    that denial with `thread/approveGuardianDeniedAction` and runs the same
    request again to observe what the approval changes. Only `echo` runs.
    """
    FakeResponses.guardian_verdicts = [DENY_VERDICT]
    thread_id = start_thread(server, project)
    results: dict = {"threadId": thread_id}
    before = len(FakeResponses.requests)
    server.request("turn/start", {"threadId": thread_id, "input": text_input("ESCALATE: run echo")})
    completed = server.wait_for(
        lambda n: n.get("method") == "item/autoApprovalReview/completed"
        and n["params"]["threadId"] == thread_id,
        timeout=30,
    )
    results["deniedReview"] = completed["params"]
    server.wait_for(turn_completed(thread_id), timeout=30)
    event = denial_event(completed["params"])
    results["event"] = event
    results["approve"] = server.request(
        "thread/approveGuardianDeniedAction", {"threadId": thread_id, "event": event}
    )
    results["approveAgain"] = server.request(
        "thread/approveGuardianDeniedAction", {"threadId": thread_id, "event": event}
    )
    mark = len(server.notifications)
    retry = server.request("turn/start", {"threadId": thread_id, "input": text_input("ESCALATE: run echo again")})
    retry_turn = retry["result"]["turn"]["id"]
    server.wait_for(
        lambda n: n.get("method") == "turn/completed" and n["params"]["turn"]["id"] == retry_turn,
        timeout=30,
    )
    results["retryNotifications"] = [
        {"method": n["method"], "params": n["params"]}
        for n in server.notifications[mark:]
        if n.get("method", "").startswith("item/autoApprovalReview")
        or (
            n.get("method") == "item/completed"
            and n["params"].get("item", {}).get("type") == "commandExecution"
        )
    ]
    guardian_inputs = [
        request for request in FakeResponses.requests[before:] if GUARDIAN_MARKER in json.dumps(request["body"])
    ]
    results["guardianRequests"] = len(guardian_inputs)
    # What the reviewer is told on the retry, beyond the first review: the
    # text parts that are new in the last review request.
    reviews = [
        request["body"].get("input", [])
        for request in FakeResponses.requests[before:]
        if GUARDIAN_MARKER in json.dumps(request["body"])
    ]

    def texts(items: list) -> list[str]:
        return [
            part.get("text", "")
            for item in items
            if isinstance(item, dict)
            for part in item.get("content", []) or []
            if isinstance(part, dict) and part.get("text")
        ]

    first = set(texts(reviews[0])) if reviews else set()
    results["retryReviewNewText"] = [text for text in texts(reviews[-1]) if text not in first] if reviews else []
    return results


SCENARIOS = {
    "collaboration": scenario_collaboration,
    "queue": scenario_queue,
    "goal": scenario_goal,
    "guardian": scenario_guardian,
    "guardian_live": scenario_guardian_live,
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scenario", choices=sorted(SCENARIOS), action="append")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scenarios = args.scenario or sorted(SCENARIOS)

    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", 0), FakeResponses)
    httpd.daemon_threads = True
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()

    version = subprocess.run(["codex", "--version"], capture_output=True, text=True).stdout.strip()
    summary = {"cli": version, "scenarios": {}}
    for name in scenarios:
        root = Path(tempfile.mkdtemp(prefix=f"batch1-{name}-"))
        home = root / "home"
        project = root / "project"
        home.mkdir()
        project.mkdir()
        subprocess.run(["git", "init", "-q", str(project)], check=True)
        live_guardian = name == "guardian_live"
        (home / "config.toml").write_text(
            "\n".join(
                [
                    'model = "fake-model"',
                    'model_provider = "fake"',
                    'approval_policy = "on-request"' if live_guardian else 'approval_policy = "never"',
                    'approvals_reviewer = "auto_review"' if live_guardian else "",
                    'sandbox_mode = "read-only"',
                    "[features]",
                    "goals = true",
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
            )
        )
        log: list[dict] = []
        server = Server(home, project, log)
        try:
            server.request(
                "initialize",
                {
                    "clientInfo": {"name": "batch1_probe", "version": "1"},
                    "capabilities": {"experimentalApi": True},
                },
            )
            server.send({"method": "initialized"})
            try:
                summary["scenarios"][name] = SCENARIOS[name](server, project)
            except Exception as error:  # noqa: BLE001 - recorded as evidence
                summary["scenarios"][name] = {"error": repr(error)}
        finally:
            server.close()
        # Temporary paths are the only machine-specific values in the logs.
        redact = lambda text: text.replace(tempfile.gettempdir(), "$TMPDIR")
        (output / f"{name}.wire.json").write_text(redact(json.dumps(log, ensure_ascii=False, indent=1)))
        (output / f"{name}.stderr.log").write_text(redact((home / "app-server.stderr.log").read_text()))
        shutil.rmtree(root, ignore_errors=True)
    httpd.shutdown()
    summary["fakeModelRequests"] = len(FakeResponses.requests)
    text = json.dumps(summary, ensure_ascii=False, indent=1).replace(tempfile.gettempdir(), "$TMPDIR")
    (output / "summary.json").write_text(text)
    print(text)


if __name__ == "__main__":
    main()
