#!/bin/sh
# Keep drivers on the internal disk and in the invoking app's TCC responsibility.
# Usage: scripts/run_native_verification.sh workspace|live|links|resize|zoom [driver arguments]
# Check grants without opening an app or requesting consent:
#   scripts/run_native_verification.sh workspace --check-permissions
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
case "${1:-}" in
  workspace) source="$root/scripts/verify_workspace_controls_native.swift" ;;
  live) source="$root/scripts/capture_live_conversation_native.swift" ;;
  links) source="$root/scripts/verify_link_preview_native.swift" ;;
  resize) source="$root/scripts/verify_resize_native.swift" ;;
  zoom) source="$root/scripts/verify_zoom_native.swift" ;;
  *) echo "usage: $0 workspace|live|links|resize|zoom [driver arguments]" >&2; exit 2 ;;
esac
driver="$1"
shift

if [ "$(uname -s)" != Darwin ]; then
  echo "native verification requires macOS" >&2
  exit 2
fi
if [ -z "${DEVELOPER_DIR:-}" ] && [ -d /Library/Developer/CommandLineTools ]; then
  export DEVELOPER_DIR=/Library/Developer/CommandLineTools
fi

digest="$(printf '%s' "$root" | cksum | awk '{print $1}')"
cache="$HOME/Library/Caches/Echora/native-verification/$digest"
mkdir -p "$cache"
binary="$cache/$driver"
if [ ! -x "$binary" ] || [ "$source" -nt "$binary" ]; then
  build="$(mktemp -d "$cache/build.XXXXXX")"
  trap 'rm -rf "$build"' EXIT HUP INT TERM
  /usr/bin/xcrun swiftc "$source" -o "$build/$driver"
  mv -f "$build/$driver" "$binary"
  rm -rf "$build"
  trap - EXIT HUP INT TERM
fi

cd "$root"
exec "$binary" "$@"
