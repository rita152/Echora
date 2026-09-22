#!/bin/sh
# Launch the packaged GPUI capture bundle for a P0 UX run. The instance gets
# its own PATH whose codex entry is the wire shim, so the native app's real
# app-server traffic lands next to the reference log.
#
# Usage: scripts/p0/launch_gpui_capture.sh [extra app arguments...]
set -eu

root="$(cd "$(dirname "$0")/../.." && pwd)"
bundle="$("$root/scripts/gpui_capture_binary.sh")"
log_dir="${P0_WIRE_LOG_DIR:-$root/artifacts/p0-stage/wire/gpui}"
shim_dir="$root/artifacts/p0-stage/wire-shims/gpui"
prefs="${GPUI_UI_PREFERENCES_PATH:-$root/artifacts/capture-preferences.json}"

if [ ! -x "$bundle" ]; then
  echo "missing $bundle; run scripts/package_gpui_capture.sh first" >&2
  exit 2
fi

mkdir -p "$log_dir" "$shim_dir"
ln -sf "$root/scripts/p0/codex_wire_shim.sh" "$shim_dir/codex"

env \
  P0_WIRE_LOG_DIR="$log_dir" \
  P0_WIRE_ORIGIN="gpui" \
  P0_SHIM_HELPER="$root/scripts/p0/app_server_wire_shim.py" \
  PATH="$shim_dir:$PATH" \
  GPUI_UI_PREFERENCES_PATH="$prefs" \
  "$bundle" "$@" >"$log_dir/gpui-instance.log" 2>&1 &

echo "launched GPUI capture instance; logs in $log_dir"
