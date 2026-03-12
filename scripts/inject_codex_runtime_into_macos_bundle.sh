#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
APP_PATH="${1:-$TARGET_DIR/packager/Hunk.app}"
SOURCE_DIR="$ROOT_DIR/assets/codex-runtime"
RUNTIME_DEST_DIR="$APP_PATH/Contents/Resources/codex-runtime"

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

rm -rf "$RUNTIME_DEST_DIR"
mkdir -p "$(dirname "$RUNTIME_DEST_DIR")"
cp -R "$SOURCE_DIR" "$RUNTIME_DEST_DIR"

# Ensure unix runtime binaries preserve executable bit inside app resources.
if [[ -f "$RUNTIME_DEST_DIR/macos/codex" ]]; then
  chmod +x "$RUNTIME_DEST_DIR/macos/codex"
fi
if [[ -f "$RUNTIME_DEST_DIR/linux/codex" ]]; then
  chmod +x "$RUNTIME_DEST_DIR/linux/codex"
fi

echo "Injected Codex runtime assets into bundle:"
echo "  source: $SOURCE_DIR"
echo "  target: $RUNTIME_DEST_DIR"
