#!/bin/sh
# Launch the dedicated ChatGPT reference instance used by the P0 capture
# scripts. Nothing here touches the user's own running ChatGPT: the instance is
# started from an explicit binary path with its own remote debugging port and
# an isolated PATH whose codex entry is the wire shim.
#
# P0_REFERENCE_PORT  remote debugging port (default 9333, must be free)
# P0_WIRE_LOG_DIR    where the app-server JSONL logs are written
#
# A second instance cannot reuse the user's own profile: the app forwards to
# the running window instead of starting. The launcher therefore clones the
# profile once (APFS clonefile, no extra disk use) and points the instance at
# the copy, so the user's profile is only ever read.
set -eu

port="${P0_REFERENCE_PORT:-9333}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
log_dir="${P0_WIRE_LOG_DIR:-$root/artifacts/p0-stage/wire/reference}"
shim_dir="$root/artifacts/p0-stage/wire-shims/reference"
user_data="${P0_REFERENCE_USER_DATA:-$root/artifacts/p0-stage/reference-user-data}"
source_data="${P0_REFERENCE_SOURCE_USER_DATA:-$HOME/Library/Application Support/Codex}"

if lsof -nP -i ":$port" >/dev/null 2>&1; then
  echo "port $port is already in use; refusing to reuse another task's instance" >&2
  exit 2
fi

if [ ! -d "$user_data/Default" ]; then
  mkdir -p "$user_data"
  cp -Rc "$source_data/." "$user_data/"
fi

mkdir -p "$log_dir" "$shim_dir"
ln -sf "$root/scripts/p0/codex_wire_shim.sh" "$shim_dir/codex"

# LaunchServices detaches the instance from this shell, which keeps it alive
# after the launcher returns; --env carries the wire shim into the app.
open -n \
  --env "P0_WIRE_LOG_DIR=$log_dir" \
  --env "P0_WIRE_ORIGIN=reference" \
  --env "P0_SHIM_HELPER=$root/scripts/p0/app_server_wire_shim.py" \
  --env "PATH=$shim_dir:$PATH" \
  --env "CODEX_CLI_PATH=$shim_dir/codex" \
  --env "CODEX_ELECTRON_USER_DATA_PATH=$user_data" \
  /Applications/ChatGPT.app \
  --args --user-data-dir="$user_data" --remote-debugging-port="$port"

echo "launched ChatGPT reference instance on port $port; logs in $log_dir"
