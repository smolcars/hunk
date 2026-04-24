#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "CEF smoke is currently implemented for macOS only." >&2
  exit 1
fi

CEF_RS_REPO="${HUNK_CEF_RS_REPO:-https://github.com/tauri-apps/cef-rs.git}"
CEF_RS_REV="${HUNK_CEF_RS_REV:-f20249dd2e34afdc0102af347f30f0218dd67e7b}"
CEF_RS_DIR="${HUNK_CEF_RS_DIR:-/tmp/cef-rs}"
CEF_RUNTIME_DIR="${HUNK_CEF_RUNTIME_DIR:-$ROOT_DIR/assets/browser-runtime/cef/macos/runtime}"
CEF_BUNDLE_DIR="${HUNK_CEF_SMOKE_BUNDLE_DIR:-$ROOT_DIR/target/browser-cef-smoke}"
HUNK_SMOKE_CRATE="$ROOT_DIR/tools/browser-cef-smoke/Cargo.toml"
HUNK_SMOKE_APP="$CEF_BUNDLE_DIR/hunk-browser-cef-smoke.app"
RUN_SECONDS="${HUNK_CEF_SMOKE_RUN_SECONDS:-0}"

if ! command -v ninja >/dev/null 2>&1; then
  echo "cef-rs requires Ninja to build cef-dll-sys. Run through 'nix develop' after updating the dev shell." >&2
  exit 1
fi

if [[ ! -d "$CEF_RS_DIR/.git" ]]; then
  git clone --depth=1 "$CEF_RS_REPO" "$CEF_RS_DIR"
fi

git -C "$CEF_RS_DIR" fetch --depth=1 origin "$CEF_RS_REV"
git -C "$CEF_RS_DIR" checkout --detach "$CEF_RS_REV" >/dev/null

mkdir -p "$CEF_RUNTIME_DIR"
mkdir -p "$CEF_BUNDLE_DIR"

if [[ "${HUNK_CEF_SKIP_EXPORT:-0}" == "1" && -f "$CEF_RUNTIME_DIR/archive.json" ]]; then
  echo "Using existing CEF runtime at $CEF_RUNTIME_DIR"
else
  echo "Exporting CEF runtime to $CEF_RUNTIME_DIR"
  (
    cd "$CEF_RS_DIR"
    cargo run -p export-cef-dir -- --force "$CEF_RUNTIME_DIR"
  )
fi

export CEF_PATH="$CEF_RUNTIME_DIR"
export DYLD_FALLBACK_LIBRARY_PATH="${DYLD_FALLBACK_LIBRARY_PATH:-}:$CEF_RUNTIME_DIR:$CEF_RUNTIME_DIR/Chromium Embedded Framework.framework/Libraries"

echo "Building bundled cef-rs OSR smoke app in $CEF_BUNDLE_DIR"
(
  cd "$CEF_RS_DIR"
  cargo run --bin bundle-cef-app -- cef-osr -o "$CEF_BUNDLE_DIR"
)

SMOKE_APP="$CEF_BUNDLE_DIR/cef-osr.app"
if [[ ! -d "$SMOKE_APP" ]]; then
  echo "Expected smoke app was not created: $SMOKE_APP" >&2
  exit 1
fi

if [[ "$RUN_SECONDS" != "0" ]]; then
  echo "Opening $SMOKE_APP for $RUN_SECONDS seconds"
  open -n "$SMOKE_APP"
  sleep "$RUN_SECONDS"
  pkill -f "$SMOKE_APP/Contents/MacOS" || true
  pkill -f "cef-osr" || true
fi

echo "CEF smoke bundle ready: $SMOKE_APP"

echo "Building Hunk-owned CEF smoke binary"
cargo build --manifest-path "$HUNK_SMOKE_CRATE"

HUNK_SMOKE_BIN="$ROOT_DIR/tools/browser-cef-smoke/target/debug/hunk-browser-cef-smoke"
if [[ ! -x "$HUNK_SMOKE_BIN" ]]; then
  echo "Expected Hunk smoke binary was not built: $HUNK_SMOKE_BIN" >&2
  exit 1
fi

rm -rf "$HUNK_SMOKE_APP"
mkdir -p "$HUNK_SMOKE_APP/Contents/MacOS"
mkdir -p "$HUNK_SMOKE_APP/Contents/Frameworks"
cp "$HUNK_SMOKE_BIN" "$HUNK_SMOKE_APP/Contents/MacOS/hunk-browser-cef-smoke"
cp -R "$CEF_RUNTIME_DIR/Chromium Embedded Framework.framework" "$HUNK_SMOKE_APP/Contents/Frameworks/"
cat > "$HUNK_SMOKE_APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>hunk-browser-cef-smoke</string>
  <key>CFBundleIdentifier</key>
  <string>dev.hunk.browser-cef-smoke</string>
  <key>CFBundleName</key>
  <string>Hunk Browser CEF Smoke</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
</dict>
</plist>
PLIST

for helper_suffix in "Helper" "Helper (GPU)" "Helper (Renderer)" "Helper (Plugin)" "Helper (Alerts)"; do
  HELPER_NAME="hunk-browser-cef-smoke $helper_suffix"
  HELPER_ID_SUFFIX="$(printf '%s' "$helper_suffix" | tr '[:upper:]' '[:lower:]' | tr -cd '[:alnum:]')"
  HELPER_APP="$HUNK_SMOKE_APP/Contents/Frameworks/$HELPER_NAME.app"
  mkdir -p "$HELPER_APP/Contents/MacOS"
  cp "$HUNK_SMOKE_BIN" "$HELPER_APP/Contents/MacOS/$HELPER_NAME"
  cat > "$HELPER_APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$HELPER_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>dev.hunk.browser-cef-smoke.$HELPER_ID_SUFFIX</string>
  <key>CFBundleName</key>
  <string>$HELPER_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>LSBackgroundOnly</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
</dict>
</plist>
PLIST
done

echo "Validating Hunk-owned CEF smoke app layout"
"$ROOT_DIR/scripts/validate_browser_cef_macos.sh" "$CEF_RUNTIME_DIR" "$HUNK_SMOKE_APP"

echo "Running Hunk-owned CEF smoke app"
"$HUNK_SMOKE_APP/Contents/MacOS/hunk-browser-cef-smoke"
echo "Hunk-owned CEF smoke passed: $HUNK_SMOKE_APP"
