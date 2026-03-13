#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SRC_ICON="$ROOT_DIR/assets/icons/hunk_new.png"
OUT_ICON="$ROOT_DIR/assets/icons/Hunk.icns"

if [[ ! -f "$SRC_ICON" ]]; then
    echo "Missing source icon: $SRC_ICON" >&2
    exit 1
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

declare -a ICON_TYPES=(icp4 icp5 icp6 ic07 ic08 ic09 ic10)
declare -a ICON_SIZES=(16 32 64 128 256 512 1024)

for i in "${!ICON_TYPES[@]}"; do
    icon_type="${ICON_TYPES[$i]}"
    size="${ICON_SIZES[$i]}"
    sips -z "$size" "$size" "$SRC_ICON" --out "$TMP_DIR/$icon_type.png" >/dev/null

done

python3 - "$TMP_DIR" "$OUT_ICON" <<'PY'
#!/usr/bin/env python3
import struct
import sys
from pathlib import Path

tmp_dir = Path(sys.argv[1])
out_path = Path(sys.argv[2])

icon_types = ["icp4", "icp5", "icp6", "ic07", "ic08", "ic09", "ic10"]
chunks = []
for icon_type in icon_types:
    data = (tmp_dir / f"{icon_type}.png").read_bytes()
    chunks.append(icon_type.encode("ascii") + struct.pack(">I", len(data) + 8) + data)

payload = b"".join(chunks)
out_path.write_bytes(b"icns" + struct.pack(">I", len(payload) + 8) + payload)
PY

echo "Generated: $OUT_ICON"
