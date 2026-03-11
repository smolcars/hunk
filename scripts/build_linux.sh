#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${HUNK_LINUX_TARGET:-x86_64-unknown-linux-gnu}"
PROFILE="release"
STAGE_RUNTIME=1
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"

usage() {
  cat <<'EOF'
Build hunk-desktop for Linux.

Usage:
  ./scripts/build_linux.sh [--target <triple>] [--debug] [--no-stage-runtime]

Options:
  --target <triple>   Override target triple (default: x86_64-unknown-linux-gnu)
                      Must be a Linux target triple.
  --debug             Build debug profile instead of release
  --no-stage-runtime  Skip staging assets/codex-runtime/linux/codex
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET_TRIPLE="${2:-}"
      if [[ -z "$TARGET_TRIPLE" ]]; then
        echo "error: --target requires a value" >&2
        exit 1
      fi
      shift 2
      ;;
    --debug)
      PROFILE="debug"
      shift
      ;;
    --no-stage-runtime)
      STAGE_RUNTIME=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "$TARGET_TRIPLE" != *linux* ]]; then
  echo "error: linux build script requires a linux target triple, got '$TARGET_TRIPLE'" >&2
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  if ! rustup target list --installed | grep -Fx "$TARGET_TRIPLE" >/dev/null 2>&1; then
    echo "error: rust target '$TARGET_TRIPLE' is not installed" >&2
    echo "hint: run 'rustup target add $TARGET_TRIPLE'" >&2
    exit 1
  fi
fi

TARGET_LIBDIR="$(rustc --target "$TARGET_TRIPLE" --print target-libdir 2>/dev/null || true)"
if [[ -z "$TARGET_LIBDIR" || ! -d "$TARGET_LIBDIR" ]]; then
  echo "error: rustc reported an invalid target library directory for '$TARGET_TRIPLE': $TARGET_LIBDIR" >&2
  echo "hint: verify your active toolchain supports this target (for rustup: 'rustup target add $TARGET_TRIPLE')" >&2
  exit 1
fi
if ! compgen -G "$TARGET_LIBDIR/libcore-*" >/dev/null && ! compgen -G "$TARGET_LIBDIR/core-*" >/dev/null; then
  echo "error: target core libraries were not found in $TARGET_LIBDIR" >&2
  echo "hint: verify your active toolchain supports this target (for rustup: 'rustup target add $TARGET_TRIPLE')" >&2
  exit 1
fi

build_args=(build -p hunk-desktop --locked --target "$TARGET_TRIPLE")
if [[ "$PROFILE" == "release" ]]; then
  build_args+=(--release)
fi

echo "Building hunk-desktop for Linux target '$TARGET_TRIPLE' ($PROFILE profile)..."
(
  cd "$ROOT_DIR"
  cargo "${build_args[@]}"
)

BINARY_PATH="$TARGET_DIR/$TARGET_TRIPLE/$PROFILE/hunk_desktop"
if [[ ! -f "$BINARY_PATH" ]]; then
  echo "error: expected Linux binary was not produced at $BINARY_PATH" >&2
  exit 1
fi
echo "Built binary: $BINARY_PATH"

if [[ "$STAGE_RUNTIME" == "1" ]]; then
  SOURCE_RUNTIME="$ROOT_DIR/assets/codex-runtime/linux/codex"
  DEST_RUNTIME="$TARGET_DIR/$TARGET_TRIPLE/$PROFILE/codex-runtime/linux/codex"

  if [[ ! -f "$SOURCE_RUNTIME" ]]; then
    echo "warn: linux runtime asset not found at $SOURCE_RUNTIME; skipping runtime staging" >&2
  else
    mkdir -p "$(dirname "$DEST_RUNTIME")"
    cp "$SOURCE_RUNTIME" "$DEST_RUNTIME"
    chmod +x "$DEST_RUNTIME"
    echo "Staged Linux runtime: $DEST_RUNTIME"
  fi
fi
