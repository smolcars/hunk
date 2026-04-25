#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
Package the staged macOS CEF runtime into a Hunk.app bundle.

Usage:
  ./scripts/package_browser_cef_macos.sh <Hunk.app> [runtime-dir] [helper-binary]
EOF
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "CEF macOS packaging must run on macOS." >&2
  exit 1
fi

if [[ $# -lt 1 || $# -gt 3 ]]; then
  usage >&2
  exit 1
fi

APP_BUNDLE="$1"
RUNTIME_DIR="${2:-${HUNK_CEF_RUNTIME_DIR:-$ROOT_DIR/assets/browser-runtime/cef/macos/runtime}}"
HELPER_BINARY="${3:-${HUNK_BROWSER_HELPER_BINARY:-$ROOT_DIR/target/aarch64-apple-darwin/release/hunk-browser-helper}}"
FRAMEWORK_SOURCE="$RUNTIME_DIR/Chromium Embedded Framework.framework"
FRAMEWORK_DEST="$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework"

if [[ ! -d "$APP_BUNDLE/Contents" ]]; then
  echo "error: expected app bundle Contents directory at $APP_BUNDLE/Contents" >&2
  exit 1
fi
if [[ ! -d "$FRAMEWORK_SOURCE" ]]; then
  echo "error: staged CEF framework not found at $FRAMEWORK_SOURCE" >&2
  exit 1
fi
if [[ ! -x "$HELPER_BINARY" ]]; then
  echo "error: hunk-browser-helper binary not found or not executable at $HELPER_BINARY" >&2
  exit 1
fi

mkdir -p "$APP_BUNDLE/Contents/Frameworks"
rm -rf "$FRAMEWORK_DEST"
cp -R "$FRAMEWORK_SOURCE" "$FRAMEWORK_DEST"

for helper_suffix in "Helper" "Helper (GPU)" "Helper (Renderer)" "Helper (Plugin)" "Helper (Alerts)"; do
  HELPER_NAME="Hunk Browser $helper_suffix"
  HELPER_ID_SUFFIX="$(printf '%s' "$helper_suffix" | tr '[:upper:]' '[:lower:]' | tr -cd '[:alnum:]')"
  HELPER_APP="$APP_BUNDLE/Contents/Frameworks/$HELPER_NAME.app"

  rm -rf "$HELPER_APP"
  mkdir -p "$HELPER_APP/Contents/MacOS"
  cp "$HELPER_BINARY" "$HELPER_APP/Contents/MacOS/$HELPER_NAME"
  chmod +x "$HELPER_APP/Contents/MacOS/$HELPER_NAME"

  cat > "$HELPER_APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$HELPER_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>com.niteshbalusu.hunk.browser.$HELPER_ID_SUFFIX</string>
  <key>CFBundleName</key>
  <string>$HELPER_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>LSBackgroundOnly</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
</dict>
</plist>
PLIST
done

HUNK_CEF_HELPER_PREFIX="Hunk Browser" \
  "$ROOT_DIR/scripts/validate_browser_cef_macos.sh" "$RUNTIME_DIR" "$APP_BUNDLE"
echo "Packaged macOS CEF runtime into $APP_BUNDLE"
