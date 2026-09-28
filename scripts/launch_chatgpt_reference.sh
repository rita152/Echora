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
#   * the legacy sidebar is pinned once the window is up: ChatGPT picks it or
#     the navigation rail with a Statsig gate fetched anew at every launch, and
#     Echora recreates the legacy sidebar. `cdp_pin_reference_layout.mjs` only
#     overrides the gate in the page's memory, so the launch fails (and stops
#     the instance) unless the sidebar renders as `legacy`. A reload of the
#     page drops the pin; rerun that script before capturing again.
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
#   CHATGPT_REFERENCE_CODEX_HOME  optional CODEX_HOME clone for the instance. The
#                                 app keeps its Appearance theme, persisted atoms
#                                 (the last opened pull request route, read
#                                 state, view toggles) in `~/.codex`, which the
#                                 user's own ChatGPT reads; with a clone the
#                                 reference can switch themes and select rows
#                                 without touching it. Cloned from ~/.codex on
#                                 first use; delete it to refresh.
#   CHATGPT_REFERENCE_EXTRA_ARGS  extra Chromium switches; the default keeps a
#                                 covered window rendering, because Chromium
#                                 stops delivering input and frames to an
#                                 occluded (`visibilityState: hidden`) page
#   CHATGPT_REFERENCE_WIRE_LOG_DIR
#                                 optional: route the app's `codex app-server`
#                                 through `scripts/p0/codex_wire_shim.sh` and
#                                 write its JSON-RPC logs here. Keep it under
#                                 `$HOME`; the shim itself is copied next to
#                                 the profile clone for the same reason as the
#                                 log below.
#   CHATGPT_REFERENCE_WIRE_ORIGIN label recorded in every wire log line
#                                 (default reference)
set -eu

port="${CHATGPT_REFERENCE_PORT:-9335}"
label="${CHATGPT_REFERENCE_LABEL:-chatgpt-reference}"
user_data="${CHATGPT_REFERENCE_USER_DATA:-$HOME/Library/Application Support/gpui-chatgpt-reference/user-data}"
source_data="${CHATGPT_REFERENCE_SOURCE_DATA:-$HOME/Library/Application Support/Codex}"
log_dir="${CHATGPT_REFERENCE_LOG_DIR:-$HOME/Library/Logs/gpui-capture}"
codex_home="${CHATGPT_REFERENCE_CODEX_HOME:-}"
wire_log_dir="${CHATGPT_REFERENCE_WIRE_LOG_DIR:-}"
binary="/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"
extra_args="${CHATGPT_REFERENCE_EXTRA_ARGS:---disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling}"

script_dir="$(cd "$(dirname "$0")" && pwd)"
pin_script="$script_dir/cdp_pin_reference_layout.mjs"

stop_instance() {
  launchctl remove "$label" 2>/dev/null || true
  for pid in $(pgrep -f "ChatGPT --user-data-dir=$user_data" 2>/dev/null || true); do
    for child in $(pgrep -P "$pid" 2>/dev/null || true); do
      kill "$child" 2>/dev/null || true
    done
    kill "$pid" 2>/dev/null || true
  done
}

if [ "${1:-}" = "--stop" ]; then
  stop_instance
  echo "stopped $label"
  exit 0
fi

[ -x "$binary" ] || { echo "missing $binary" >&2; exit 2; }
command -v node >/dev/null 2>&1 || { echo "node is required to pin the legacy sidebar" >&2; exit 2; }
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

codex_home_export=""
if [ -n "$codex_home" ]; then
  if [ ! -f "$codex_home/config.toml" ]; then
    mkdir -p "$codex_home"
    cp -Rc "$HOME/.codex/." "$codex_home/"
  fi
  codex_home_export="export CODEX_HOME='$codex_home';"
fi

wire_export=""
if [ -n "$wire_log_dir" ]; then
  # The app spawns `codex` from CODEX_CLI_PATH. A launchd job cannot read this
  # repository's external volume, so the shim and its helper run from a copy.
  shim_dir="$(dirname "$user_data")/wire-shim-$port"
  mkdir -p "$shim_dir" "$wire_log_dir"
  cp "$script_dir/p0/codex_wire_shim.sh" "$shim_dir/codex"
  cp "$script_dir/p0/app_server_wire_shim.py" "$shim_dir/app_server_wire_shim.py"
  chmod +x "$shim_dir/codex"
  wire_export="export PATH='$shim_dir:$PATH'; export CODEX_CLI_PATH='$shim_dir/codex'; export P0_WIRE_LOG_DIR='$wire_log_dir'; export P0_WIRE_ORIGIN='${CHATGPT_REFERENCE_WIRE_ORIGIN:-reference}';"
fi

mkdir -p "$log_dir"
log="$log_dir/reference-instance-$port.log"

launchctl submit -l "$label" -- /bin/sh -c \
  "export HOME='$HOME'; export CODEX_ELECTRON_USER_DATA_PATH='$user_data'; $codex_home_export $wire_export exec '$binary' --user-data-dir='$user_data' --remote-debugging-port=$port $extra_args >>'$log' 2>&1"

endpoint_ready=""
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  sleep 1
  if curl -s --max-time 2 "http://127.0.0.1:$port/json/version" | grep -q Browser; then
    endpoint_ready=1
    break
  fi
done
if [ -z "$endpoint_ready" ]; then
  echo "no debugging endpoint on port $port yet; check $log" >&2
  tail -20 "$log" >&2 || true
  exit 2
fi

# The main window, its Statsig client and the sidebar mount a few seconds
# after the endpoint answers; the pin script retries until the sidebar renders.
layout_report="$log_dir/reference-instance-$port-layout.json"
if ! CHATGPT_CDP_HTTP="http://127.0.0.1:$port" node "$pin_script" --layout=legacy --wait=60 >"$layout_report" 2>&1; then
  echo "the reference did not render the legacy sidebar; stopping $label" >&2
  cat "$layout_report" >&2 || true
  stop_instance
  exit 3
fi

echo "launched $label on port $port"
echo "profile: $user_data"
[ -z "$codex_home" ] || echo "codex home: $codex_home"
[ -z "$wire_log_dir" ] || echo "wire logs: $wire_log_dir"
echo "layout: legacy sidebar pinned in memory ($(grep -o '"networkValue": [a-z]*' "$layout_report" | sed 's/.*: //;s/true/network value: rail/;s/false/network value: legacy/'))"
echo "log: $log"
