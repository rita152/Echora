#!/bin/sh
# Print the packaged capture bundle or binary of THIS worktree, packaging it on
# demand.
#
# Every worktree used to publish `target/GPUI Capture.app` with the same name
# and identifier, so a stale copy from another build directory could be
# launched instead, and Computer Use bound by display name. The packaged bundle
# now carries the worktree slug in its name and identifier and ships its own
# assets; this helper is the single place that knows where it lives.
#
# Usage:  binary="$(scripts/gpui_capture_binary.sh)"
#         bundle="$(scripts/gpui_capture_binary.sh --bundle)"
#         "$binary" --print-diagnostics
#
# GPUI_CAPTURE_BUNDLE overrides the location; GPUI_CAPTURE_SKIP_BUILD=1 uses an
# existing package without rebuilding it; GPUI_CAPTURE_PROFILE=release packages
# the release binary.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

what="binary"
if [ "${1:-}" = "--bundle" ]; then
  what="bundle"
fi

bundle="$("$root/scripts/gpui_capture_name.sh")"
binary="$bundle/Contents/MacOS/gpui-chat-clone"

if [ ! -x "$binary" ]; then
  if [ "${GPUI_CAPTURE_SKIP_BUILD:-0}" = "1" ]; then
    echo "missing $binary; run scripts/package_gpui_capture.sh first" >&2
    exit 2
  fi
  "$root/scripts/package_gpui_capture.sh" >&2
fi

if [ ! -x "$binary" ]; then
  echo "missing $binary after packaging" >&2
  exit 2
fi

if [ "$what" = "bundle" ]; then
  printf '%s\n' "$bundle"
else
  printf '%s\n' "$binary"
fi
