#!/bin/zsh
# Capture Echora's batch-two states in both themes, at the reference's
# 1470x923 @2 window, into artifacts/batch2-<topic>-<date>/echora/.
#
# Every state is a deterministic fixture (--hooks-settings-state,
# --experimental-features-state, --memories-state, --find-bar-state): nothing
# is written to app-server. The app still starts `codex app-server`, so
# CODEX_HOME points at a copy of ~/.codex, never at the real one.
#
#   ECHORA_CODEX_HOME="$HOME/Library/Application Support/echora-batch2/codex-home" \
#     scripts/capture_batch2_gpui.sh 20260928
set -euo pipefail

root="${0:A:h:h}"
cd "$root"
date="${1:-$(date +%Y%m%d)}"
codex_home="${ECHORA_CODEX_HOME:?set ECHORA_CODEX_HOME to a copy of ~/.codex}"
if [[ "$codex_home" == "$HOME/.codex" || "$codex_home" == "$HOME/.codex/" ]]; then
  echo "refusing to capture against the real ~/.codex" >&2
  exit 2
fi
app="$(scripts/gpui_capture_binary.sh)"
prefs_dir="artifacts/batch2-hooks-$date"
mkdir -p "$prefs_dir"
[[ -f "$prefs_dir/capture-preferences.json" ]] || cp artifacts/capture-preferences.json "$prefs_dir/capture-preferences.json"
prefs="$PWD/$prefs_dir/capture-preferences.json"

capture() {
  local topic="$1" name="$2"
  shift 2
  local output="artifacts/batch2-$topic-$date/echora"
  mkdir -p "$output"
  CODEX_HOME="$codex_home" GPUI_UI_PREFERENCES_PATH="$prefs" "$app" \
    --window-width=1470 --window-height=924 --language=en "$@" \
    "--screenshot=$output/$name.png" >/dev/null 2>&1 || echo "capture failed: $topic/$name" >&2
}

for theme in dark light; do
  for state in overview dialog expanded issues trusted overridden refreshed empty; do
    capture hooks "$state-$theme" "--theme=$theme" --settings-page=hooks-settings "--hooks-settings-state=$state"
  done
  for state in list restart empty loading; do
    capture features "$state-$theme" "--theme=$theme" --settings-page=agent \
      "--experimental-features-state=$state" --screenshot-delay-ms=1500
  done
  for state in settings-on settings-off settings-unavailable delete-confirm deleted; do
    capture memories "$state-$theme" "--theme=$theme" --settings-page=personalization \
      "--memories-state=$state" --screenshot-delay-ms=2500
  done
  for state in slash dialog-new dialog-started dialog-generate-off rollback; do
    capture memories "$state-$theme" "--theme=$theme" "--memories-state=$state"
  done
  for state in open results second capped none; do
    capture find "$state-$theme" "--theme=$theme" "--find-bar-state=$state" --screenshot-delay-ms=800
  done
done
find artifacts/batch2-*-"$date"/echora -name '*.png' | wc -l | tr -d ' '
