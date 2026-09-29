#!/usr/bin/env python3
"""Observe the baseline CLI's batch-three methods.

Covers `review/start`, `thread/shellCommand`, `modelProvider/capabilities/read`,
`memory/status`, `thread/loaded/list` and `threadSection/update|delete` (with
the `threadSection/create|list` and `thread/section/move` calls around them).

Like scripts/batch2_app_server_probe.py, it reuses the app-server driver and
fake Responses endpoint of scripts/batch1_app_server_probe.py: the `codex
app-server` on PATH runs against an isolated CODEX_HOME whose only provider is
served from this process on 127.0.0.1, so no real model request is made and
nothing under ~/.codex is read or written. Every JSON-RPC line is recorded, so
the output is both the wire evidence and the fixture data of the regression
tests.

Usage:
    python3 scripts/batch3_app_server_probe.py --output artifacts/batch3-baseline-<date>
    python3 scripts/batch3_app_server_probe.py --output DIR --scenario review
"""

from __future__ import annotations

import argparse
import json
import shutil
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

UNKNOWN_THREAD = "00000000-0000-0000-0000-000000000000"


def _wait_for_after(self: Server, start: int, predicate, timeout: float = 30.0):
    """`Server.wait_for`, but only over notifications from index `start`."""
    deadline = time.monotonic() + timeout
    seen = start
    while time.monotonic() < deadline:
        while seen < len(self.notifications):
            note = self.notifications[seen]
            seen += 1
            if predicate(note):
                return note
        time.sleep(0.02)
    raise TimeoutError("notification not observed")


Server.wait_for_after = _wait_for_after

# What the review sub-agent must answer: the structured review output the CLI
# parses into `exitedReviewMode.review`.
REVIEW_OUTPUT = {
    "findings": [
        {
            "title": "[P2] Greeting drops the trailing newline",
            "body": "`hello.txt` now ends without a newline, which breaks `cat` concatenation.",
            "confidence_score": 0.6,
            "priority": 2,
            "code_location": {"absolute_file_path": "$PROJECT/hello.txt", "line_range": {"start": 1, "end": 1}},
        }
    ],
    "overall_correctness": "patch is incorrect",
    "overall_explanation": "One small formatting regression.",
    "overall_confidence_score": 0.7,
}


def reply_for(last_user: str) -> str:
    lowered = last_user.lower()
    if "review" in lowered or "diff" in lowered or "commit" in lowered or "changes" in lowered:
        return json.dumps(REVIEW_OUTPUT)
    return "ok"


def wait_turn(server: Server, thread_id: str, turn_id: str, timeout: float = 30) -> dict:
    return server.wait_for(
        lambda n: n.get("method") == "turn/completed"
        and n["params"]["threadId"] == thread_id
        and n["params"]["turn"]["id"] == turn_id,
        timeout=timeout,
    )


def notes_since(server: Server, start: int, thread_id: str | None = None) -> list[dict]:
    """Method, item type and ids of every notification after `start`, in order."""
    out = []
    for note in server.notifications[start:]:
        params = note.get("params") or {}
        if thread_id and params.get("threadId") not in (None, thread_id):
            continue
        item = params.get("item") or {}
        out.append(
            {
                "method": note["method"],
                "threadId": params.get("threadId"),
                "turnId": params.get("turnId") or (params.get("turn") or {}).get("id"),
                "itemType": item.get("type"),
                "itemId": item.get("id"),
                "status": item.get("status") or (params.get("turn") or {}).get("status"),
            }
        )
    return out


def git(project: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(project), *args], check=True, capture_output=True, text=True).stdout


def prepare_repository(project: Path) -> dict:
    """main with one commit, a feature branch with a second one, and an
    uncommitted edit on top."""
    git(project, "config", "user.email", "probe@example.invalid")
    git(project, "config", "user.name", "probe")
    (project / "hello.txt").write_text("hello\n")
    git(project, "add", ".")
    git(project, "commit", "-q", "-m", "initial")
    git(project, "branch", "-M", "main")
    git(project, "checkout", "-q", "-b", "feature")
    (project / "hello.txt").write_text("hello, world\n")
    git(project, "commit", "-q", "-am", "Greet the world")
    sha = git(project, "rev-parse", "HEAD").strip()
    (project / "hello.txt").write_text("hello, world")
    (project / "notes.md").write_text("untracked\n")
    return {"featureSha": sha}


def run_review(server: Server, thread_id: str, target: dict, delivery: str | None = "inline") -> dict:
    start = len(server.notifications)
    params = {"threadId": thread_id, "target": target}
    if delivery is not None:
        params["delivery"] = delivery
    response = server.request("review/start", params)
    result = {"response": response}
    if "result" in response:
        review_thread = response["result"]["reviewThreadId"]
        turn_id = response["result"]["turn"]["id"]
        result["completed"] = wait_turn(server, review_thread, turn_id, timeout=60)
        result["notifications"] = notes_since(server, start)
        items = server.request(
            "thread/turns/list",
            {"threadId": review_thread, "limit": 5, "sortDirection": "desc", "itemsView": "full"},
        )
        result["turnsAfter"] = items
    else:
        time.sleep(0.3)
        result["notifications"] = notes_since(server, start)
    return result


def scenario_review(server: Server, project: Path, _home: Path) -> dict:
    """Every target, both deliveries, the schema-recommended detached
    replacement (thread/start + inline) and the error branches."""
    results: dict = {"repository": prepare_repository(project)}
    thread_id = start_thread(server, project)
    results["threadId"] = thread_id
    results["uncommittedInline"] = run_review(server, thread_id, {"type": "uncommittedChanges"})
    results["baseBranchInline"] = run_review(server, thread_id, {"type": "baseBranch", "branch": "main"})
    results["commitInline"] = run_review(
        server, thread_id, {"type": "commit", "sha": results["repository"]["featureSha"], "title": "Greet the world"}
    )
    results["customInline"] = run_review(server, thread_id, {"type": "custom", "instructions": "Check the greeting."})
    results["defaultDelivery"] = run_review(server, thread_id, {"type": "uncommittedChanges"}, delivery=None)
    results["detached"] = run_review(server, thread_id, {"type": "uncommittedChanges"}, delivery="detached")
    fresh = start_thread(server, project)
    results["threadStartThenInline"] = {"threadId": fresh, **run_review(server, fresh, {"type": "baseBranch", "branch": "main"})}
    # Error branches.
    results["unknownThread"] = run_review(server, UNKNOWN_THREAD, {"type": "uncommittedChanges"})
    results["emptyBranch"] = run_review(server, thread_id, {"type": "baseBranch", "branch": ""})
    results["emptyInstructions"] = run_review(server, thread_id, {"type": "custom", "instructions": "  "})
    results["missingBranch"] = run_review(server, thread_id, {"type": "baseBranch", "branch": "no-such-branch"})
    results["unknownTarget"] = run_review(server, thread_id, {"type": "stagedChanges"})
    # While another turn runs on the same thread.
    mark = len(server.notifications)
    server.request("turn/start", {"threadId": thread_id, "input": text_input("SLOW busy")})
    busy = server.wait_for_after(
        mark, lambda n: n.get("method") == "turn/started" and n["params"]["threadId"] == thread_id
    )
    results["busyTurnId"] = busy["params"]["turn"]["id"]
    results["whileTurnRunning"] = run_review(server, thread_id, {"type": "uncommittedChanges"})
    results["threadRead"] = server.request("thread/read", {"threadId": thread_id, "includeTurns": False})
    return results


def run_shell(server: Server, thread_id: str, command: str, timeout_ms: int | None = None) -> dict:
    """Runs one command and waits for the turn it starts to complete."""
    start = len(server.notifications)
    params = {"threadId": thread_id, "command": command}
    if timeout_ms is not None:
        params["timeoutMs"] = timeout_ms
    response = server.request("thread/shellCommand", params)
    result = {"response": response}
    if "result" in response:
        item = server.wait_for_after(
            start,
            lambda n: n.get("method") == "item/completed"
            and (n["params"].get("item") or {}).get("type") == "commandExecution"
            and n["params"]["threadId"] == thread_id,
            timeout=20,
        )
        result["completed"] = item
        turn_id = item["params"]["turnId"]
        try:
            server.wait_for_after(
                start,
                lambda n: n.get("method") == "turn/completed" and n["params"]["turn"]["id"] == turn_id,
                timeout=10,
            )
        except TimeoutError:
            result["turnNeverCompleted"] = True
    time.sleep(0.3)
    result["notifications"] = notes_since(server, start)
    result["raw"] = [
        note
        for note in server.notifications[start:]
        if note["method"] not in ("thread/tokenUsage/updated", "account/rateLimits/updated")
    ]
    return result


def scenario_shell(server: Server, project: Path, _home: Path) -> dict:
    """Success, failure, output streaming, timeouts, stopping and errors."""
    results: dict = {}
    thread_id = start_thread(server, project)
    results["threadId"] = thread_id
    results["echo"] = run_shell(server, thread_id, "echo shell-probe && printf 'two\\nlines\\n'")
    results["failure"] = run_shell(server, thread_id, "echo to-stderr >&2; exit 3")
    results["pipe"] = run_shell(server, thread_id, "printf 'b\\na\\n' | sort")
    results["streaming"] = run_shell(server, thread_id, "for i in 1 2 3; do echo tick $i; sleep 0.3; done")
    results["timeout"] = run_shell(server, thread_id, "echo before; sleep 5; echo never", timeout_ms=300)
    results["zeroTimeout"] = run_shell(server, thread_id, "echo zero", timeout_ms=0)
    results["negativeTimeout"] = run_shell(server, thread_id, "echo negative", timeout_ms=-1)
    results["empty"] = run_shell(server, thread_id, "")
    results["whitespace"] = run_shell(server, thread_id, "   ")
    results["unknownThread"] = run_shell(server, UNKNOWN_THREAD, "echo x")
    # Stopping a long command: the turn it runs in, then the process.
    start = len(server.notifications)
    response = server.request("thread/shellCommand", {"threadId": thread_id, "command": "echo started; sleep 20"})
    started = server.wait_for_after(
        start,
        lambda n: n.get("method") == "item/started"
        and (n["params"].get("item") or {}).get("type") == "commandExecution"
        and n["params"]["threadId"] == thread_id,
        timeout=10,
    )
    turn_id = started["params"]["turnId"]
    stop: dict = {"response": response, "turnId": turn_id}
    time.sleep(0.8)
    stop["interrupt"] = server.request("turn/interrupt", {"threadId": thread_id, "turnId": turn_id})
    time.sleep(0.8)
    stop["terminals"] = server.request("thread/backgroundTerminals/list", {"threadId": thread_id})
    for terminal in stop["terminals"].get("result", {}).get("data", []):
        stop.setdefault("terminate", []).append(
            server.request(
                "thread/backgroundTerminals/terminate",
                {"threadId": thread_id, "processId": terminal["processId"]},
            )
        )
    try:
        server.wait_for_after(
            start,
            lambda n: n.get("method") == "turn/completed" and n["params"]["turn"]["id"] == turn_id,
            timeout=25,
        )
    except TimeoutError:
        stop["turnNeverCompleted"] = True
    time.sleep(0.5)
    stop["notifications"] = notes_since(server, start)
    stop["raw"] = server.notifications[start:]
    results["stop"] = stop
    # While a model turn runs.
    start = len(server.notifications)
    server.request("turn/start", {"threadId": thread_id, "input": text_input("SLOW busy")})
    busy = server.wait_for_after(
        start, lambda n: n.get("method") == "turn/started" and n["params"]["threadId"] == thread_id
    )
    results["busyTurnId"] = busy["params"]["turn"]["id"]
    results["whileTurnRunning"] = run_shell(server, thread_id, "echo during-turn")
    try:
        server.wait_for_after(
            start,
            lambda n: n.get("method") == "turn/completed" and n["params"]["turn"]["id"] == results["busyTurnId"],
            timeout=30,
        )
    except TimeoutError:
        results["busyNeverCompleted"] = True
    results["history"] = server.request(
        "thread/turns/list", {"threadId": thread_id, "limit": 30, "sortDirection": "asc", "itemsView": "full"}
    )
    return results


def scenario_capabilities(server: Server, project: Path, home: Path) -> dict:
    """The fake Responses provider, then the built-in OpenAI provider and a
    thread-level model change."""
    results: dict = {}
    results["fake"] = server.request("modelProvider/capabilities/read", {})
    results["nullParams"] = server.request("modelProvider/capabilities/read", None)
    thread_id = start_thread(server, project)
    results["afterThreadStart"] = server.request("modelProvider/capabilities/read", {})
    results["withUnknownField"] = server.request("modelProvider/capabilities/read", {"threadId": thread_id})
    # Switch the default provider in the user layer and read again.
    read = server.request("config/read", {"includeLayers": True, "cwd": str(project)})
    user = next(layer for layer in read["result"]["layers"] if layer["name"]["type"] == "user")
    results["switchToOpenai"] = server.request(
        "config/batchWrite",
        {
            "edits": [
                {"keyPath": "model_provider", "value": "openai", "mergeStrategy": "replace"},
                {"keyPath": "model", "value": "gpt-5.5", "mergeStrategy": "replace"},
            ],
            "filePath": user["name"]["file"],
            "expectedVersion": user["version"],
            "reloadUserConfig": True,
        },
    )
    results["openai"] = server.request("modelProvider/capabilities/read", {})
    # Built-in providers that are not OpenAI's own Responses service.
    for provider in ("amazon-bedrock", "ollama", "lmstudio"):
        results[f"switchTo_{provider}"] = server.request(
            "config/batchWrite",
            {
                "edits": [{"keyPath": "model_provider", "value": provider, "mergeStrategy": "replace"}],
                "filePath": None,
                "expectedVersion": None,
                "reloadUserConfig": True,
            },
        )
        results[provider] = server.request("modelProvider/capabilities/read", {})
    return results


def scenario_memory_status(server: Server, project: Path, _home: Path) -> dict:
    """Defaults, the documented range of `minConsolidatedThreads`, and the
    memories feature switched off."""
    results: dict = {}
    results["default"] = server.request("memory/status", {})
    results["noParams"] = server.request("memory/status", None)
    results["nullMin"] = server.request("memory/status", {"minConsolidatedThreads": None})
    results["min1"] = server.request("memory/status", {"minConsolidatedThreads": 1})
    results["min0"] = server.request("memory/status", {"minConsolidatedThreads": 0})
    results["min4096"] = server.request("memory/status", {"minConsolidatedThreads": 4096})
    results["min4097"] = server.request("memory/status", {"minConsolidatedThreads": 4097})
    results["negative"] = server.request("memory/status", {"minConsolidatedThreads": -1})
    start_thread(server, project)
    results["afterThread"] = server.request("memory/status", {})
    results["featureOff"] = server.request(
        "config/batchWrite",
        {
            "edits": [{"keyPath": "features.memories", "value": False, "mergeStrategy": "replace"}],
            "filePath": None,
            "expectedVersion": None,
            "reloadUserConfig": True,
        },
    )
    results["statusFeatureOff"] = server.request("memory/status", {})
    return results


def scenario_loaded(server: Server, project: Path, _home: Path) -> dict:
    """Which threads count as loaded across start, resume, unsubscribe and
    pagination."""
    results: dict = {}
    results["empty"] = server.request("thread/loaded/list", {})
    first = start_thread(server, project)
    second = start_thread(server, project)
    third = start_thread(server, project)
    for thread_id in (first, second):
        response = server.request("turn/start", {"threadId": thread_id, "input": text_input("hello")})
        wait_turn(server, thread_id, response["result"]["turn"]["id"])
    results["ids"] = [first, second, third]
    results["afterStarts"] = server.request("thread/loaded/list", {})
    pages = [server.request("thread/loaded/list", {"limit": 1})]
    seen = set()
    while (cursor := pages[-1].get("result", {}).get("nextCursor")) and cursor not in seen and len(pages) < 10:
        seen.add(cursor)
        pages.append(server.request("thread/loaded/list", {"limit": 1, "cursor": cursor}))
    results["pages"] = pages
    results["limit0"] = server.request("thread/loaded/list", {"limit": 0})
    results["badCursor"] = server.request("thread/loaded/list", {"cursor": "not-a-cursor"})
    results["unsubscribe"] = server.request("thread/unsubscribe", {"threadId": second})
    time.sleep(0.5)
    results["afterUnsubscribe"] = server.request("thread/loaded/list", {})
    results["closedNotes"] = [n for n in server.notifications if n["method"] == "thread/closed"]
    results["resume"] = server.request("thread/resume", {"threadId": second})
    results["afterResume"] = server.request("thread/loaded/list", {})
    results["readUnloaded"] = server.request("thread/read", {"threadId": second, "includeTurns": False})
    return results


def scenario_sections(server: Server, project: Path, _home: Path) -> dict:
    """Custom sections: rename (appearance omitted, null, replaced), delete,
    thread membership around them, and the error branches."""
    results: dict = {}
    thread_a = start_thread(server, project)
    thread_b = start_thread(server, project)
    for thread_id in (thread_a, thread_b):
        response = server.request("turn/start", {"threadId": thread_id, "input": text_input("hello")})
        wait_turn(server, thread_id, response["result"]["turn"]["id"])
    results["listEmpty"] = server.request("threadSection/list", {"limit": 100})
    pinned = server.request("threadSection/create", {"name": "Pinned"})
    results["createPinned"] = pinned
    work = server.request("threadSection/create", {"name": "Work", "appearance": {"icon": "folder", "color": "blue"}})
    results["createWork"] = work
    later = server.request("threadSection/create", {"name": "Later"})
    results["createLater"] = later
    results["createEmptyName"] = server.request("threadSection/create", {"name": ""})
    results["createDuplicateName"] = server.request("threadSection/create", {"name": "Work"})
    work_id = work["result"]["section"]["id"]
    later_id = later["result"]["section"]["id"]
    results["moveA"] = server.request("thread/section/move", {"threadId": thread_a, "sectionId": work_id})
    results["moveB"] = server.request(
        "thread/section/move", {"threadId": thread_b, "sectionId": work_id, "beforeThreadId": thread_a}
    )
    results["listWork"] = server.request("thread/list", {"sectionId": work_id, "limit": 20})
    results["listUnsectioned"] = server.request("thread/list", {"sectionId": None, "limit": 20})
    results["renameKeepAppearance"] = server.request("threadSection/update", {"sectionId": work_id, "name": "Work stuff"})
    results["renameReplaceAppearance"] = server.request(
        "threadSection/update", {"sectionId": work_id, "name": "Work stuff", "appearance": {"icon": "star", "color": None}}
    )
    results["renameClearAppearance"] = server.request(
        "threadSection/update", {"sectionId": work_id, "name": "Work", "appearance": None}
    )
    results["renameEmpty"] = server.request("threadSection/update", {"sectionId": work_id, "name": ""})
    results["renameWhitespace"] = server.request("threadSection/update", {"sectionId": work_id, "name": "   "})
    results["renameUnknown"] = server.request("threadSection/update", {"sectionId": UNKNOWN_THREAD, "name": "x"})
    results["renameMalformedId"] = server.request("threadSection/update", {"sectionId": "nope", "name": "x"})
    results["threadReadInSection"] = server.request("thread/read", {"threadId": thread_a, "includeTurns": False})
    results["listAfterRename"] = server.request("threadSection/list", {"limit": 100})
    results["listPaged"] = server.request("threadSection/list", {"limit": 1})
    results["deleteWork"] = server.request("threadSection/delete", {"sectionId": work_id})
    results["deleteAgain"] = server.request("threadSection/delete", {"sectionId": work_id})
    results["deleteUnknown"] = server.request("threadSection/delete", {"sectionId": UNKNOWN_THREAD})
    results["deleteMalformedId"] = server.request("threadSection/delete", {"sectionId": "nope"})
    results["threadReadAfterDelete"] = server.request("thread/read", {"threadId": thread_a, "includeTurns": False})
    results["listDeletedSection"] = server.request("thread/list", {"sectionId": work_id, "limit": 20})
    results["moveIntoDeleted"] = server.request("thread/section/move", {"threadId": thread_a, "sectionId": work_id})
    results["updateDeleted"] = server.request("threadSection/update", {"sectionId": work_id, "name": "back"})
    results["listAfterDelete"] = server.request("threadSection/list", {"limit": 100})
    results["deleteLater"] = server.request("threadSection/delete", {"sectionId": later_id})
    results["deletePinned"] = server.request("threadSection/delete", {"sectionId": pinned["result"]["section"]["id"]})
    results["listFinal"] = server.request("threadSection/list", {"limit": 100})
    results["sectionNotifications"] = [
        n for n in server.notifications if "ection" in n["method"] or n["method"].startswith("thread/list")
    ]
    return results


SCENARIOS = {
    "review": scenario_review,
    "shell": scenario_shell,
    "capabilities": scenario_capabilities,
    "memory_status": scenario_memory_status,
    "loaded": scenario_loaded,
    "sections": scenario_sections,
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--scenario", choices=sorted(SCENARIOS), action="append")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scenarios = args.scenario or sorted(SCENARIOS)

    FakeResponses.reply_for = staticmethod(reply_for)
    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", 0), FakeResponses)
    httpd.daemon_threads = True
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()

    version = subprocess.run(["codex", "--version"], capture_output=True, text=True).stdout.strip()
    summary = {"cli": version, "scenarios": {}}
    for name in scenarios:
        root = Path(tempfile.mkdtemp(prefix=f"batch3-{name}-")).resolve()
        home = root / "home"
        project = root / "project"
        home.mkdir()
        project.mkdir()
        subprocess.run(["git", "init", "-q", str(project)], check=True)
        lines = [
            'model = "fake-model"',
            'model_provider = "fake"',
            'approval_policy = "never"',
            'sandbox_mode = "read-only"',
            "[features]",
            "memories = true",
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
        server = Server(home, project, log)
        try:
            server.request(
                "initialize",
                {"clientInfo": {"name": "batch3_probe", "version": "1"}, "capabilities": {"experimentalApi": True}},
            )
            server.send({"method": "initialized"})
            try:
                summary["scenarios"][name] = SCENARIOS[name](server, project, home)
            except Exception as error:  # noqa: BLE001 - recorded as evidence
                summary["scenarios"][name] = {"error": repr(error)}
        finally:
            server.close()

        # Temporary paths are the only machine-specific values in the logs.
        def redact(text: str) -> str:
            for prefix in (str(root), str(root).replace("/private", "", 1)):
                text = text.replace(prefix, "$PROBE")
            return text.replace(tempfile.gettempdir(), "$TMPDIR")

        (output / f"{name}.wire.json").write_text(redact(json.dumps(log, ensure_ascii=False, indent=1)))
        (output / f"{name}.stderr.log").write_text(redact((home / "app-server.stderr.log").read_text()))
        summary["scenarios"][name] = json.loads(redact(json.dumps(summary["scenarios"][name], ensure_ascii=False)))
        shutil.rmtree(root, ignore_errors=True)
    httpd.shutdown()
    summary["fakeModelRequests"] = len(FakeResponses.requests)
    text = json.dumps(summary, ensure_ascii=False, indent=1)
    (output / "summary.json").write_text(text)
    print(text[:3000])


if __name__ == "__main__":
    main()
