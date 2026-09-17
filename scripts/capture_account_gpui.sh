#!/bin/zsh
# Capture the GPUI account surfaces for phase-two validation.
#
# The account menu and the logout confirmation are captured from the real
# application against the local Codex app-server, in both themes, at a fixed
# 1440x900 logical window with DPR 1. Each state waits for the real account read
# before the frame is saved.
set -euo pipefail

root="${0:A:h:h}"
cd "$root"

output="${1:-artifacts/account-phase/ui-validation/gpui}"
app="target/GPUI Capture.app/Contents/MacOS/gpui-chat-clone"
prefs="$PWD/artifacts/account-phase/ui-validation/capture-preferences.json"
mkdir -p "$output"

if [[ ! -x "$app" ]]; then
  echo "missing $app; build it with: cargo build --features screenshot" >&2
  exit 2
fi

capture() {
  local name="$1"
  shift
  local attempt
  for attempt in 1 2 3; do
    if GPUI_UI_PREFERENCES_PATH="$prefs" "$app" \
      --window-width=1440 --window-height=900 "$@" \
      "--screenshot=$output/$name.png" >/dev/null 2>&1; then
      return 0
    fi
    echo "retrying capture ($attempt/3): $name" >&2
    sleep 2
  done
  echo "capture failed: $name" >&2
  return 1
}

for theme in light dark; do
  capture "account-menu-$theme" "--theme=$theme" --profile-menu-open
  capture "logout-confirm-$theme" "--theme=$theme" --account-dialog=logout
done

count=$(find "$output" -maxdepth 1 -name '*.png' | wc -l | tr -d ' ')
if (( count < 4 )); then
  echo "expected four account captures, found $count" >&2
  exit 2
fi
echo "captured the account menu and logout confirmation in both themes"
