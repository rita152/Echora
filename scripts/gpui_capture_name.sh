#!/bin/sh
# Print the capture bundle path of THIS worktree.
#
# This is the single place that knows the naming rule: the bundle name and
# identifier end with the worktree slug (a digest of the absolute path) so two
# worktrees can never be confused by `open` or by Computer Use, and the release
# profile gets its own bundle instead of overwriting the debug one.
#
# Environment:
#   GPUI_CAPTURE_PROFILE    debug (default) or release
#   GPUI_CAPTURE_BASE_NAME  bundle name before the suffix (default `GPUI Capture`)
#   GPUI_CAPTURE_BUNDLE     explicit bundle path, used as-is
set -eu

if [ -n "${GPUI_CAPTURE_BUNDLE:-}" ]; then
  printf '%s\n' "$GPUI_CAPTURE_BUNDLE"
  exit 0
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
slug="$(basename "$root" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9-')"
[ -n "$slug" ] || slug="worktree"
digest="$(printf '%s' "$root" | cksum | awk '{printf "%08x", $1}' | cut -c1-6)"
base_name="${GPUI_CAPTURE_BASE_NAME:-GPUI Capture}"

case "${GPUI_CAPTURE_PROFILE:-debug}" in
  debug) suffix="" ;;
  release) suffix="-release" ;;
  *)
    echo "unsupported GPUI_CAPTURE_PROFILE: ${GPUI_CAPTURE_PROFILE}" >&2
    exit 2
    ;;
esac

printf '%s\n' "$root/target/$base_name ($slug-$digest$suffix).app"
