#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_DIR="${1:-${HUNK_CEF_RUNTIME_DIR:-$ROOT_DIR/assets/browser-runtime/cef/macos/runtime}}"
APP_BUNDLE="${2:-${HUNK_CEF_APP_BUNDLE:-}}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "CEF macOS validation must run on macOS." >&2
  exit 1
fi

failures=0

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "missing file: $path" >&2
    failures=$((failures + 1))
  fi
}

require_executable() {
  local path="$1"
  if [[ ! -x "$path" ]]; then
    echo "missing executable: $path" >&2
    failures=$((failures + 1))
  fi
}

require_dir() {
  local path="$1"
  if [[ ! -d "$path" ]]; then
    echo "missing directory: $path" >&2
    failures=$((failures + 1))
  fi
}

validate_framework() {
  local framework_dir="$1"
  require_dir "$framework_dir"
  require_executable "$framework_dir/Chromium Embedded Framework"
  require_file "$framework_dir/Resources/Info.plist"
  require_file "$framework_dir/Resources/icudtl.dat"
  require_file "$framework_dir/Resources/chrome_100_percent.pak"
  require_file "$framework_dir/Resources/chrome_200_percent.pak"
  require_file "$framework_dir/Resources/en.lproj/locale.pak"
  require_file "$framework_dir/Libraries/libEGL.dylib"
  require_file "$framework_dir/Libraries/libGLESv2.dylib"

  local locale_count
  locale_count="$(find "$framework_dir/Resources" -path '*/locale.pak' -type f 2>/dev/null | wc -l | tr -d ' ')"
  if [[ "${locale_count:-0}" -lt 1 ]]; then
    echo "missing locale.pak resources under: $framework_dir/Resources" >&2
    failures=$((failures + 1))
  fi
}

validate_helper_apps() {
  local app_bundle="$1"
  local helper_prefix="$2"

  for suffix in "Helper" "Helper (GPU)" "Helper (Renderer)" "Helper (Plugin)" "Helper (Alerts)"; do
    local helper_name="$helper_prefix $suffix"
    local helper_app="$app_bundle/Contents/Frameworks/$helper_name.app"
    require_file "$helper_app/Contents/Info.plist"
    require_executable "$helper_app/Contents/MacOS/$helper_name"
  done
}

require_file "$RUNTIME_DIR/archive.json"
validate_framework "$RUNTIME_DIR/Chromium Embedded Framework.framework"

if [[ -n "$APP_BUNDLE" ]]; then
  require_dir "$APP_BUNDLE"
  require_file "$APP_BUNDLE/Contents/Info.plist"

  main_executable="${HUNK_CEF_APP_EXECUTABLE:-}"
  if [[ -z "$main_executable" ]]; then
    while IFS= read -r candidate; do
      if [[ -x "$candidate" ]]; then
        main_executable="$(basename "$candidate")"
        break
      fi
    done < <(find "$APP_BUNDLE/Contents/MacOS" -maxdepth 1 -type f 2>/dev/null)
  fi
  if [[ -z "$main_executable" ]]; then
    echo "missing main executable under: $APP_BUNDLE/Contents/MacOS" >&2
    failures=$((failures + 1))
  else
    require_executable "$APP_BUNDLE/Contents/MacOS/$main_executable"
    validate_helper_apps "$APP_BUNDLE" "${HUNK_CEF_HELPER_PREFIX:-$main_executable}"
  fi

  validate_framework "$APP_BUNDLE/Contents/Frameworks/Chromium Embedded Framework.framework"
fi

if [[ "$failures" -gt 0 ]]; then
  echo "CEF macOS validation failed with $failures issue(s)." >&2
  exit 1
fi

echo "CEF macOS validation passed."
