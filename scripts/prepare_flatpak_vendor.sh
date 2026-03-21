#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$ROOT_DIR/flatpak/vendor"
CONFIG_PATH="$ROOT_DIR/flatpak/cargo-config.toml"
PATCH_ROOT="$ROOT_DIR/flatpak/patches/gpui-component"

rm -rf "$VENDOR_DIR"
rm -rf "$ROOT_DIR/flatpak/patches"
mkdir -p "$ROOT_DIR/flatpak"

vendor_config="$(
  cd "$ROOT_DIR"
  cargo vendor --locked --versioned-dirs "$VENDOR_DIR"
)"

{
  printf '%s\n' "$vendor_config" | sed "s|$VENDOR_DIR|flatpak/vendor|g"
  cat <<'EOF'

[patch."https://github.com/niteshbalusu11/gpui-component"]
gpui-component = { path = "flatpak/patches/gpui-component/crates/ui" }
gpui-component-assets = { path = "flatpak/patches/gpui-component/crates/assets" }
gpui-component-macros = { path = "flatpak/patches/gpui-component/crates/macros" }
EOF
  printf '\n[net]\noffline = true\n'
} >"$CONFIG_PATH"

mkdir -p "$PATCH_ROOT/crates"
cp -R "$VENDOR_DIR/gpui-component-0.5.1" "$PATCH_ROOT/crates/ui"
cp -R "$VENDOR_DIR/gpui-component-assets-0.5.1" "$PATCH_ROOT/crates/assets"
cp -R "$VENDOR_DIR/gpui-component-macros-0.5.1" "$PATCH_ROOT/crates/macros"
printf '# Flatpak gpui-component workspace overlay\n' >"$PATCH_ROOT/README.md"

GPUI_COMPONENT_PANEL_PATH="$PATCH_ROOT/crates/ui/src/resizable/panel.rs"
if [[ -f "$GPUI_COMPONENT_PANEL_PATH" ]]; then
  perl -0pi -e 's|impl<T> From<T> for ResizablePanel\nwhere\n    T: Into<AnyElement>,\n\{\n    fn from\(value: T\) -> Self \{\n        resizable_panel\(\)\.child\(value\.into\(\)\)\n    \}\n\}\n|impl From<AnyElement> for ResizablePanel {\n    fn from(value: AnyElement) -> Self {\n        resizable_panel().child(value)\n    }\n}\n|g' "$GPUI_COMPONENT_PANEL_PATH"
fi

echo "Vendored Cargo dependencies into $VENDOR_DIR"
echo "Wrote gpui-component overlay to $PATCH_ROOT"
echo "Wrote Cargo config to $CONFIG_PATH"
