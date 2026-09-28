#!/bin/sh
# PATH entry named "codex" for a dedicated P0 verification instance.
#
# The wrapper forwards to scripts/p0/app_server_wire_shim.py, which spawns the
# real CLI and tees both stdio directions into a timestamped JSONL log without
# changing, reordering, or delaying a single byte.
#
# P0_WIRE_LOG_DIR  directory for the per-process logs (required)
# P0_SHIM_HELPER   the Python tee (default: app_server_wire_shim.py beside
#                  this file)
# P0_REAL_CODEX    real CLI to forward to (default: the CLI bundled with the
#                  ChatGPT app when present, otherwise the first codex on PATH
#                  that is not this shim)
# P0_WIRE_ORIGIN   label recorded in every log line (default: shim)
set -eu

if [ -z "${P0_WIRE_LOG_DIR:-}" ]; then
  echo "codex wire shim: P0_WIRE_LOG_DIR is not set" >&2
  exit 2
fi

self="$0"
helper="${P0_SHIM_HELPER:-$(dirname "$self")/app_server_wire_shim.py}"
origin="${P0_WIRE_ORIGIN:-shim}"
real="${P0_REAL_CODEX:-}"
if [ -z "$real" ]; then
  for candidate in \
    /Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex \
    /Applications/ChatGPT.app/Contents/Resources/codex; do
    if [ -x "$candidate" ]; then
      real="$candidate"
      break
    fi
  done
fi
if [ -z "$real" ]; then
  # The launcher puts this shim first on PATH, so `command -v codex` would find
  # the shim itself and recurse forever; skip every entry that resolves to it.
  saved_ifs="$IFS"
  IFS=:
  for directory in $PATH; do
    candidate="$directory/codex"
    if [ -x "$candidate" ] && ! [ "$candidate" -ef "$self" ]; then
      real="$candidate"
      break
    fi
  done
  IFS="$saved_ifs"
fi
if [ -z "$real" ]; then
  echo "codex wire shim: no real codex found; set P0_REAL_CODEX" >&2
  exit 2
fi

mkdir -p "$P0_WIRE_LOG_DIR"
log="$P0_WIRE_LOG_DIR/$origin-$(date +%Y%m%d-%H%M%S)-$$.jsonl"
exec python3 "$helper" \
  --real "$real" \
  --log "$log" \
  --origin "$origin" \
  -- "$@"
