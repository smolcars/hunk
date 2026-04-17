#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -n "${HUNK_CODEX_TAG:-}" ]]; then
  printf '%s\n' "$HUNK_CODEX_TAG"
  exit 0
fi

CODEX_VERSION="$(
  python3 - "$ROOT_DIR/Cargo.lock" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
match = re.search(
    r'\[\[package\]\]\nname = "codex-app-server"\nversion = "([^"]+)"',
    text,
)
if match:
    print(match.group(1))
PY
)"

if [[ -z "$CODEX_VERSION" ]]; then
  echo "error: failed to resolve Codex version from Cargo.lock" >&2
  exit 1
fi

CODEX_TAG="rust-v$CODEX_VERSION"

printf '%s\n' "$CODEX_TAG"
