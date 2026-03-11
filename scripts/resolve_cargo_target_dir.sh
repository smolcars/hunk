#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${1:-$(pwd)}"

normalize_output_path() {
  local path="$1"

  case "${OSTYPE:-}" in
    msys*|cygwin*)
      if command -v cygpath >/dev/null 2>&1; then
        cygpath -w "$path"
        return 0
      fi
      ;;
  esac

  printf '%s\n' "$path"
}

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  normalize_output_path "$CARGO_TARGET_DIR"
  exit 0
fi

if GIT_COMMON_DIR="$(git -C "$ROOT_DIR" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"; then
  SHARED_ROOT="$(cd "$GIT_COMMON_DIR/.." && pwd)"
  normalize_output_path "$SHARED_ROOT/target-shared"
  exit 0
fi

normalize_output_path "$ROOT_DIR/target"
