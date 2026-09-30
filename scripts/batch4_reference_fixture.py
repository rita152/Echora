#!/usr/bin/env python3
"""Seed the batch-four reference clone with fixture projects and threads.

Run only while the dedicated reference instance is stopped. It creates two git
repositories under the reference support directory (the launchd-submitted app
cannot read this repository's external volume), one whose origin is
`openai/codex` and one whose origin is `rita152/Echora`, and starts one thread
per pull-request state on the matching head branch, so each thread's
`gitInfo` records the branch and origin the sidebar pull-request chip compares.

Threads are created by a `codex app-server` on PATH pointed at the clone's
CODEX_HOME, with a fake Responses provider on 127.0.0.1 supplied through `-c`
overrides: every turn is answered locally, no real model request is made, and
the clone's config.toml is left as it is. The projects are then registered in
the clone's `.codex-global-state.json` (`local-projects`, `project-order`,
`selected-project`), which the app imports at startup; `~/.codex` is never
touched. Attachments are added afterwards through the running reference's own
app-server connection (scripts/cdp_capture_batch4.mjs --seed).

Usage:
    python3 scripts/batch4_reference_fixture.py \
        --codex-home "$HOME/Library/Application Support/gpui-chatgpt-reference/batch4-codex-home" \
        --root "$HOME/Library/Application Support/gpui-chatgpt-reference/batch4-fixture" \
        --out artifacts/batch4-fixture-20260929.json
"""

from __future__ import annotations

import argparse
import json
import socketserver
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from batch1_app_server_probe import FakeResponses, Server, text_input  # noqa: E402

# (repository key, origin, branch, thread title, pull request url or None)
THREADS = [
    ("codex", "https://github.com/openai/codex.git", "dependabot/rust_toolchain/codex-rs/rust-toolchain-1.97.1",
     "Fixture PR failing", "https://github.com/openai/codex/pull/35882"),
    ("codex", "https://github.com/openai/codex.git", "starr/bazel-hot-devbox-cache",
     "Fixture PR successful", "https://github.com/openai/codex/pull/25114"),
    ("codex", "https://github.com/openai/codex.git", "codex/viyatb/remote-exec-network-guardian",
     "Fixture PR draft", "https://github.com/openai/codex/pull/31458"),
    ("codex", "https://github.com/openai/codex.git", "main", "Fixture background terminal", None),
    ("echora", "https://github.com/rita152/Echora.git", "feat/streaming-reveal",
     "Fixture PR merged", "https://github.com/rita152/Echora/pull/14"),
    ("echora", "https://github.com/rita152/Echora.git", "refactor/manager-one-shot-dispatch-20260911",
     "Fixture PR closed", "https://github.com/rita152/Echora/pull/3"),
]


def git(repo: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True).stdout


def prepare_repository(repo: Path, origin: str) -> None:
    if (repo / ".git").exists():
        return
    repo.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", "main", str(repo)], check=True)
    git(repo, "config", "user.email", "fixture@example.invalid")
    git(repo, "config", "user.name", "fixture")
    (repo / "README.md").write_text("Echora batch-four reference fixture.\n")
    git(repo, "add", ".")
    git(repo, "commit", "-q", "-m", "fixture")
    git(repo, "remote", "add", "origin", origin)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--codex-home", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    home = args.codex_home.resolve()
    if home == (Path.home() / ".codex").resolve():
        sys.exit("refusing to seed the real ~/.codex")
    state_path = home / ".codex-global-state.json"
    if not state_path.exists():
        sys.exit(f"{state_path} is missing; clone ~/.codex first")
    if subprocess.run(["pgrep", "-f", "ChatGPT --user-data-dir=.*batch4"], capture_output=True).returncode == 0:
        sys.exit("stop the batch4 reference instance first")

    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", 0), FakeResponses)
    httpd.daemon_threads = True
    port = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()

    repos = {"codex": args.root / "codex", "echora": args.root / "echora"}
    origins = {key: origin for key, origin, *_ in THREADS}
    for key, repo in repos.items():
        prepare_repository(repo, origins[key])

    overrides = [
        "-c", 'model_provider="batch4fake"',
        "-c", 'model="fake-model"',
        "-c", f'model_providers.batch4fake={{name="batch4fake",base_url="http://127.0.0.1:{port}/v1",wire_api="responses",request_max_retries=0,stream_max_retries=0}}',
    ]
    log: list[dict] = []
    created = []
    server = Server(home, args.root, log, tuple(overrides))
    try:
        server.request("initialize", {"clientInfo": {"name": "batch4_fixture", "version": "1"}, "capabilities": {"experimentalApi": True}})
        server.send({"method": "initialized"})
        for key, _origin, branch, title, url in THREADS:
            repo = repos[key]
            existing = git(repo, "branch", "--list", branch).strip()
            git(repo, "switch", "-q", *(() if existing else ("-c",)), branch)
            started = server.request(
                "thread/start", {"cwd": str(repo), "ephemeral": False, "historyMode": "paginated", "serviceName": "batch4-fixture"}
            )
            thread_id = started["result"]["thread"]["id"]
            turn = server.request("turn/start", {"threadId": thread_id, "input": text_input(title)})
            turn_id = turn["result"]["turn"]["id"]
            server.wait_for(
                lambda n: n.get("method") == "turn/completed" and n["params"]["turn"]["id"] == turn_id, timeout=60
            )
            server.request("thread/name/set", {"threadId": thread_id, "name": title})
            read = server.request("thread/read", {"threadId": thread_id, "includeTurns": False})
            created.append(
                {"threadId": thread_id, "title": title, "cwd": str(repo), "branch": branch, "url": url,
                 "gitInfo": read["result"]["thread"].get("gitInfo")}
            )
        for repo in repos.values():
            git(repo, "switch", "-q", "main")
    finally:
        server.close()
        httpd.shutdown()

    state = json.loads(state_path.read_text())
    projects = state.setdefault("local-projects", {})
    order = state.setdefault("project-order", [])
    now = int(time.time() * 1000)
    project_ids = {}
    for key, repo in repos.items():
        project_id = next((p["id"] for p in projects.values() if p.get("rootPaths") == [str(repo)]), None)
        if project_id is None:
            project_id = str(uuid.uuid4())
            projects[project_id] = {"id": project_id, "name": f"batch4-{key}", "rootPaths": [str(repo)], "createdAt": now, "updatedAt": now}
        if project_id not in order:
            order.insert(0, project_id)
        project_ids[key] = project_id
    state["selected-project"] = {"type": "local", "projectId": project_ids["codex"]}
    state_path.write_text(json.dumps(state, ensure_ascii=False))
    summary = {"fakeModelRequests": len(FakeResponses.requests), "projects": project_ids, "threads": created}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(summary, ensure_ascii=False, indent=1))
    print(json.dumps(summary, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
