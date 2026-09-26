#!/bin/zsh
# Capture the GPUI sidebar activity view in the states
# `scripts/cdp_capture_activity_view.mjs` records from the reference, against
# the local Codex app-server's real chats, at the reference capture's 1470x924
# viewport, DPR 1 and 240 px sidebar.
#
# Usage: scripts/capture_activity_view_gpui.sh [output-dir] [reference-dir]
set -euo pipefail

root="${0:A:h:h}"
cd "$root"

output="${1:-artifacts/activity-view-26917/gpui}"
reference="${2:-artifacts/activity-view-26917/reference}"
prefs="$root/artifacts/activity-view-26917/capture-preferences.json"
app="$(scripts/gpui_capture_binary.sh)"
mkdir -p "$output"

if [[ ! -x "$app" ]]; then
  echo "missing $app; run scripts/package_gpui_capture.sh first" >&2
  exit 2
fi

capture() {
  local name="$1"
  shift
  local attempt
  for attempt in 1 2 3; do
    if GPUI_UI_PREFERENCES_PATH="$prefs" "$app" \
      --window-width=1470 --window-height=924 --sidebar-width=240 "$@" \
      "--screenshot=$root/$output/$name.png" >/dev/null 2>&1; then
      return 0
    fi
    echo "retrying capture ($attempt/3): $name" >&2
    sleep 2
  done
  echo "capture failed: $name" >&2
  return 1
}

for theme in dark light; do
  # Hover the same row the reference hovered.
  hover="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["rows"][0]["label"])' \
    "$reference/$theme-default.json")"
  capture "$theme-default" "--theme=$theme" --activity-open
  capture "$theme-hover" "--theme=$theme" "--activity-hover=$hover"
  capture "$theme-tooltip" "--theme=$theme" --activity-tooltip=bell
  capture "$theme-options" "--theme=$theme" --activity-options-open
  capture "$theme-scroll-100" "--theme=$theme" --activity-scroll=100
  capture "$theme-scroll-200" "--theme=$theme" --activity-scroll=200
done
echo "captured the activity view in both themes into $output"
