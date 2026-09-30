#!/bin/zsh
# Capture Echora's batch-four states in both themes, at the reference's
# 1470x923 @2 window, into artifacts/batch4-<topic>-<date>/echora/.
#
# The captures resume the fixture threads of scripts/batch4_reference_fixture.py,
# so ECHORA_CODEX_HOME must be a copy of the reference clone that holds them
# (never ~/.codex): the sidebar chips, the summary panel and its pull requests
# come from that copy's attachments and the live `gh` state, like the
# reference. Background terminals cannot be resumed after a restart, so the
# background states add the reference's `BGTERM-LONG` turn as a fixture
# (--batch4-state=background:*): no clean request and no model request is made.
# The panel states switch the fixture checkout to the pull request's branch,
# as the reference capture does, and back to main afterwards.
#
#   ECHORA_CODEX_HOME="$HOME/Library/Application Support/echora-batch4/codex-home" \
#     scripts/capture_batch4_gpui.sh 20260929 [chips panel background]
set -euo pipefail

root="${0:A:h:h}"
cd "$root"
date="${1:-$(date +%Y%m%d)}"
shift $(( $# > 0 ? 1 : 0 ))
topics=("$@")
(( ${#topics} )) || topics=(chips panel background)
codex_home="${ECHORA_CODEX_HOME:?set ECHORA_CODEX_HOME to a copy of the batch-four reference CODEX_HOME}"
if [[ "${codex_home:A}" == "${HOME}/.codex" ]]; then
  echo "refusing to capture against the real ~/.codex" >&2
  exit 2
fi
fixture="artifacts/batch4-fixture-20260929.json"
thread_id() {
  python3 -c 'import json,sys; print(next(t["threadId"] for t in json.load(open(sys.argv[1]))["threads"] if t["title"] == sys.argv[2]))' "$fixture" "$1"
}
thread_field() {
  python3 -c 'import json,sys; print(next(t[sys.argv[3]] for t in json.load(open(sys.argv[1]))["threads"] if t["title"] == sys.argv[2]))' "$fixture" "$1" "$2"
}
background="$(thread_id 'Fixture background terminal')"
failing="$(thread_id 'Fixture PR failing')"
failing_cwd="$(thread_field 'Fixture PR failing' cwd)"
failing_branch="$(thread_field 'Fixture PR failing' branch)"

app="$(scripts/gpui_capture_binary.sh)"
prefs_dir="artifacts/batch4-panel-$date"
mkdir -p "$prefs_dir"
[[ -f "$prefs_dir/capture-preferences.json" ]] || cp artifacts/capture-preferences.json "$prefs_dir/capture-preferences.json"

capture() {
  local topic="$1" name="$2" thread="$3"
  shift 3
  local output="artifacts/batch4-$topic-$date/echora"
  mkdir -p "$output"
  # Each capture starts from the same preferences: "closed" unpins the panel.
  local prefs="$output/.preferences-$name.json"
  cp "$prefs_dir/capture-preferences.json" "$prefs"
  CODEX_HOME="$codex_home" GPUI_UI_PREFERENCES_PATH="$PWD/$prefs" "$app" \
    --window-width=1470 --window-height=924 --language=en "--resume-thread=$thread" "$@" \
    "--screenshot=$output/$name.png" >/dev/null 2>"$output/$name.log" || echo "capture failed: $topic/$name" >&2
  rm -f "$prefs"
}

for theme in dark light; do
  if (( ${topics[(Ie)chips]} )); then
    capture chips "sidebar-$theme" "$background" "--theme=$theme" --batch4-state=chips:sidebar
    capture chips "hover-merged-$theme" "$background" "--theme=$theme" --batch4-state=chips:hover-merged
    # Echora lists this project last, so its row is only in view (with room
    # for its card) with the sidebar scrolled to the bottom.
    capture chips "hover-failing-$theme" "$background" "--theme=$theme" --sidebar-bottom \
      --batch4-state=chips:hover-failing
  fi
  if (( ${topics[(Ie)panel]} )); then
    git -C "$failing_cwd" switch -q "$failing_branch"
    {
      for state in open section-hover closed reopened pr-row-hover pr-actions-menu unmatched-pr-hover; do
        capture panel "$state-$theme" "$failing" "--theme=$theme" "--batch4-state=panel:$state"
      done
    } always {
      git -C "$failing_cwd" switch -q main
    }
  fi
  if (( ${topics[(Ie)background]} )); then
    for state in section row-hover row-focus card-running terminal-tab stopping stop-failed card-stopped card-finished; do
      capture background "$state-$theme" "$background" "--theme=$theme" "--batch4-state=background:$state"
    done
  fi
done
find artifacts/batch4-*-"$date"/echora -name '*.png' | wc -l | tr -d ' '
