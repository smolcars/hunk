#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEX_TAG="${HUNK_CODEX_TAG:-$("$ROOT_DIR/scripts/resolve_codex_release_tag.sh")}"
PLATFORM="${1:-}"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hunk-codex-runtime.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

usage() {
  cat <<'EOF'
Download a bundled Codex runtime for a Unix-like target.

Usage:
  ./scripts/download_codex_runtime_unix.sh <macos|linux>
EOF
}

case "$PLATFORM" in
  macos)
    asset_name="codex-aarch64-apple-darwin.tar.gz"
    extracted_name="codex-aarch64-apple-darwin"
    destination="$ROOT_DIR/assets/codex-runtime/macos/codex"
    ;;
  linux)
    asset_name="codex-x86_64-unknown-linux-musl.tar.gz"
    extracted_name="codex-x86_64-unknown-linux-musl"
    destination="$ROOT_DIR/assets/codex-runtime/linux/codex"
    ;;
  *)
    usage >&2
    exit 1
    ;;
esac

archive_path="$TMP_DIR/$asset_name"
download_url="https://github.com/openai/codex/releases/download/$CODEX_TAG/$asset_name"

echo "Downloading Codex runtime from $download_url" >&2
curl --fail --location --silent --show-error "$download_url" --output "$archive_path"

tar -xzf "$archive_path" -C "$TMP_DIR"

source_binary="$TMP_DIR/$extracted_name"
if [[ ! -f "$source_binary" ]]; then
  echo "error: expected extracted Codex binary at $source_binary" >&2
  exit 1
fi

mkdir -p "$(dirname "$destination")"
staged_destination="$(mktemp "$destination.XXXXXX")"
cp "$source_binary" "$staged_destination"
chmod +x "$staged_destination"
mv -f "$staged_destination" "$destination"

echo "Prepared bundled Codex runtime at $destination" >&2

printf '%s\n' "$destination"
