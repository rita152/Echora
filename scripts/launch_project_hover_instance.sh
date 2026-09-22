#!/bin/sh
# Launch the dedicated sidebar hover-card verification instance from THIS
# worktree.
#
# macOS resolves `open -n "target/GPUI Capture.app"` against the LaunchServices
# database, so an identically named bundle from another build directory can be
# launched instead, and Computer Use binds by display name. Both hazards are
# removed by packaging a self-contained bundle whose name and identifier end
# with this worktree's slug: `scripts/package_gpui_capture.sh` embeds the
# assets and prints that identity. This launcher runs the packaged binary from
# the repository root and prints the exact path, pid, and log so the instance
# under test is unambiguous.
#
# Usage: scripts/launch_project_hover_instance.sh [extra app arguments...]
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
bundle="${GPUI_HOVER_BUNDLE:-$("$root/scripts/gpui_capture_name.sh")}"
binary="$bundle/Contents/MacOS/gpui-chat-clone"
log="${GPUI_HOVER_LOG:-$root/artifacts/project-hover/gpui/instance.log}"

if ! [ -d "$root/assets/icons" ]; then
  echo "missing $root/assets/icons; every icon would render blank" >&2
  exit 2
fi
if ! [ -x "$binary" ]; then
  echo "missing $binary; package the capture bundle first:" >&2
  echo "  scripts/package_gpui_capture.sh" >&2
  exit 2
fi
if pgrep -f "$binary" >/dev/null 2>&1; then
  echo "the hover verification instance from this worktree is already running:" >&2
  pgrep -lf "$binary" >&2
  exit 2
fi

mkdir -p "$(dirname "$log")"
cd "$root"
# Prove which build this is before driving it.
"$binary" --print-diagnostics
GPUI_UI_PREFERENCES_PATH="${GPUI_UI_PREFERENCES_PATH:-$root/artifacts/project-hover/preferences.json}" \
  nohup "$binary" "$@" >>"$log" 2>&1 &
echo "launched $binary as $!"
echo "log: $log"
