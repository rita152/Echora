#!/bin/sh
# Launch the dedicated ChatGPT reference instance used for CDP calibration.
#
# The running ChatGPT app cannot be reused: it has no debugging port, and it
# belongs to the user. A second instance needs three things the plain
# `open -n /Applications/ChatGPT.app --args --remote-debugging-port=…` recipe
# gets wrong on this machine:
#
#   * `launchctl submit` rather than `open -n`, so the instance outlives the
#     shell that started it and never becomes its own TCC responsible process
#     (the repository lives on an external volume).
#   * `CODEX_ELECTRON_USER_DATA_PATH`, because the app overrides Electron's
#     user-data directory from that variable; passing only
#     `--user-data-dir` still lands on the user's profile.
#   * the clone's `Singleton*` entries are removed: copying the profile copies
#     the lock/reply symlinks that point at the user's own window, and Chromium
#     then forwards the new instance there instead of starting one.
#   * the profile clone and the log live under `$HOME`, not in `artifacts/`:
#     a launchd-submitted process is denied access to this repository's
#     external volume, and a redirected log on that volume makes the job exit
#     before the app starts.
#
# Usage:
#   scripts/launch_chatgpt_reference.sh [--stop]
#
# Environment:
#   CHATGPT_REFERENCE_PORT        remote debugging port (default 9335)
#   CHATGPT_REFERENCE_USER_DATA   profile clone directory
#   CHATGPT_REFERENCE_SOURCE_DATA profile to clone from
#   CHATGPT_REFERENCE_LABEL       launchd label (default chatgpt-reference)
#   CHATGPT_REFERENCE_LOG_DIR     where the instance log is written
#   CHATGPT_REFERENCE_EXTRA_ARGS  extra Chromium switches; the default keeps a
#                                 covered window rendering, because Chromium
#                                 stops delivering input and frames to an
#                                 occluded (`visibilityState: hidden`) page
set -eu

port="${CHATGPT_REFERENCE_PORT:-9335}"
label="${CHATGPT_REFERENCE_LABEL:-chatgpt-reference}"
user_data="${CHATGPT_REFERENCE_USER_DATA:-$HOME/Library/Application Support/gpui-chatgpt-reference/user-data}"
source_data="${CHATGPT_REFERENCE_SOURCE_DATA:-$HOME/Library/Application Support/Codex}"
log_dir="${CHATGPT_REFERENCE_LOG_DIR:-$HOME/Library/Logs/gpui-capture}"
binary="/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"
extra_args="${CHATGPT_REFERENCE_EXTRA_ARGS:---disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling}"

if [ "${1:-}" = "--stop" ]; then
  launchctl remove "$label" 2>/dev/null || true
  for pid in $(pgrep -f "ChatGPT --user-data-dir=$user_data" 2>/dev/null || true); do
    for child in $(pgrep -P "$pid" 2>/dev/null || true); do
      kill "$child" 2>/dev/null || true
    done
    kill "$pid" 2>/dev/null || true
  done
  echo "stopped $label"
  exit 0
fi

[ -x "$binary" ] || { echo "missing $binary" >&2; exit 2; }
if lsof -nP -i ":$port" >/dev/null 2>&1; then
  echo "port $port is already in use; refusing to reuse another task's instance" >&2
  exit 2
fi
if launchctl list | grep -q "[[:space:]]$label$"; then
  echo "$label is already loaded; run --stop first" >&2
  exit 2
fi

if [ ! -d "$user_data/Default" ]; then
  mkdir -p "$user_data"
  cp -Rc "$source_data/." "$user_data/"
fi
# Never let the clone's single-instance plumbing point back at the user.
for name in SingletonCookie SingletonLock SingletonSocket; do
  [ -L "$user_data/$name" ] || [ -e "$user_data/$name" ] || continue
  mv -f "$user_data/$name" "$user_data/$name.stale-from-clone"
done

mkdir -p "$log_dir"
log="$log_dir/reference-instance-$port.log"

launchctl submit -l "$label" -- /bin/sh -c \
  "export HOME='$HOME'; export CODEX_ELECTRON_USER_DATA_PATH='$user_data'; exec '$binary' --user-data-dir='$user_data' --remote-debugging-port=$port $extra_args >>'$log' 2>&1"

for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  sleep 1
  if curl -s --max-time 2 "http://127.0.0.1:$port/json/version" | grep -q Browser; then
    echo "launched $label on port $port"
    echo "profile: $user_data"
    echo "log: $log"
    exit 0
  fi
done
echo "no debugging endpoint on port $port yet; check $log" >&2
tail -20 "$log" >&2 || true
exit 2
