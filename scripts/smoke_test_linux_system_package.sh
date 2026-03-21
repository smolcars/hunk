#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/linux_release_common.sh"
init_linux_release_paths

usage() {
  cat <<'EOF'
Smoke-test Linux system packages in a container.

Usage:
  ./scripts/smoke_test_linux_system_package.sh <deb|rpm> [package-path]
EOF
}

detect_container_runner() {
  if command -v docker >/dev/null 2>&1; then
    printf '%s\n' "docker"
    return 0
  fi

  if command -v podman >/dev/null 2>&1; then
    printf '%s\n' "podman"
    return 0
  fi

  return 1
}

run_deb_smoke_test() {
  local package_path="$1"
  local runner="$2"
  local image="${HUNK_DEB_SMOKE_TEST_IMAGE:-ubuntu:24.04}"
  local package_dir package_name

  package_dir="$(dirname "$package_path")"
  package_name="$(basename "$package_path")"

  "$runner" run --rm \
    -v "$package_dir:/artifacts:ro" \
    "$image" \
    /bin/bash -lc "
      set -euo pipefail
      export DEBIAN_FRONTEND=noninteractive
      apt-get update
      apt-get install -y /artifacts/$package_name
      test -x /usr/bin/$PACKAGE_NAME
      test -x /usr/lib/$PACKAGE_NAME/$REAL_BINARY_NAME
      ldd /usr/lib/$PACKAGE_NAME/$REAL_BINARY_NAME | tee /tmp/hunk-ldd.txt
      if grep -Fq 'not found' /tmp/hunk-ldd.txt; then
        echo 'error: unresolved runtime dependency after Debian install' >&2
        exit 1
      fi
    "
}

run_rpm_smoke_test() {
  local package_path="$1"
  local runner="$2"
  local image="${HUNK_RPM_SMOKE_TEST_IMAGE:-fedora:latest}"
  local package_dir package_name

  package_dir="$(dirname "$package_path")"
  package_name="$(basename "$package_path")"

  "$runner" run --rm \
    -v "$package_dir:/artifacts:ro" \
    "$image" \
    /bin/bash -lc "
      set -euo pipefail
      dnf install -y /artifacts/$package_name
      test -x /usr/bin/$PACKAGE_NAME
      test -x /usr/lib/$PACKAGE_NAME/$REAL_BINARY_NAME
      ldd /usr/lib/$PACKAGE_NAME/$REAL_BINARY_NAME | tee /tmp/hunk-ldd.txt
      if grep -Fq 'not found' /tmp/hunk-ldd.txt; then
        echo 'error: unresolved runtime dependency after RPM install' >&2
        exit 1
      fi
    "
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

format="$1"
package_path="${2:-}"
runner="$(detect_container_runner || true)"
if [[ -z "$runner" ]]; then
  echo "error: install Docker or Podman to run Linux package smoke tests" >&2
  exit 1
fi

case "$format" in
  deb)
    package_path="${package_path:-$DEB_PATH}"
    if [[ ! -f "$package_path" ]]; then
      echo "error: Debian package not found at $package_path" >&2
      echo "hint: run ./scripts/package_linux_release.sh --formats deb first" >&2
      exit 1
    fi
    run_deb_smoke_test "$package_path" "$runner"
    ;;
  rpm)
    package_path="${package_path:-$RPM_PATH}"
    if [[ ! -f "$package_path" ]]; then
      echo "error: RPM package not found at $package_path" >&2
      echo "hint: run ./scripts/package_linux_release.sh --formats rpm first" >&2
      exit 1
    fi
    run_rpm_smoke_test "$package_path" "$runner"
    ;;
  *)
    echo "error: unknown package format '$format'" >&2
    usage >&2
    exit 1
    ;;
esac

echo "Linux $format smoke test passed for $package_path" >&2
