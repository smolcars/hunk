#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_PATH="${1:-$ROOT_DIR/assets/codex-runtime/macos/codex}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this staging script is for macOS only" >&2
  exit 1
fi

if [[ ! -f "$SOURCE_PATH" ]]; then
  echo "error: source runtime not found: $SOURCE_PATH" >&2
  echo "hint: run ./scripts/install_codex_runtime_macos.sh first" >&2
  exit 1
fi
if [[ ! -x "$SOURCE_PATH" ]]; then
  echo "error: source runtime is not executable: $SOURCE_PATH" >&2
  exit 1
fi

for profile in debug release; do
  destination="$ROOT_DIR/target/$profile/codex-runtime/macos/codex"
  mkdir -p "$(dirname "$destination")"
  cp "$SOURCE_PATH" "$destination"
  chmod +x "$destination"
  echo "staged: $destination"
done

echo "Codex runtime staged for local macOS binaries."
