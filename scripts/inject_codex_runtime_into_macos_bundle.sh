#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="${1:-$ROOT_DIR/target/release/bundle/osx/Hunk.app}"
SOURCE_DIR="$ROOT_DIR/assets/codex-runtime"
TARGET_DIR="$APP_PATH/Contents/Resources/codex-runtime"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: runtime injection script is macOS-only" >&2
  exit 1
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "error: app bundle not found: $APP_PATH" >&2
  exit 1
fi
if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "error: source runtime directory missing: $SOURCE_DIR" >&2
  exit 1
fi

rm -rf "$TARGET_DIR"
mkdir -p "$(dirname "$TARGET_DIR")"
cp -R "$SOURCE_DIR" "$TARGET_DIR"

# Ensure unix runtime binaries preserve executable bit inside app resources.
if [[ -f "$TARGET_DIR/macos/codex" ]]; then
  chmod +x "$TARGET_DIR/macos/codex"
fi
if [[ -f "$TARGET_DIR/linux/codex" ]]; then
  chmod +x "$TARGET_DIR/linux/codex"
fi

echo "Injected Codex runtime assets into bundle:"
echo "  source: $SOURCE_DIR"
echo "  target: $TARGET_DIR"
