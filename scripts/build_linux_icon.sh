#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SRC_ICON="$ROOT_DIR/assets/icons/hunk_new.png"
OUT_ICON="$ROOT_DIR/assets/icons/hunk_linux_512.png"

if [[ ! -f "$SRC_ICON" ]]; then
  echo "Missing source icon: $SRC_ICON" >&2
  exit 1
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "Missing required tool: ffmpeg" >&2
  exit 1
fi

ffmpeg -loglevel error -y -i "$SRC_ICON" -vf scale=512:512 "$OUT_ICON"

echo "Generated: $OUT_ICON"
