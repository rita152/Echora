#!/bin/sh
# Build and package the GPUI Capture bundle for THIS worktree.
#
# Two problems this closes at once:
#
#   * Assets are resolved at run time, so a bundle that is copied away from the
#     worktree that built it used to render every icon blank. The bundle now
#     carries its own copy under `Contents/Resources/assets`, which the app
#     prefers over anything on the build machine.
#   * Every worktree used to publish the same bundle name and identifier, so
#     `open` and Computer Use could pick a different worktree's build. The name
#     and identifier now end with a slug derived from this worktree's path, and
#     `scripts/gpui_capture_binary.sh` is the single way to resolve them.
#
# Prints the bundle path, identity, and `--print-diagnostics` output so a
# verification run can prove which build it is about to drive.
#
# GPUI_CAPTURE_PROFILE=release packages `target/release` instead of
# `target/debug`; GPUI_CAPTURE_BASE_NAME renames the bundle.
#
# Usage: scripts/package_gpui_capture.sh [extra app arguments for diagnostics]
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

profile="${GPUI_CAPTURE_PROFILE:-debug}"
case "$profile" in
  debug) build_args="" ;;
  release) build_args="--release" ;;
  *)
    echo "unsupported GPUI_CAPTURE_PROFILE: $profile (use debug or release)" >&2
    exit 2
    ;;
esac

# One naming rule for every script, so a profile never overwrites another and no
# two worktrees share a name or identifier.
bundle="$("$root/scripts/gpui_capture_name.sh")"
display="$(basename "$bundle" .app)"
identifier_suffix="$(printf '%s' "$display" | sed 's/^[^(]*(//; s/).*$//' | tr '-' '.')"

if ! [ -d "$root/assets/icons" ]; then
  echo "missing $root/assets/icons; refusing to package an iconless bundle" >&2
  exit 2
fi

# shellcheck disable=SC2086
cargo build --features screenshot $build_args

mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
python3 - "$root/scripts/gpui_capture_info.plist" "$bundle/Contents/Info.plist" \
  "$display" "com.openai.gpui-chat-clone.capture.$identifier_suffix" <<'PY'
import pathlib, sys

source, destination, display, identifier = sys.argv[1:5]
plist = pathlib.Path(source).read_text()
plist = plist.replace(
    "<string>com.openai.gpui-chat-clone.capture</string>",
    f"<string>{identifier}</string>",
)
plist = plist.replace("<string>GPUI Capture</string>", f"<string>{display}</string>")
pathlib.Path(destination).write_text(plist)
PY

cp "$root/target/$profile/gpui-chat-clone" "$bundle/Contents/MacOS/gpui-chat-clone"
# The bundle carries its own assets, so copying it anywhere keeps the icons.
mkdir -p "$bundle/Contents/Resources/assets"
cp -R "$root/assets/." "$bundle/Contents/Resources/assets/"
codesign --force --sign - "$bundle" >/dev/null 2>&1 || true

echo "bundle: $bundle"
echo "profile: $profile"
echo "bundle name: $display"
echo "bundle identifier: com.openai.gpui-chat-clone.capture.$identifier_suffix"
echo "assets copied: $(find "$bundle/Contents/Resources/assets/icons" -type f | wc -l | tr -d ' ') files"
"$bundle/Contents/MacOS/gpui-chat-clone" --print-diagnostics "$@"
