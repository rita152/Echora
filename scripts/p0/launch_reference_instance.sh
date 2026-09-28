#!/bin/sh
# Launch the dedicated ChatGPT reference instance with its app-server routed
# through the wire shim, so the JSON-RPC dialogue is logged as evidence.
#
# This is `scripts/launch_chatgpt_reference.sh` with the P0 defaults: its own
# port, launchd label and profile clone, plus `CHATGPT_REFERENCE_WIRE_LOG_DIR`.
# Everything that launcher does applies here too: `launchctl submit` instead of
# `open -n`, the clone's `Singleton*` entries removed so the instance cannot
# forward into the user's window, and the legacy sidebar pinned once the
# window is up (the instance is stopped when that fails).
#
# Usage:
#   scripts/p0/launch_reference_instance.sh [--stop]
#
# Environment:
#   P0_REFERENCE_PORT              remote debugging port (default 9333, must be free)
#   P0_REFERENCE_LABEL             launchd label (default chatgpt-reference-p0)
#   P0_REFERENCE_USER_DATA         profile clone directory
#   P0_REFERENCE_SOURCE_USER_DATA  profile to clone from
#   P0_WIRE_LOG_DIR                where the app-server JSONL logs are written;
#                                  keep it under $HOME, a launchd job cannot
#                                  write to this repository's external volume
# The underlying launcher's CHATGPT_REFERENCE_* variables, such as
# CHATGPT_REFERENCE_CODEX_HOME, apply as well.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
support="$HOME/Library/Application Support/gpui-chatgpt-reference"

CHATGPT_REFERENCE_PORT="${P0_REFERENCE_PORT:-9333}"
CHATGPT_REFERENCE_LABEL="${P0_REFERENCE_LABEL:-chatgpt-reference-p0}"
CHATGPT_REFERENCE_USER_DATA="${P0_REFERENCE_USER_DATA:-$support/p0-user-data}"
CHATGPT_REFERENCE_WIRE_LOG_DIR="${P0_WIRE_LOG_DIR:-$HOME/Library/Logs/gpui-capture/p0-wire/reference}"
export CHATGPT_REFERENCE_PORT CHATGPT_REFERENCE_LABEL CHATGPT_REFERENCE_USER_DATA CHATGPT_REFERENCE_WIRE_LOG_DIR
if [ -n "${P0_REFERENCE_SOURCE_USER_DATA:-}" ]; then
  CHATGPT_REFERENCE_SOURCE_DATA="$P0_REFERENCE_SOURCE_USER_DATA"
  export CHATGPT_REFERENCE_SOURCE_DATA
fi

exec "$here/../launch_chatgpt_reference.sh" "$@"
