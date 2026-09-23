#!/usr/bin/env bash
# Capture the native task rename panel and score it against the reference.
#
# The reference half needs the dedicated ChatGPT debug instance described in
# README.md; the native half builds and packages this worktree's capture
# bundle, opens the same task's panel without a pointer, and writes one PNG
# plus a `.render.json` sidecar per theme.
#
# Usage: scripts/capture_thread_rename_gpui.sh [light|dark|both]
#
# THREAD_RENAME_TITLE   task whose panel is captured (default: the title the
#                       checked-in reference capture used)
# THREAD_RENAME_ID      thread id to open behind the panel, so both captures
#                       share the same transcript
# THREAD_RENAME_SCROLL  scroll offset from the bottom for that transcript
# CHATGPT_CDP_HTTP      dedicated debug instance; when set the script also
#                       refreshes the reference capture, otherwise it scores
#                       the existing one
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

themes="${1:-both}"
if [ "$themes" = "both" ]; then
  theme_list=(light dark)
else
  theme_list=("$themes")
fi

title="${THREAD_RENAME_TITLE:-44}"
thread_id="${THREAD_RENAME_ID:-local:01a0c7f2-32b5-7c21-b481-181ebf45dbfc}"
scroll="${THREAD_RENAME_SCROLL:-0}"
reference="$root/artifacts/thread-rename/reference"
native="$root/artifacts/thread-rename/gpui"
compare="$root/artifacts/thread-rename/compare"

scripts/package_gpui_capture.sh >/dev/null
app="$(scripts/gpui_capture_binary.sh)"
mkdir -p "$native"

# The native capture runs first: the reference instance keeps a writer on every
# task it opens, and the native app then cannot resume the same task.
capture() {
  local theme="$1"
  GPUI_UI_PREFERENCES_PATH="$root/artifacts/thread-rename/preferences.json" \
    "$app" --theme="$theme" --language=en --window-width=1440 --window-height=900 \
    --resume-thread="$thread_id" --resume-scroll-from-bottom="$scroll" \
    --thread-rename="$title" --screenshot="$native/$theme-panel.png"
}

for theme in "${theme_list[@]}"; do
  capture "$theme"
done

if [ -n "${CHATGPT_CDP_HTTP:-}" ]; then
  # The reference renders at the native capture's device pixel ratio, so both
  # sides rasterize glyphs and the ring identically.
  dpr="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['dpr'])" \
    "$native/${theme_list[0]}-panel.png.render.json")"
  for theme in "${theme_list[@]}"; do
    CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_thread_rename.mjs \
      --output="$reference" --thread="$title" --dpr="$dpr" --scale=1 --theme="$theme"
  done
else
  echo "CHATGPT_CDP_HTTP is unset; scoring the existing capture in $reference" >&2
fi

for theme in "${theme_list[@]}"; do
  python3 scripts/compare_thread_rename.py \
    --reference "$reference" --gpui "$native" --output "$compare" --theme="$theme"
done
