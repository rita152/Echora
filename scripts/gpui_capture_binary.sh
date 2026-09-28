#!/bin/sh
# Print the packaged capture bundle or binary of THIS worktree after packaging
# the current working tree.
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
# Every call repackages this worktree first (Cargo only rebuilds what changed),
# so a capture never drives a build older than the working tree.
# GPUI_CAPTURE_SKIP_BUILD=1 uses the existing package without rebuilding it;
# GPUI_CAPTURE_BUNDLE names an explicit bundle, used as-is and never repackaged;
# GPUI_CAPTURE_PROFILE=release packages the release binary.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

what="binary"
if [ "${1:-}" = "--bundle" ]; then
  what="bundle"
fi

bundle="$("$root/scripts/gpui_capture_name.sh")"
binary="$bundle/Contents/MacOS/gpui-chat-clone"

if [ "${GPUI_CAPTURE_SKIP_BUILD:-0}" != "1" ] && [ -z "${GPUI_CAPTURE_BUNDLE:-}" ]; then
  "$root/scripts/package_gpui_capture.sh" >&2
elif [ ! -x "$binary" ]; then
  echo "missing $binary; run scripts/package_gpui_capture.sh first" >&2
  exit 2
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
