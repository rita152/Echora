#!/bin/sh
# Launch the verification instance that Computer Use can drive, with no macOS
# permission prompt and the same data the ChatGPT app uses.
#
# Three deliberate choices, each measured on 2026-09-22 (tccd log):
#
#   * Not `open -n`. LaunchServices makes the app its own TCC "responsible
#     process", and because this repository lives on an external volume macOS
#     then asks "…would like to access files on a removable volume" before the
#     window can read its own bundle. `launchctl submit` produced no such
#     request at all, and a direct child of the session is covered by the Codex
#     app's own grant.
#   * A copy under `~/Applications`, so the GUI process never opens a file on
#     the external volume and that policy cannot apply to it in the first
#     place. The packaged bundle carries its own assets, so the copy is
#     self-contained.
#   * The session's `PATH` and the real `HOME`, because the app spawns
#     `codex app-server --stdio` from `PATH`; without it the sidebar reports
#     "Load projects failed" and shows no projects or chats. With it, the
#     instance reads the same `~/.codex` projects, threads and auth as the
#     ChatGPT app.
#
# Usage:
#   scripts/launch_verify_instance.sh [extra app arguments…]
#   scripts/launch_verify_instance.sh --stop
#
# Environment:
#   GPUI_CAPTURE_SKIP_BUILD=1   use the existing package instead of rebuilding
#   GPUI_VERIFY_BUNDLE          bundle to copy (default: this worktree's package)
#   GPUI_VERIFY_LABEL           launchd label (default gpui-verify)
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

label="${GPUI_VERIFY_LABEL:-gpui-verify}"

if [ "${1:-}" = "--stop" ]; then
  launchctl remove "$label" 2>/dev/null || true
  pkill -f "gpui-chat-clone" 2>/dev/null || true
  echo "stopped $label"
  exit 0
fi

source_bundle="${GPUI_VERIFY_BUNDLE:-$("$root/scripts/gpui_capture_binary.sh" --bundle)}"
display="$(basename "$source_bundle" .app)"
destination="$HOME/Applications/$display.app"

mkdir -p "$HOME/Applications"
rm -rf "$destination"
if command -v ditto >/dev/null 2>&1; then
  ditto "$source_bundle" "$destination"
else
  cp -R "$source_bundle" "$destination"
fi

binary="$destination/Contents/MacOS/gpui-chat-clone"
if [ ! -x "$binary" ]; then
  echo "missing $binary after copying $source_bundle" >&2
  exit 2
fi

log_dir="$HOME/Library/Logs/gpui-capture"
log="$log_dir/verify-instance.log"
mkdir -p "$log_dir"

if launchctl list | grep -q "[[:space:]]$label$"; then
  echo "$label is already loaded; run --stop first" >&2
  exit 2
fi

# Prove which build this is, and that the copy resolves its own assets.
"$binary" --print-diagnostics

arguments=""
for argument in "$@"; do
  arguments="$arguments '$argument'"
done

# `launchctl submit` keeps the process alive after this shell exits, which
# Computer Use needs, and it never routes the launch through LaunchServices.
launchctl submit -l "$label" -- /bin/sh -c \
  "export HOME='$HOME'; export PATH='$PATH'; exec '$binary'$arguments >>'$log' 2>&1"

sleep 3
echo "launched $label from $destination"
echo "binary: $binary"
echo "log: $log"
pgrep -lf "$binary" | head -3 || echo "not running yet; check $log"
