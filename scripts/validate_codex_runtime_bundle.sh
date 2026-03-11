#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_DIR="$ROOT_DIR/assets/codex-runtime"
STRICT="${HUNK_STRICT_CODEX_RUNTIME:-0}"
PLATFORM_FILTER=""

expected_binary_name() {
  case "$1" in
    macos|linux)
      printf '%s\n' "codex"
      ;;
    windows)
      printf '%s\n' "codex.exe"
      ;;
    *)
      return 1
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)
      STRICT=1
      shift
      ;;
    --platform)
      PLATFORM_FILTER="${2:-}"
      if [[ -z "$PLATFORM_FILTER" ]]; then
        echo "error: --platform requires a value (macos|linux|windows)" >&2
        exit 1
      fi
      shift 2
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      echo "usage: $0 [--strict] [--platform macos|linux|windows]" >&2
      exit 1
      ;;
  esac
done

if [[ -n "$PLATFORM_FILTER" ]] && ! expected_binary_name "$PLATFORM_FILTER" >/dev/null 2>&1; then
  echo "error: invalid platform '$PLATFORM_FILTER' (expected macos|linux|windows)" >&2
  exit 1
fi

echo "Validating Codex runtime layout in $RUNTIME_DIR (strict=$STRICT)" >&2

for platform in macos linux windows; do
  if [[ -n "$PLATFORM_FILTER" && "$platform" != "$PLATFORM_FILTER" ]]; then
    continue
  fi

  platform_dir="$RUNTIME_DIR/$platform"
  binary_name="$(expected_binary_name "$platform")"
  binary_path="$platform_dir/$binary_name"

  if [[ ! -d "$platform_dir" ]]; then
    echo "error: missing platform directory: $platform_dir" >&2
    exit 1
  fi

  if [[ -f "$binary_path" ]]; then
    if [[ "$platform" != "windows" && ! -x "$binary_path" ]]; then
      if [[ "$STRICT" == "1" ]]; then
        echo "error: runtime binary is not executable: $binary_path" >&2
        exit 1
      fi
      echo "warn: runtime binary is not executable: $binary_path" >&2
      continue
    fi
    echo "ok: found $binary_path" >&2
  else
    if [[ "$STRICT" == "1" ]]; then
      echo "error: missing runtime binary for $platform: $binary_path" >&2
      exit 1
    fi
    echo "warn: runtime binary not present for $platform: $binary_path" >&2
  fi
done

echo "Codex runtime layout check completed." >&2
