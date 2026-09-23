#!/usr/bin/env bash
# Capture the native conversation user-message navigation rail and score it
# against the reference.
#
# The reference half needs the dedicated ChatGPT debug instance described in
# README.md; the native half packages this worktree's capture bundle and opens
# the same task twice per theme: once at rest and once with one marker hovered,
# which is what drives the preview card.
#
# Usage: scripts/capture_user_message_rail_gpui.sh [light|dark|both]
#
# USER_MESSAGE_RAIL_THREAD  thread id behind the rail
# USER_MESSAGE_RAIL_TITLE   sidebar title, used by the reference capture
# USER_MESSAGE_RAIL_HOVER   1-based marker to hover in both builds (default 4)
# CHATGPT_CDP_HTTP          dedicated debug instance; when set the script also
#                           refreshes the reference capture, otherwise it scores
#                           the existing one
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

themes="${1:-both}"
if [ "$themes" = "both" ]; then
  theme_list=(light dark)
else
  theme_list=("$themes")
fi

thread="${USER_MESSAGE_RAIL_THREAD:-local:01a0cd83-cfdc-7bc0-af5a-16abe529e026}"
title="${USER_MESSAGE_RAIL_TITLE:-提交本次修改}"
hover="${USER_MESSAGE_RAIL_HOVER:-4}"
window="${USER_MESSAGE_RAIL_WINDOW:-1800x1000}"
reference="$root/artifacts/user-message-rail/reference"
native="$root/artifacts/user-message-rail/gpui"
compare="$root/artifacts/user-message-rail/compare"

scripts/package_gpui_capture.sh >/dev/null
app="$(scripts/gpui_capture_binary.sh)"
mkdir -p "$native"

# The native captures run first: the reference instance keeps a writer on every
# task it opens, and the native app then cannot resume the same task.
capture() {
  local theme="$1" name="$2" extra="$3"
  # One preferences file per capture: a shared file left behind by an
  # interrupted run changes the next run's startup state.
  GPUI_UI_PREFERENCES_PATH="$native/preferences-$theme-$name.json" \
    "$app" --theme="$theme" --language=en \
    --window-width="${window%x*}" --window-height="${window#*x}" \
    --resume-thread="$thread" \
    --user-message-navigation-jump=1 \
    $extra \
    --screenshot="$native/$theme-$name.png"
}

for theme in "${theme_list[@]}"; do
  capture "$theme" window ""
  capture "$theme" hover "--user-message-navigation-hover=$hover"
done

if [ -n "${CHATGPT_CDP_HTTP:-}" ]; then
  dpr="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['dpr'])" \
    "$native/${theme_list[0]}-window.png.render.json")"
  for theme in "${theme_list[@]}"; do
    CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_user_message_rail.mjs \
      --output="$reference" --thread="$title" --window="$window" --dpr="$dpr" \
      --scale=1 --hover="$hover" --theme="$theme"
  done
else
  echo "CHATGPT_CDP_HTTP is unset; scoring the existing capture in $reference" >&2
fi

for theme in "${theme_list[@]}"; do
  python3 scripts/compare_user_message_rail.py \
    --reference "$reference" --gpui "$native" --output "$compare" --theme="$theme"
done
