#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="io.github.niteshbalusu11.Hunk"
MANIFEST_PATH="$ROOT_DIR/flatpak/$APP_ID.yaml"
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
BUILD_DIR="$TARGET_DIR/flatpak/build"

"$ROOT_DIR/scripts/build_flatpak.sh"

flatpak-builder --run "$BUILD_DIR" "$MANIFEST_PATH" hunk_desktop
