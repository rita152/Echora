#!/usr/bin/env python3
"""Observe the baseline CLI's batch-two methods.

Covers `turn/settings/update`, `thread/searchOccurrences`, `hooks/list`,
`experimentalFeature/list`, `thread/memoryMode/set` and `memory/reset`, plus
the `config/batchWrite` edits the settings pages send with them.

Like scripts/batch1_app_server_probe.py (whose app-server driver and fake
Responses endpoint this reuses), it runs the `codex app-server` on PATH against
an isolated CODEX_HOME whose only provider is served from this process on
127.0.0.1. No real model request is made and nothing under ~/.codex is read or
written. Every JSON-RPC line is recorded, so the output is both the wire
evidence and the fixture data of the regression tests.

Usage:
    python3 scripts/batch2_app_server_probe.py --output artifacts/batch2-baseline-<date>
    python3 scripts/batch2_app_server_probe.py --output DIR --scenario search
"""

from __future__ import annotations

import argparse
import json
import os
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
    turn_completed,
)

# Replies with known content, keyed by a marker in the user's text, so search
# results cover the assistant's final answers too.
REPLIES = {
    "REPLY1": "HELLO from the assistant 🙂 hello",
    "REPLY2": "回答：你好 😀 hello",
    "REPLY3": "ok",
}


def reply_for(last_user: str) -> str:
    for marker, reply in REPLIES.items():
        if marker in last_user:
            return reply
    return "ok"


def run_turn(server: Server, thread_id: str, text: str) -> dict:
    response = server.request("turn/start", {"threadId": thread_id, "input": text_input(text)})
    turn_id = response["result"]["turn"]["id"]
    server.wait_for(
        lambda n: n.get("method") == "turn/completed" and n["params"]["turn"]["id"] == turn_id, timeout=30
    )
    return response


def scenario_turn_settings(server: Server, project: Path, _home: Path) -> dict:
    """`turn/settings/update` while a turn runs, then after it ended, in the
    order Echora sends it: the thread settings first, then the reviewer."""
    thread_id = start_thread(server, project)
    results: dict = {"threadId": thread_id}
    server.request("turn/start", {"threadId": thread_id, "input": text_input("SLOW first")})
    started = server.wait_for(
        lambda n: n.get("method") == "turn/started" and n["params"]["threadId"] == thread_id
    )
    turn_id = started["params"]["turn"]["id"]
    results["turnId"] = turn_id
    results["threadUpdate"] = server.request(
        "thread/settings/update",
        {
            "threadId": thread_id,
            "approvalPolicy": "on-request",
            "approvalsReviewer": "auto_review",
            "permissions": ":workspace",
        },
    )
    results["threadUpdated"] = server.wait_for(
        lambda n: n.get("method") == "thread/settings/updated" and n["params"]["threadId"] == thread_id
    )
    results["runningApplied"] = server.request(
        "turn/settings/update",
        {"threadId": thread_id, "turnId": turn_id, "approvalsReviewer": "auto_review"},
    )
    results["unknownTurn"] = server.request(
        "turn/settings/update",
        {"threadId": thread_id, "turnId": "no-such-turn", "approvalsReviewer": "user"},
    )
    server.wait_for(turn_completed(thread_id), timeout=30)
    results["afterCompletion"] = server.request(
        "turn/settings/update",
        {"threadId": thread_id, "turnId": turn_id, "approvalsReviewer": "user"},
    )
    results["unknownThread"] = server.request(
        "turn/settings/update",
        {"threadId": "00000000-0000-0000-0000-000000000000", "turnId": turn_id, "approvalsReviewer": "user"},
    )
    return results


def scenario_search(server: Server, project: Path, _home: Path) -> dict:
    """Pagination, cursors and UTF-16 ranges of `thread/searchOccurrences`."""
    thread_id = start_thread(server, project)
    results: dict = {"threadId": thread_id, "replies": REPLIES}
    for text in ("Hello world, hello again REPLY1", "你好🙂世界，你好 REPLY2", "nothing here REPLY3"):
        run_turn(server, thread_id, text)

    def search(term: str, **extra) -> dict:
        return server.request("thread/searchOccurrences", {"threadId": thread_id, "searchTerm": term, **extra})

    pages = [search("hello", limit=2)]
    seen = set()
    while (cursor := pages[-1].get("result", {}).get("nextCursor")) and cursor not in seen and len(pages) < 10:
        seen.add(cursor)
        pages.append(search("hello", limit=2, cursor=cursor))
    results["helloPages"] = pages
    results["helloAll"] = search("hello")
    results["mixedCase"] = search("HeLLo")
    results["chinese"] = search("你好")
    results["emoji"] = search("🙂")
    results["emojiThenText"] = search("😀 hello")
    results["noMatch"] = search("zzz")
    results["empty"] = search("")
    results["whitespace"] = search("   ")
    first_cursor = pages[0].get("result", {}).get("nextCursor")
    if first_cursor:
        results["cursorWithOtherTerm"] = search("你好", cursor=first_cursor)
    results["unknownThread"] = server.request(
        "thread/searchOccurrences", {"threadId": "00000000-0000-0000-0000-000000000000", "searchTerm": "x"}
    )
    # The turn cursor is inclusive: the page it starts must hold that turn.
    occurrences = results["helloAll"].get("result", {}).get("data", [])
    results["turnCursorPages"] = []
    for occurrence in occurrences[-1:] + occurrences[:1]:
        results["turnCursorPages"].append(
            {
                "occurrence": occurrence,
                "page": server.request(
                    "thread/turns/list",
                    {
                        "threadId": thread_id,
                        "cursor": occurrence["turnCursor"],
                        "limit": 1,
                        "sortDirection": "asc",
                        "itemsView": "full",
                    },
                ),
            }
        )
    return results


def hooks_config(project: Path) -> str:
    return "\n".join(
        [
            "[[hooks.PreToolUse]]",
            'matcher = "Bash"',
            "[[hooks.PreToolUse.hooks]]",
            'type = "command"',
            'command = "echo trusted-pre-tool"',
            "timeout = 30",
            'statusMessage = "Checking the command"',
            "[[hooks.PostToolUse]]",
            "[[hooks.PostToolUse.hooks]]",
            'type = "mcp_tool"',
            'server = "audit"',
            'tool = "record"',
            "input = {}",
            "[[hooks.Stop]]",
            "[[hooks.Stop.hooks]]",
            'type = "command"',
            'command = "echo modified-stop"',
            "async = true",
            # Skipped with a load warning: the matcher is not a valid regex.
            "[[hooks.PreToolUse]]",
            'matcher = "(unclosed"',
            "[[hooks.PreToolUse.hooks]]",
            'type = "command"',
            'command = "echo bad-matcher"',
            f"[projects.{json.dumps(str(project))}]",
            'trust_level = "trusted"',
            "",
        ]
    )


def scenario_hooks(server: Server, project: Path, home: Path) -> dict:
    """User and project layers; trusted, untrusted, modified and managed
    hooks; a load warning; both write styles and a version conflict."""
    results: dict = {}
    listed = server.request("hooks/list", {"cwds": [str(project)]})
    results["initial"] = listed
    hooks = {hook["command" if hook["handlerType"] == "command" else "handlerType"]: hook
             for hook in listed["result"]["data"][0]["hooks"]}
    read = server.request("config/read", {"includeLayers": True, "cwd": str(project)})
    user = next(layer for layer in read["result"]["layers"] if layer["name"]["type"] == "user")
    trusted = hooks["echo trusted-pre-tool"]
    modified = hooks["echo modified-stop"]
    key = lambda hook: "hooks.state." + json.dumps(hook["key"])
    # Echora's discipline: the user layer's own file and version, replace.
    results["trustEchoraStyle"] = server.request(
        "config/batchWrite",
        {
            "edits": [
                {"keyPath": key(trusted) + ".trusted_hash", "value": trusted["currentHash"], "mergeStrategy": "replace"},
                {"keyPath": key(modified) + ".trusted_hash", "value": modified["currentHash"], "mergeStrategy": "replace"},
            ],
            "filePath": user["name"]["file"],
            "expectedVersion": user["version"],
            "reloadUserConfig": True,
        },
    )
    # The reference's form: upsert into the default user file, no version.
    results["disableReferenceStyle"] = server.request(
        "config/batchWrite",
        {
            "edits": [{"keyPath": key(modified) + ".enabled", "value": False, "mergeStrategy": "upsert"}],
            "filePath": None,
            "expectedVersion": None,
        },
    )
    results["staleVersion"] = server.request(
        "config/batchWrite",
        {
            "edits": [{"keyPath": key(trusted) + ".enabled", "value": False, "mergeStrategy": "replace"}],
            "filePath": user["name"]["file"],
            "expectedVersion": user["version"],
            "reloadUserConfig": True,
        },
    )
    # Change the modified hook's command so its stored hash goes stale.
    config = (home / "config.toml").read_text().replace("echo modified-stop", "echo modified-stop --changed")
    (home / "config.toml").write_text(config)
    results["afterWrites"] = server.request("hooks/list", {"cwds": [str(project)]})
    results["defaultCwds"] = server.request("hooks/list", {"cwds": []})
    results["readback"] = server.request("config/read", {"includeLayers": True, "cwd": str(project)})
    results["userConfigFile"] = (home / "config.toml").read_text()
    return results


def scenario_features(server: Server, project: Path, _home: Path) -> dict:
    """`experimentalFeature/list` pagination and a `features.<name>` write."""
    results: dict = {}
    pages = [server.request("experimentalFeature/list", {"cursor": None, "limit": 40})]
    seen = set()
    while (cursor := pages[-1].get("result", {}).get("nextCursor")) and cursor not in seen and len(pages) < 20:
        seen.add(cursor)
        pages.append(server.request("experimentalFeature/list", {"cursor": cursor, "limit": 40}))
    results["pages"] = pages
    results["defaultPage"] = server.request("experimentalFeature/list", {})
    results["reusedCursor"] = server.request("experimentalFeature/list", {"cursor": "not-a-cursor", "limit": 40})
    features = [feature for page in pages for feature in page.get("result", {}).get("data", [])]
    beta = [feature for feature in features if feature["stage"] == "beta"]
    results["betaNames"] = [feature["name"] for feature in beta]
    thread_id = start_thread(server, project)
    results["withThread"] = server.request(
        "experimentalFeature/list", {"cursor": None, "limit": 5, "threadId": thread_id}
    )
    if beta:
        name = beta[0]["name"]
        results["write"] = server.request(
            "config/batchWrite",
            {
                "edits": [{"keyPath": f"features.{name}", "value": not beta[0]["enabled"], "mergeStrategy": "replace"}],
                "filePath": None,
                "expectedVersion": None,
                "reloadUserConfig": True,
            },
        )
        after = [server.request("experimentalFeature/list", {"cursor": None, "limit": 200})]
        results["afterWrite"] = next(
            (feature for feature in after[0]["result"]["data"] if feature["name"] == name), None
        )
    return results


def scenario_memory(server: Server, project: Path, _home: Path) -> dict:
    """Per-thread memory mode, the new-chat `config` overrides and reset."""
    results: dict = {}
    started = server.request(
        "thread/start",
        {
            "cwd": str(project),
            "ephemeral": False,
            "historyMode": "paginated",
            "serviceName": "batch2-probe",
            "config": {"memories.generate_memories": False, "memories.use_memories": False},
        },
    )
    results["startWithMemoryConfig"] = started
    thread_id = started["result"]["thread"]["id"]
    results["disable"] = server.request("thread/memoryMode/set", {"threadId": thread_id, "mode": "disabled"})
    results["enable"] = server.request("thread/memoryMode/set", {"threadId": thread_id, "mode": "enabled"})
    results["unknownMode"] = server.request("thread/memoryMode/set", {"threadId": thread_id, "mode": "on"})
    results["unknownThread"] = server.request(
        "thread/memoryMode/set", {"threadId": "00000000-0000-0000-0000-000000000000", "mode": "enabled"}
    )
    results["featureEntry"] = next(
        (
            feature
            for feature in server.request("experimentalFeature/list", {"limit": 200})["result"]["data"]
            if feature["name"] == "memories"
        ),
        None,
    )
    results["settingsWrite"] = server.request(
        "config/batchWrite",
        {
            "edits": [
                {"keyPath": "features.memories", "value": True, "mergeStrategy": "replace"},
                {"keyPath": "memories.generate_memories", "value": True, "mergeStrategy": "replace"},
                {"keyPath": "memories.use_memories", "value": True, "mergeStrategy": "replace"},
            ],
            "filePath": None,
            "expectedVersion": None,
            "reloadUserConfig": True,
        },
    )
    results["toolAssistedWrite"] = server.request(
        "config/batchWrite",
        {
            "edits": [
                {"keyPath": "memories.disable_on_external_context", "value": True, "mergeStrategy": "replace"},
                {"keyPath": "memories.no_memories_if_mcp_or_web_search", "value": None, "mergeStrategy": "replace"},
            ],
            "filePath": None,
            "expectedVersion": None,
            "reloadUserConfig": True,
        },
    )
    results["readback"] = server.request("config/read", {"includeLayers": False, "cwd": str(project)})
    server.send({"id": 9001, "method": "memory/reset"})
    deadline = time.monotonic() + 10
    reset = None
    while time.monotonic() < deadline and reset is None:
        reset = next((entry["message"] for entry in server.log if entry["dir"] == "s2c" and entry["message"].get("id") == 9001), None)
        time.sleep(0.02)
    results["resetWithoutParams"] = reset
    results["resetWithEmptyParams"] = server.request("memory/reset", {})
    return results


SCENARIOS = {
    "turn_settings": scenario_turn_settings,
    "search": scenario_search,
    "hooks": scenario_hooks,
    "features": scenario_features,
    "memory": scenario_memory,
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
        root = Path(tempfile.mkdtemp(prefix=f"batch2-{name}-")).resolve()
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
        ]
        if name == "hooks":
            lines.append(hooks_config(project))
            (project / ".codex").mkdir()
            (project / ".codex" / "config.toml").write_text(
                "\n".join(
                    [
                        "[[hooks.SessionStart]]",
                        "[[hooks.SessionStart.hooks]]",
                        'type = "command"',
                        'command = "echo project-session-start"',
                        "",
                    ]
                )
            )
            # A hooks file that fails to parse: another load warning. Managed
            # hooks come only from /etc/codex/requirements.toml, which the
            # probe must not write, so `managed` stays fixture-only.
            (home / "hooks.json").write_text('{"hooks": {"Stop": [{"hooks": [{"type": "command"}]}]}}')
        else:
            lines += [f"[projects.{json.dumps(str(project))}]", 'trust_level = "trusted"', ""]
        (home / "config.toml").write_text("\n".join(lines))
        log: list[dict] = []
        server = Server(home, project, log)
        try:
            server.request(
                "initialize",
                {"clientInfo": {"name": "batch2_probe", "version": "1"}, "capabilities": {"experimentalApi": True}},
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
    print(text[:4000])


if __name__ == "__main__":
    main()
