#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Validate packaged release bundle layouts.

Usage:
  ./scripts/validate_release_bundle_layout.sh <macos-app|linux-package|linux-appdir|linux-install-root> <path>
EOF
}

require_path() {
  local target_path="$1"
  local description="$2"

  if [[ ! -e "$target_path" ]]; then
    echo "error: missing $description: $target_path" >&2
    exit 1
  fi
}

require_executable() {
  local target_path="$1"
  local description="$2"

  require_path "$target_path" "$description"
  if [[ ! -x "$target_path" ]]; then
    echo "error: expected executable $description: $target_path" >&2
    exit 1
  fi
}

forbid_helix_paths() {
  local root_path="$1"

  if find "$root_path" -print | grep -E '(^|/)(helix|hx-runtime|queries|grammars)(/|$)' >/dev/null; then
    echo "error: forbidden Helix-era bundle content found under $root_path" >&2
    find "$root_path" -print | grep -E '(^|/)(helix|hx-runtime|queries|grammars)(/|$)' >&2
    exit 1
  fi
}

validate_macos_app() {
  local app_path="$1"

  require_executable "$app_path/Contents/MacOS/hunk_desktop" "macOS app binary"
  require_executable \
    "$app_path/Contents/Resources/codex-runtime/macos/codex" \
    "bundled macOS Codex runtime"
  forbid_helix_paths "$app_path"
}

validate_linux_package() {
  local package_path="$1"

  require_executable "$package_path/hunk_desktop_bin" "Linux packaged binary"
  require_executable "$package_path/hunk-desktop" "Linux launcher"
  require_executable \
    "$package_path/codex-runtime/linux/codex" \
    "bundled Linux Codex runtime"
  require_path "$package_path/lib" "Linux shared library directory"
  forbid_helix_paths "$package_path"
}

validate_linux_appdir() {
  local appdir_path="$1"

  require_executable "$appdir_path/AppRun" "AppImage AppRun launcher"
  require_executable "$appdir_path/usr/bin/hunk_desktop_bin" "AppImage binary"
  require_executable "$appdir_path/usr/bin/hunk_desktop" "AppImage launcher"
  require_executable \
    "$appdir_path/usr/lib/hunk_desktop/codex-runtime/linux/codex" \
    "AppImage bundled Codex runtime"
  require_path "$appdir_path/usr/lib" "AppImage shared library directory"
  forbid_helix_paths "$appdir_path"
}

validate_linux_install_root() {
  local install_root="$1"

  require_executable "$install_root/usr/bin/hunk-desktop" "Linux installed launcher wrapper"
  require_executable "$install_root/usr/bin/hunk_desktop" "Linux installed launcher alias"
  require_executable "$install_root/usr/lib/hunk-desktop/hunk_desktop_bin" "Linux installed binary"
  require_executable "$install_root/usr/lib/hunk-desktop/hunk-desktop" "Linux installed private launcher"
  require_executable \
    "$install_root/usr/lib/hunk-desktop/codex-runtime/linux/codex" \
    "Linux installed bundled Codex runtime"
  require_path "$install_root/usr/lib/hunk-desktop/lib" "Linux installed shared library directory"
  require_path "$install_root/usr/share/applications/hunk-desktop.desktop" "Linux desktop entry"
  require_path "$install_root/usr/share/icons/hicolor/1024x1024/apps/hunk-desktop.png" "Linux desktop icon"
  forbid_helix_paths "$install_root"
}

if [[ $# -ne 2 ]]; then
  usage >&2
  exit 1
fi

mode="$1"
bundle_path="$2"

case "$mode" in
  macos-app)
    validate_macos_app "$bundle_path"
    ;;
  linux-package)
    validate_linux_package "$bundle_path"
    ;;
  linux-appdir)
    validate_linux_appdir "$bundle_path"
    ;;
  linux-install-root)
    validate_linux_install_root "$bundle_path"
    ;;
  *)
    echo "error: unknown validation mode '$mode'" >&2
    usage >&2
    exit 1
    ;;
esac

echo "Validated $mode bundle layout at $bundle_path" >&2
