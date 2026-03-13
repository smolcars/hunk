#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_PATH="$ROOT_DIR/assets/codex-runtime/macos/codex"
SOURCE_INPUT="${1:-${HUNK_CODEX_RUNTIME_SOURCE:-}}"

if [[ -z "$SOURCE_INPUT" ]]; then
  exec "$ROOT_DIR/scripts/download_codex_runtime_unix.sh" macos
fi

find_native_codex_from_wrapper() {
  local wrapper_path="$1"
  local wrapper_real="$wrapper_path"
  local search_root=""
  local arch
  local triple

  if command -v realpath >/dev/null 2>&1; then
    wrapper_real="$(realpath "$wrapper_path" 2>/dev/null || echo "$wrapper_path")"
  fi

  if ! grep -q "Unified entry point for the Codex CLI" "$wrapper_real" 2>/dev/null; then
    return 1
  fi

  search_root="$(cd "$(dirname "$wrapper_real")/../.." && pwd)"
  arch="$(uname -m)"
  case "$arch" in
    arm64|aarch64)
      triple="aarch64-apple-darwin"
      ;;
    x86_64)
      triple="x86_64-apple-darwin"
      ;;
    *)
      echo "error: unsupported macOS architecture '$arch'" >&2
      return 1
      ;;
  esac

  find "$search_root" \
    -maxdepth 6 \
    -type f \
    -path "*/codex-*/vendor/${triple}/codex/codex" \
    | sort \
    | head -n 1
}

resolve_source_binary() {
  local explicit="$1"
  local candidate="$explicit"

  if [[ -z "$candidate" ]]; then
    candidate="$(command -v codex || true)"
  fi

  if [[ -z "$candidate" ]]; then
    echo "error: unable to locate codex CLI. Install Codex or pass a source binary path." >&2
    return 1
  fi

  if file "$candidate" | grep -q "Mach-O"; then
    echo "$candidate"
    return 0
  fi

  local native
  native="$(find_native_codex_from_wrapper "$candidate" || true)"
  if [[ -n "$native" ]]; then
    echo "$native"
    return 0
  fi

  echo "error: '$candidate' is not a native macOS Codex binary and no bundled native binary was found." >&2
  echo "hint: pass the native binary path explicitly (for npm installs this is under codex-*/vendor/*/codex/codex)." >&2
  return 1
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this installer is for macOS only" >&2
  exit 1
fi

SOURCE_PATH="$(resolve_source_binary "$SOURCE_INPUT")"

if [[ ! -f "$SOURCE_PATH" ]]; then
  echo "error: source binary does not exist: $SOURCE_PATH" >&2
  exit 1
fi
if [[ ! -x "$SOURCE_PATH" ]]; then
  echo "error: source binary is not executable: $SOURCE_PATH" >&2
  exit 1
fi

mkdir -p "$(dirname "$DEST_PATH")"
cp "$SOURCE_PATH" "$DEST_PATH"
chmod +x "$DEST_PATH"

echo "Installed macOS Codex runtime:"
echo "  source: $SOURCE_PATH"
echo "  target: $DEST_PATH"
"$DEST_PATH" --version || true
