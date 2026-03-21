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
  printf '%s\n' "$vendor_config" | sed "s|$VENDOR_DIR|flatpak/vendor|g"
  printf '\n[net]\noffline = true\n'
} >"$CONFIG_PATH"

GPUI_COMPONENT_ASSETS_DIR="$VENDOR_DIR/gpui-component-assets-0.5.1/assets"
GPUI_COMPONENT_COMPAT_ASSETS_DIR="$VENDOR_DIR/assets"
GPUI_COMPONENT_PANEL_PATH="$VENDOR_DIR/gpui-component-0.5.1/src/resizable/panel.rs"

if [[ -d "$GPUI_COMPONENT_ASSETS_DIR" ]]; then
  rm -rf "$GPUI_COMPONENT_COMPAT_ASSETS_DIR"
  mkdir -p "$GPUI_COMPONENT_COMPAT_ASSETS_DIR"
  cp -R "$GPUI_COMPONENT_ASSETS_DIR" "$GPUI_COMPONENT_COMPAT_ASSETS_DIR/assets"
fi

if [[ -f "$GPUI_COMPONENT_PANEL_PATH" ]]; then
  perl -0pi -e 's|impl<T> From<T> for ResizablePanel\nwhere\n    T: Into<AnyElement>,\n\{\n    fn from\(value: T\) -> Self \{\n        resizable_panel\(\)\.child\(value\.into\(\)\)\n    \}\n\}\n|impl From<AnyElement> for ResizablePanel {\n    fn from(value: AnyElement) -> Self {\n        resizable_panel().child(value)\n    }\n}\n|g' "$GPUI_COMPONENT_PANEL_PATH"
fi

echo "Vendored Cargo dependencies into $VENDOR_DIR"
echo "Wrote Cargo config to $CONFIG_PATH"
