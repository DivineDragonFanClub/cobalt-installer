#!/usr/bin/env bash
#
# Bundle the macOS .app and register it as the nxm:// handler.
#
# dx's [deep_links] config only wires a URL scheme into the iOS/Android manifests, not the desktop
# .app Info.plist, so we inject CFBundleURLTypes ourselves after bundling (same idea as the Android
# launcher-icon overlay in android/build.sh). Without this, macOS won't hand nxm:// links (from the
# NexusMods "Mod Manager Download" button) to the app, and the Opened-event handler never fires.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> dx bundle (macOS .app)"
dx bundle --platform desktop --package-types macos "$@"

APP=$(find target/dx -type d -name "*.app" -path "*bundle/macos*" | head -1)
[ -n "$APP" ] || { echo "couldn't find the bundled .app under target/dx"; exit 1; }
PLIST="$APP/Contents/Info.plist"
PB=/usr/libexec/PlistBuddy

echo "==> registering nxm:// in $PLIST"
# Idempotent: drop any existing entry first so re-runs don't stack duplicates.
$PB -c "Delete :CFBundleURLTypes" "$PLIST" 2>/dev/null || true
$PB -c "Add :CFBundleURLTypes array" "$PLIST"
$PB -c "Add :CFBundleURLTypes:0 dict" "$PLIST"
$PB -c "Add :CFBundleURLTypes:0:CFBundleURLName string com.divinedragonfanclub.nxm" "$PLIST"
$PB -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes array" "$PLIST"
$PB -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes:0 string nxm" "$PLIST"
plutil -lint "$PLIST" >/dev/null

# Register with LaunchServices so nxm:// resolves to this app without moving it to /Applications.
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$LSREGISTER" -f "$APP" || true

echo "==> done: $APP"
echo
echo "Test the handler:"
echo "  1. sign in to NexusMods once in the app (paste your API key) so it has your key"
echo "  2. run:  open 'nxm://fireemblemengage/mods/2/files/2?key=test&expires=1'"
echo "     (a real link comes from the site's \"Mod Manager Download\" button)"
