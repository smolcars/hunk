#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$ROOT_DIR/flatpak/vendor"
CONFIG_PATH="$ROOT_DIR/flatpak/cargo-config.toml"

rm -rf "$VENDOR_DIR"
mkdir -p "$ROOT_DIR/flatpak"

vendor_config="$(
  cd "$ROOT_DIR"
  cargo vendor --locked --versioned-dirs "$VENDOR_DIR"
)"

{
  printf '%s\n' "$vendor_config"
  printf '\n[net]\noffline = true\n'
} >"$CONFIG_PATH"

echo "Vendored Cargo dependencies into $VENDOR_DIR"
echo "Wrote Cargo config to $CONFIG_PATH"
