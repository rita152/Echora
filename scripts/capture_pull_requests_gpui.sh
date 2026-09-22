#!/usr/bin/env bash
# Capture the native Pull Requests page with the dedicated capture bundle.
#
# Each state launches the newest executable with an explicit state flag; the
# app waits until the page has finished loading and the rendered frame differs
# from the flat startup fill, then writes one PNG plus a `.render.json` sidecar
# and exits. Output goes to artifacts/pull-requests-gpui/.
#
# Usage: scripts/capture_pull_requests_gpui.sh [light|dark|both] [core|full]
#
# `core` (default) captures the list, Summary, Code, file tree, and Activity
# states that the per-component pixel comparison checks. `full` adds every
# interaction state the checklist names, so the reference and the native build
# can be compared surface by surface.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

themes="${1:-both}"
states="${2:-${STATES:-core}}"
if [ "$themes" = "both" ]; then
  theme_list=(light dark)
else
  theme_list=("$themes")
fi

app="$(scripts/gpui_capture_binary.sh)"
wait_for_no_instance() {
  for _ in $(seq 1 40); do
    if ! pgrep -f "$app" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
}

capture() {
  local name="$1" theme="$2"
  shift 2
  wait_for_no_instance
  GPUI_UI_PREFERENCES_PATH="$root/artifacts/capture-preferences.json" \
    "$app" --theme="$theme" --window-width=1440 --window-height=900 \
    "$@" --screenshot="$root/artifacts/pull-requests-gpui/$name-$theme.png"
}

# A capture can time out while the machine is busy; retrying once has always
# been enough, and the retry is logged either way.
capture_with_retry() {
  local name="$1" theme="$2"
  shift 2
  if ! capture "$name" "$theme" "$@"; then
    echo "retrying $name ($theme)" >&2
    sleep 3
    capture "$name" "$theme" "$@"
  fi
}

for theme in "${theme_list[@]}"; do
  if [ "$states" = "full" ] || [ "$states" = "list" ]; then
    capture_with_retry list "$theme" --pull-requests
    capture_with_retry list-reviewing "$theme" --pull-requests --pull-requests-list-tab=reviewing
    capture_with_retry list-authored "$theme" --pull-requests --pull-requests-list-tab=authored
    capture_with_retry list-search "$theme" --pull-requests --pull-requests-search=workspace
    capture_with_retry list-search-empty "$theme" --pull-requests --pull-requests-search=zzzz
    capture_with_retry list-group-collapsed "$theme" \
      --pull-requests --pull-requests-collapse-group=authored
    capture_with_retry list-filter-menu "$theme" \
      --pull-requests --pull-requests-action=filter-menu
    capture_with_retry list-filter-status "$theme" \
      --pull-requests --pull-requests-action=filter-status
    capture_with_retry list-filter-repository "$theme" \
      --pull-requests --pull-requests-action=filter-repository
  fi
  if [ "$states" = "core" ] || [ "$states" = "full" ]; then
    capture_with_retry summary "$theme" --pull-requests --pull-requests-select=1
    capture_with_retry code "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-tab=code
    capture_with_retry code-tree "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-tab=code --pull-requests-file-tree
  fi
  if [ "$states" = "full" ]; then
    capture_with_retry summary-title-edit "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-action=title-edit
    capture_with_retry summary-status-menu "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-action=status-menu
    capture_with_retry summary-description-menu "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-action=description-menu
    capture_with_retry summary-reviewers "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-action=reviewers
    capture_with_retry code-review-options "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-tab=code \
      --pull-requests-action=review-options
    capture_with_retry code-split "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-tab=code \
      --pull-requests-action=split
    capture_with_retry code-collapse-all "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-tab=code \
      --pull-requests-action=collapse-all
    capture_with_retry code-inline-comment "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-tab=code \
      --pull-requests-action=inline-comment
    capture_with_retry review-tab "$theme" \
      --pull-requests --pull-requests-select=1 --pull-requests-tab=review
  fi
  if [ "$states" = "core" ] || [ "$states" = "full" ] || [ "$states" = "comments" ]; then
    # Only merged pull requests carry review comments here, and the two clients
    # order that list differently, so the activity state selects the pull
    # request by title and scrolls its detail column to the offset the reference
    # capture uses. Kept non-fatal so a flaky run never blocks the others.
    activity_title="Reconcile the app-server integration table"
    capture activity "$theme" \
      --pull-requests --pull-requests-status=merged \
      --pull-requests-title="$activity_title" --pull-requests-scroll=1640 || true
    capture_with_retry comment-menu "$theme" \
      --pull-requests --pull-requests-status=merged \
      --pull-requests-title="$activity_title" \
      --pull-requests-scroll=1640 --pull-requests-comment-menu || true
  fi
done
