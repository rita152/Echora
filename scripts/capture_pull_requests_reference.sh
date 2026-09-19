#!/usr/bin/env bash
# Capture the reference Pull Requests page in both themes.
#
# The main window follows `data-theme`, but the embedded diff viewer follows the
# app's own Appearance setting, so each theme is captured with the app actually
# switched to it. The app is left on the last theme captured.
#
#   CAPTURE_CDP_PORT=9412 scripts/capture_pull_requests_reference.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

port="${CAPTURE_CDP_PORT:-9412}"
export CHATGPT_CDP_HTTP="${CHATGPT_CDP_HTTP:-http://127.0.0.1:$port}"

states="${STATES:-list,detail,code}"
scale="${SCALE:-2}"

for theme in light dark; do
  node scripts/switch_chatgpt_theme.mjs "$theme"
  node scripts/cdp_capture_pull_requests.mjs --states="$states" --themes="$theme" --scale="$scale"
done
