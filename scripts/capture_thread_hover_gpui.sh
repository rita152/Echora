#!/usr/bin/env bash
# Capture the native sidebar task hover card and score it against the reference.
#
# The reference half needs the dedicated ChatGPT debug instance described in
# README.md; the native half builds and packages this worktree's capture bundle,
# opens the same task's card without a pointer, and writes one PNG plus a
# `.render.json` sidecar per theme.
#
# Usage: scripts/capture_thread_hover_gpui.sh [light|dark|both]
#
# THREAD_HOVER_TITLE  task whose card is captured (default: the title the
#                     checked-in reference capture used)
# CHATGPT_CDP_HTTP    dedicated debug instance; when set the script also
#                     refreshes the reference capture, otherwise it scores the
#                     existing one
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

themes="${1:-both}"
if [ "$themes" = "both" ]; then
  theme_list=(light dark)
else
  theme_list=("$themes")
fi

title="${THREAD_HOVER_TITLE:-复刻 ChatGPT 工作目录 hover 面板}"
reference="$root/artifacts/thread-hover/reference"
native="$root/artifacts/thread-hover/gpui"
compare="$root/artifacts/thread-hover/compare"

if [ -n "${CHATGPT_CDP_HTTP:-}" ]; then
  # The reference must render at the same device pixel ratio as the native
  # capture, or the score would compare two different rasterizations.
  CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_thread_hover.mjs \
    --output="$reference" --thread="$title" --dpr=1 --scale=1 --theme="${theme_list[0]}"
  if [ "${#theme_list[@]}" -gt 1 ]; then
    CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_thread_hover.mjs \
      --output="$reference" --thread="$title" --dpr=1 --scale=1
  fi
else
  echo "CHATGPT_CDP_HTTP is unset; scoring the existing capture in $reference" >&2
fi

scripts/package_gpui_capture.sh >/dev/null
app="$(scripts/gpui_capture_binary.sh)"
mkdir -p "$native"

capture() {
  local theme="$1"
  GPUI_UI_PREFERENCES_PATH="$root/artifacts/thread-hover/preferences.json" \
    "$app" --theme="$theme" --window-width=1440 --window-height=900 \
    --thread-hover-card="$title" --screenshot-frames=1200 \
    --screenshot="$native/$theme-card.png"
}

for theme in "${theme_list[@]}"; do
  capture "$theme"
  python3 scripts/compare_thread_hover.py \
    --reference "$reference" --gpui "$native" --output "$compare" --theme "$theme"
done
