#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="io.github.BlixtWallet.Hunk"
MANIFEST_PATH="$ROOT_DIR/flatpak/$APP_ID.yaml"
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
BUILD_DIR="$TARGET_DIR/flatpak/build"
REPO_DIR="$TARGET_DIR/flatpak/repo"

if ! command -v flatpak-builder >/dev/null 2>&1; then
  echo "error: flatpak-builder is required" >&2
  exit 1
fi

"$ROOT_DIR/scripts/download_codex_runtime_unix.sh" linux >/dev/null
"$ROOT_DIR/scripts/validate_codex_runtime_bundle.sh" --strict --platform linux >/dev/null
"$ROOT_DIR/scripts/prepare_flatpak_vendor.sh"

mkdir -p "$BUILD_DIR" "$REPO_DIR"

flatpak-builder \
  --force-clean \
  --repo="$REPO_DIR" \
  --install-deps-from=flathub \
  "$BUILD_DIR" \
  "$MANIFEST_PATH"

echo "Built Flatpak repo at $REPO_DIR"
