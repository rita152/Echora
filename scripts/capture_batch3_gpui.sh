#!/bin/zsh
# Capture Echora's batch-three states in both themes, at the reference's
# 1470x923 @2 window, into artifacts/batch3-<topic>-<date>/echora/.
#
# Every state is a deterministic fixture (--review-menu-state,
# --review-turn-state, --review-delivery-state, --shell-mode-state,
# --memory-status-state, --capabilities-state, --sections-state): nothing is
# written to app-server, and no model request is made. The app still starts
# `codex app-server`, so CODEX_HOME points at a copy of ~/.codex, never at the
# real one. The review submenu lists this repository's own branches, like the
# reference capture of the same project.
#
#   ECHORA_CODEX_HOME="$HOME/Library/Application Support/echora-batch3/codex-home" \
#     scripts/capture_batch3_gpui.sh 20260929
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
prefs_dir="artifacts/batch3-review-$date"
mkdir -p "$prefs_dir"
[[ -f "$prefs_dir/capture-preferences.json" ]] || cp artifacts/capture-preferences.json "$prefs_dir/capture-preferences.json"
prefs="$PWD/$prefs_dir/capture-preferences.json"

capture() {
  local topic="$1" name="$2"
  shift 2
  local output="artifacts/batch3-$topic-$date/echora"
  mkdir -p "$output"
  CODEX_HOME="$codex_home" GPUI_UI_PREFERENCES_PATH="$prefs" "$app" \
    --window-width=1470 --window-height=924 --language=en "$@" \
    "--screenshot=$output/$name.png" >/dev/null 2>&1 || echo "capture failed: $topic/$name" >&2
}

for theme in dark light; do
  # Reference states: slash, submenu, submenu-branch, submenu-escaped.
  capture review "slash-$theme" "--theme=$theme" --review-menu-state=slash --screenshot-delay-ms=1500
  capture review "submenu-$theme" "--theme=$theme" --review-menu-state=submenu --screenshot-delay-ms=1500
  capture review "submenu-branch-$theme" "--theme=$theme" --review-menu-state=submenu-branch --screenshot-delay-ms=1500
  capture review "submenu-escaped-$theme" "--theme=$theme" --review-menu-state=escaped --screenshot-delay-ms=1500
  # Echora-only states (no reference without a model request).
  for state in loading failed; do
    capture review "submenu-$state-$theme" "--theme=$theme" "--review-menu-state=$state" --screenshot-delay-ms=1500
  done
  for state in running finished; do
    capture review "turn-$state-$theme" "--theme=$theme" "--review-turn-state=$state" --screenshot-delay-ms=1500
  done
  for delivery in inline detached; do
    capture review "git-$delivery-$theme" "--theme=$theme" --settings-page=git-settings \
      "--review-delivery-state=$delivery" --screenshot-delay-ms=1500
  done
  # `!` shell mode has no reference entry point.
  for state in typing running completed failed timeout interrupted; do
    capture shell "$state-$theme" "--theme=$theme" "--shell-mode-state=$state" --screenshot-delay-ms=1500
  done
  for state in unsupported supported; do
    capture capabilities "web-search-$state-$theme" "--theme=$theme" --settings-page=agent \
      "--capabilities-state=$state" --screenshot-delay-ms=2500
  done
  for state in pending ready; do
    capture memory "settings-$state-$theme" "--theme=$theme" --settings-page=personalization \
      "--memory-status-state=settings-$state" --screenshot-delay-ms=2500
    capture memory "dialog-started-$state-$theme" "--theme=$theme" "--memory-status-state=$state" \
      --screenshot-delay-ms=1500
  done
  for state in sidebar hover menu thread-menu dialog-new dialog-edit; do
    capture sections "$state-$theme" "--theme=$theme" "--sections-state=$state" --screenshot-delay-ms=4000
  done
done
find artifacts/batch3-*-"$date"/echora -name '*.png' | wc -l | tr -d ' '
