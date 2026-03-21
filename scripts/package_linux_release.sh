#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/linux_release_common.sh"
init_linux_release_paths

usage() {
  cat <<'EOF'
Build Linux release artifacts for Hunk.

Usage:
  ./scripts/package_linux_release.sh [--formats <csv>]

Formats:
  appimage
  tarball
  deb
  rpm
  all

Defaults to: appimage,tarball,deb,rpm
EOF
}

FORMATS="appimage,tarball,deb,rpm"

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
  FORMATS="appimage,tarball,deb,rpm"
fi

build_tarball=0
build_appimage=0
build_deb=0
build_rpm=0

IFS=',' read -r -a requested_formats <<<"$FORMATS"
for requested_format in "${requested_formats[@]}"; do
  case "$requested_format" in
    appimage)
      build_appimage=1
      ;;
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
      echo "error: unknown Linux package format '$requested_format'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

prepare_linux_release_bundle

artifact_paths=()

if [[ "$build_tarball" == "1" ]]; then
  rm -f "$ARCHIVE_PATH"
  tar -C "$(dirname "$PACKAGE_DIR")" -czf "$ARCHIVE_PATH" "$(basename "$PACKAGE_DIR")"
  echo "Created Linux tarball artifact at $ARCHIVE_PATH" >&2
  artifact_paths+=("$ARCHIVE_PATH")
fi

if [[ "$build_appimage" == "1" ]]; then
  rm -f "$APPIMAGE_PATH"
  echo "Building Linux AppImage package..." >&2
  build_linux_appimage
  echo "Created Linux AppImage artifact at $APPIMAGE_PATH" >&2
  artifact_paths+=("$APPIMAGE_PATH")
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
