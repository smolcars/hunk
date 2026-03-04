#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_DIR="$ROOT_DIR/assets/codex-runtime"
STRICT="${HUNK_STRICT_CODEX_RUNTIME:-0}"
PLATFORM_FILTER=""

declare -A EXPECTED_BINARIES=(
  ["macos"]="codex"
  ["linux"]="codex"
  ["windows"]="codex.exe"
)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)
      STRICT=1
      shift
      ;;
    --platform)
      PLATFORM_FILTER="${2:-}"
      if [[ -z "$PLATFORM_FILTER" ]]; then
        echo "error: --platform requires a value (macos|linux|windows)"
        exit 1
      fi
      shift 2
      ;;
    *)
      echo "error: unknown argument '$1'"
      echo "usage: $0 [--strict] [--platform macos|linux|windows]"
      exit 1
      ;;
  esac
done

if [[ -n "$PLATFORM_FILTER" && -z "${EXPECTED_BINARIES[$PLATFORM_FILTER]+x}" ]]; then
  echo "error: invalid platform '$PLATFORM_FILTER' (expected macos|linux|windows)"
  exit 1
fi

echo "Validating Codex runtime layout in $RUNTIME_DIR (strict=$STRICT)"

for platform in "${!EXPECTED_BINARIES[@]}"; do
  if [[ -n "$PLATFORM_FILTER" && "$platform" != "$PLATFORM_FILTER" ]]; then
    continue
  fi

  platform_dir="$RUNTIME_DIR/$platform"
  binary_name="${EXPECTED_BINARIES[$platform]}"
  binary_path="$platform_dir/$binary_name"

  if [[ ! -d "$platform_dir" ]]; then
    echo "error: missing platform directory: $platform_dir"
    exit 1
  fi

  if [[ -f "$binary_path" ]]; then
    if [[ "$platform" != "windows" && ! -x "$binary_path" ]]; then
      if [[ "$STRICT" == "1" ]]; then
        echo "error: runtime binary is not executable: $binary_path"
        exit 1
      fi
      echo "warn: runtime binary is not executable: $binary_path"
      continue
    fi
    echo "ok: found $binary_path"
  else
    if [[ "$STRICT" == "1" ]]; then
      echo "error: missing runtime binary for $platform: $binary_path"
      exit 1
    fi
    echo "warn: runtime binary not present for $platform: $binary_path"
  fi
done

echo "Codex runtime layout check completed."
