#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/linux_release_common.sh"
init_linux_release_paths

usage() {
  cat <<'EOF'
Build Zed-like Linux release artifacts for Hunk.

This experimental path keeps the current Hunk bundle layout, but aligns more
closely with Zed's Linux release strategy:
- no AppImage output
- no forced extra graphics runtime libraries
- launcher exposes only the package's private lib directory for dlopen users

Usage:
  ./scripts/package_linux_release_zed_like.sh [--formats <csv>]

Formats:
  tarball
  deb
  rpm
  all

Defaults to: tarball
EOF
}

FORMATS="tarball"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --formats)
      FORMATS="${2:-}"
      if [[ -z "$FORMATS" ]]; then
        echo "error: --formats requires a comma-separated value" >&2
        exit 1
      fi
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ ",$FORMATS," == *",all,"* ]]; then
  FORMATS="tarball,deb,rpm"
fi

build_tarball=0
build_deb=0
build_rpm=0

IFS=',' read -r -a requested_formats <<<"$FORMATS"
for requested_format in "${requested_formats[@]}"; do
  case "$requested_format" in
    tarball)
      build_tarball=1
      ;;
    deb)
      build_deb=1
      ;;
    rpm)
      build_rpm=1
      ;;
    "")
      ;;
    *)
      echo "error: unsupported Zed-like Linux package format '$requested_format'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

write_zed_like_linux_launcher() {
  cat >"$PACKAGED_LAUNCHER_PATH" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_BINARY_NAME="${HUNK_LINUX_REAL_BINARY_NAME:-hunk_desktop_bin}"
REAL_BINARY_PATH="$SCRIPT_DIR/$REAL_BINARY_NAME"
PRIVATE_LIB_DIR="$SCRIPT_DIR/lib"
HOST_GRAPHICS_LIBRARY_PATHS="${HUNK_LINUX_HOST_GRAPHICS_LIBRARY_PATHS:-/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:/usr/lib64}"

append_library_path() {
  local candidate="$1"

  [[ -n "$candidate" && -d "$candidate" ]] || return 0

  case ":${LD_LIBRARY_PATH:-}:" in
    *":$candidate:"*) ;;
    *)
      export LD_LIBRARY_PATH="${LD_LIBRARY_PATH:+$LD_LIBRARY_PATH:}$candidate"
      ;;
  esac
}

if [[ ! -x "$REAL_BINARY_PATH" ]]; then
  echo "error: expected Linux GUI binary at $REAL_BINARY_PATH" >&2
  exit 1
fi

append_library_path "$PRIVATE_LIB_DIR"

IFS=':' read -r -a host_graphics_library_paths <<<"$HOST_GRAPHICS_LIBRARY_PATHS"
for host_graphics_library_path in "${host_graphics_library_paths[@]}"; do
  append_library_path "$host_graphics_library_path"
done

exec "$REAL_BINARY_PATH" "$@"
EOF
  chmod +x "$PACKAGED_LAUNCHER_PATH"
}

prepare_zed_like_linux_release_bundle() {
  prepare_linux_release_build_inputs

  rm -rf "$PACKAGE_DIR"
  mkdir -p "$PACKAGE_DIR/codex-runtime/linux" "$PACKAGE_LIB_DIR" "$DIST_DIR"

  cp "$BINARY_SOURCE_PATH" "$PACKAGED_BINARY_PATH"
  cp "$BROWSER_HELPER_SOURCE_PATH" "$PACKAGED_BROWSER_HELPER_PATH"
  cp "$CODEX_SOURCE_PATH" "$PACKAGED_CODEX_PATH"
  cp -R "$BROWSER_CEF_SOURCE_DIR"/. "$PACKAGE_LIB_DIR"/
  chmod +x "$PACKAGED_BINARY_PATH" "$PACKAGED_BROWSER_HELPER_PATH" "$PACKAGED_CODEX_PATH"

  write_zed_like_linux_launcher

  echo "Bundling Linux shared libraries into experimental Zed-like release bundle..." >&2
  bundle_linux_runtime_dependencies "$BINARY_SOURCE_PATH" "$PACKAGE_LIB_DIR"
  bundle_linux_runtime_dependencies "$BROWSER_HELPER_SOURCE_PATH" "$PACKAGE_LIB_DIR"
  bundle_linux_dynamic_runtime_dependencies "$PACKAGE_LIB_DIR"
  patch_linux_runtime_paths "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR" '$ORIGIN/lib'
  patch_linux_runtime_paths "$PACKAGED_BROWSER_HELPER_PATH" "$PACKAGE_LIB_DIR" '$ORIGIN/lib'
  validate_linux_runtime_bundle "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR"
  validate_linux_runtime_bundle "$PACKAGED_BROWSER_HELPER_PATH" "$PACKAGE_LIB_DIR"
  "$ROOT_DIR/scripts/validate_release_bundle_layout.sh" linux-package "$PACKAGE_DIR"
  "$ROOT_DIR/scripts/validate_browser_cef_linux.sh" "$BROWSER_CEF_SOURCE_DIR" linux-package "$PACKAGE_DIR" >/dev/null
}

prepare_zed_like_linux_release_bundle

artifact_paths=()

if [[ "$build_tarball" == "1" ]]; then
  rm -f "$ARCHIVE_PATH"
  tar -C "$(dirname "$PACKAGE_DIR")" -czf "$ARCHIVE_PATH" "$(basename "$PACKAGE_DIR")"
  echo "Created experimental Linux tarball artifact at $ARCHIVE_PATH" >&2
  artifact_paths+=("$ARCHIVE_PATH")
fi

if [[ "$build_deb" == "1" || "$build_rpm" == "1" ]]; then
  prepare_linux_system_install_root
fi

if [[ "$build_deb" == "1" ]]; then
  build_linux_deb_package
  artifact_paths+=("$DEB_PATH")
fi

if [[ "$build_rpm" == "1" ]]; then
  build_linux_rpm_package
  artifact_paths+=("$RPM_PATH")
fi

printf '%s\n' "${artifact_paths[@]}"
