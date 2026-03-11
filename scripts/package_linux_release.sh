#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${HUNK_LINUX_TARGET:-x86_64-unknown-linux-gnu}"
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
VERSION_LABEL="${HUNK_RELEASE_VERSION:-$("$ROOT_DIR/scripts/resolve_hunk_version.sh")}"
DIST_DIR="$TARGET_DIR/dist"
PACKAGE_DIR="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64"
ARCHIVE_PATH="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64.tar.gz"

echo "Downloading bundled Codex runtime for Linux..." >&2
"$ROOT_DIR/scripts/download_codex_runtime_unix.sh" linux >/dev/null
echo "Validating bundled Codex runtime for Linux..." >&2
"$ROOT_DIR/scripts/validate_codex_runtime_bundle.sh" --strict --platform linux >/dev/null
echo "Building Linux release binary..." >&2
"$ROOT_DIR/scripts/build_linux.sh" --target "$TARGET_TRIPLE"

rm -rf "$PACKAGE_DIR" "$ARCHIVE_PATH"
mkdir -p "$PACKAGE_DIR/codex-runtime/linux"

cp "$TARGET_DIR/$TARGET_TRIPLE/release/hunk_desktop" "$PACKAGE_DIR/hunk-desktop"
cp "$TARGET_DIR/$TARGET_TRIPLE/release/codex-runtime/linux/codex" "$PACKAGE_DIR/codex-runtime/linux/codex"
chmod +x "$PACKAGE_DIR/hunk-desktop" "$PACKAGE_DIR/codex-runtime/linux/codex"

mkdir -p "$DIST_DIR"
tar -C "$DIST_DIR" -czf "$ARCHIVE_PATH" "$(basename "$PACKAGE_DIR")"

echo "Created Linux release artifact at $ARCHIVE_PATH" >&2

printf '%s\n' "$ARCHIVE_PATH"
