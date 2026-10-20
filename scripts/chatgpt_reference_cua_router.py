#!/usr/bin/env python3
"""Route a dedicated reference's CUA runtime to its own native preview service.

The packaged app regenerates MCP configuration without private process env.
Only inject the isolated socket into those runtime configs; forward all actual
requests, responses and events unchanged. Never log protocol contents or auth.
"""
import json
import os
import signal
from pathlib import Path
import subprocess
import sys
import threading


def route(value, pipe):
    if isinstance(value, dict):
        if "/cua_node/" in str(value.get("command", "")):
            value.setdefault("env", {})["SKY_CUA_SERVICE_NATIVE_PIPE_PATH"] = pipe
        for child in list(value.values()):
            route(child, pipe)
    elif isinstance(value, list):
        for child in value:
            route(child, pipe)


def main():
    root = Path(os.environ["CODEX_HOME"]).resolve()
    if root == (Path.home() / ".codex").resolve():
        sys.exit("Reference CUA routing requires an isolated CODEX_HOME")
    real = os.environ.get(
        "CHATGPT_REFERENCE_REAL_CLI",
        "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
    )
    args = sys.argv[1:]
    if "app-server" not in args:
        # The trusted Node kernel invokes CODEX_CLI_PATH with a minimal env.
        # It needs the real CLI, not the app-server-only routing variables.
        os.execv(real, [real, *args])
    pipe = os.environ["SKY_CUA_SERVICE_NATIVE_PIPE_PATH"]
    process = subprocess.Popen([real, *args], stdin=subprocess.PIPE,
                               stdout=sys.stdout.buffer, stderr=sys.stderr.buffer)

    # The launcher stops its direct child; forward termination to the real CLI
    # so its MCP runtimes and native capture service cannot remain orphaned.
    def terminate(signum, _frame):
        if process.poll() is None:
            process.send_signal(signum)

    for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(signum, terminate)

    def forward():
        try:
            for line in sys.stdin.buffer:
                try:
                    message = json.loads(line)
                    route(message, pipe)
                    if message.get("method") in ("config/batchWrite", "thread/start", "thread/resume", "turn/start"):
                        cache = root / "plugins/cache/openai-bundled/unified-computer-use"
                        for path in cache.glob("*/.mcp.json"):
                            # The app has just regenerated this private clone's plugin.
                            value = json.loads(path.read_text())
                            route(value, pipe)
                            path.write_text(json.dumps(value))
                    line = json.dumps(message, separators=(",", ":")).encode() + b"\n"
                except (ValueError, OSError):
                    pass
                process.stdin.write(line)
                process.stdin.flush()
        except BrokenPipeError:
            pass
        finally:
            process.stdin.close()

    threading.Thread(target=forward, daemon=True).start()
    return process.wait()


if __name__ == "__main__":
    sys.exit(main())
