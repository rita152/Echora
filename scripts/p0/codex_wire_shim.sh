#!/bin/sh
# PATH entry named "codex" for a dedicated P0 verification instance.
#
# The wrapper forwards to scripts/p0/app_server_wire_shim.py, which spawns the
# real CLI and tees both stdio directions into a timestamped JSONL log without
# changing, reordering, or delaying a single byte.
#
# P0_WIRE_LOG_DIR  directory for the per-process logs (required)
# P0_REAL_CODEX    real CLI to forward to (default: the ChatGPT bundle resource
#                  when present, otherwise the codex on PATH)
# P0_WIRE_ORIGIN   label recorded in every log line (default: shim)
set -eu

if [ -z "${P0_WIRE_LOG_DIR:-}" ]; then
  echo "codex wire shim: P0_WIRE_LOG_DIR is not set" >&2
  exit 2
fi

origin="${P0_WIRE_ORIGIN:-shim}"
real="${P0_REAL_CODEX:-}"
if [ -z "$real" ]; then
  if [ -x "/Applications/ChatGPT.app/Contents/Resources/codex" ]; then
    real="/Applications/ChatGPT.app/Contents/Resources/codex"
  else
    real="$(command -v codex)"
  fi
fi

mkdir -p "$P0_WIRE_LOG_DIR"
log="$P0_WIRE_LOG_DIR/$origin-$(date +%Y%m%d-%H%M%S)-$$.jsonl"
exec python3 "$P0_SHIM_HELPER" \
  --real "$real" \
  --log "$log" \
  --origin "$origin" \
  -- "$@"

