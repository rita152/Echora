#!/bin/sh
# Read-only application identity and permission preflight diagnostics.
set -eu
app="${1:-/Applications/Echora.app}"
binary="$app/Contents/MacOS/gpui-chat-clone"
[ -x "$binary" ] || { echo "Missing Echora executable: $binary" >&2; exit 2; }
sw_vers
printf '\nApplication: %s\n' "$app"
codesign --verify --deep --strict "$app"
codesign -dvv -r- "$app" 2>&1
"$binary" --print-diagnostics
printf '\nPermission preflight applies to this diagnostic process. Check the running app log for its own denial; a false result does not establish a signature mismatch.\n'
