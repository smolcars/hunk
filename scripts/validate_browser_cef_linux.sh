#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Validate Linux CEF runtime and packaged Hunk browser layout.

Usage:
  ./scripts/validate_browser_cef_linux.sh <runtime-dir> [linux-package|linux-install-root] [path]
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

validate_runtime_dir() {
  local runtime_dir="$1"

  require_path "$runtime_dir/libcef.so" "Linux CEF libcef.so"
  require_path "$runtime_dir/icudtl.dat" "Linux CEF ICU data"
  require_path "$runtime_dir/resources.pak" "Linux CEF resources.pak"
  require_path "$runtime_dir/chrome_100_percent.pak" "Linux CEF chrome_100_percent.pak"
  require_path "$runtime_dir/chrome_200_percent.pak" "Linux CEF chrome_200_percent.pak"
  require_path "$runtime_dir/locales" "Linux CEF locales directory"
  if ! find "$runtime_dir/locales" -maxdepth 1 -type f -name '*.pak' | grep -q .; then
    echo "error: Linux CEF locales directory has no .pak files: $runtime_dir/locales" >&2
    exit 1
  fi
}

validate_package() {
  local package_path="$1"

  require_executable "$package_path/hunk-browser-helper" "Linux CEF helper"
  validate_runtime_dir "$package_path/lib"
}

validate_install_root() {
  local install_root="$1"
  local app_dir="$install_root/usr/lib/hunk-desktop"

  require_executable "$app_dir/hunk-browser-helper" "Linux installed CEF helper"
  validate_runtime_dir "$app_dir/lib"
}

if [[ $# -ne 1 && $# -ne 3 ]]; then
  usage >&2
  exit 1
fi

runtime_dir="$1"
validate_runtime_dir "$runtime_dir"

if [[ $# -eq 3 ]]; then
  mode="$2"
  bundle_path="$3"
  case "$mode" in
    linux-package)
      validate_package "$bundle_path"
      ;;
    linux-install-root)
      validate_install_root "$bundle_path"
      ;;
    *)
      echo "error: unknown Linux CEF validation mode '$mode'" >&2
      usage >&2
      exit 1
      ;;
  esac
fi

echo "CEF Linux validation passed." >&2
