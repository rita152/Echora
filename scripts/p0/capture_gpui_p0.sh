#!/bin/sh
# Capture the native GPUI components for the P0 pixel gate in fixed states.
#
# Usage: scripts/p0/capture_gpui_p0.sh [state ...]
set -eu

root="$(cd "$(dirname "$0")/../.." && pwd)"
bundle="$("$root/scripts/gpui_capture_binary.sh")"
out="$root/artifacts/p0-stage/actual"
mkdir -p "$out"

capture() {
  name="$1"; shift
  theme="$1"; shift
  target="$out/$theme-$name.png"
  GPUI_UI_PREFERENCES_PATH="$root/artifacts/capture-preferences.json" \
    "$bundle" --window-width=1440 --window-height=900 --theme="$theme" \
    --screenshot="$target" "$@" >"$out/$theme-$name.log" 2>&1 || {
      echo "capture failed: $theme $name" >&2
      tail -5 "$out/$theme-$name.log" >&2
      return 1
    }
  echo "captured $target"
}

for theme in dark light; do
  capture message-edit "$theme" --message-edit-state=open
  capture files-empty "$theme" --chat-search-state=files-empty
  capture files-results "$theme" --chat-search-state=files-result --chat-search-query=chat_search
  capture files-none "$theme" --chat-search-state=files-none --chat-search-query=zzzz-no-such-file
done
