#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SRC_ICON="$ROOT_DIR/assets/icons/hunk_new.png"
OUT_ICON="$ROOT_DIR/assets/icons/Hunk.ico"

if [[ ! -f "$SRC_ICON" ]]; then
    echo "Missing source icon: $SRC_ICON" >&2
    exit 1
fi

magick "$SRC_ICON" -define icon:auto-resize=16,32,48,64,128,256 "$OUT_ICON"

echo "Generated: $OUT_ICON"
