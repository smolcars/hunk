#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${HUNK_WINDOWS_TARGET:-x86_64-pc-windows-msvc}"
PROFILE="release"
STAGE_RUNTIME=1
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"

usage() {
  cat <<'EOF'
Build hunk-desktop for Windows.

Usage:
  ./scripts/build_windows.sh [--target <triple>] [--debug] [--no-stage-runtime]

Options:
  --target <triple>   Override target triple (default: x86_64-pc-windows-msvc)
                      Must be a Windows target triple.
  --debug             Build debug profile instead of release
  --no-stage-runtime  Skip staging assets/codex-runtime/windows
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

if [[ "$TARGET_TRIPLE" != *windows* ]]; then
  echo "error: windows build script requires a windows target triple, got '$TARGET_TRIPLE'" >&2
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
if ! find "$TARGET_LIBDIR" -maxdepth 1 \( -name 'libcore-*' -o -name 'core-*' \) | grep -q .; then
  echo "error: target core libraries were not found in $TARGET_LIBDIR" >&2
  echo "hint: verify your active toolchain supports this target (for rustup: 'rustup target add $TARGET_TRIPLE')" >&2
  exit 1
fi

build_args=(build -p hunk-desktop --locked --target "$TARGET_TRIPLE")
if [[ "$PROFILE" == "release" ]]; then
  build_args+=(--release)
fi

echo "Building hunk-desktop for Windows target '$TARGET_TRIPLE' ($PROFILE profile)..."
(
  cd "$ROOT_DIR"
  cargo "${build_args[@]}"
)

BINARY_PATH="$TARGET_DIR/$TARGET_TRIPLE/$PROFILE/hunk_desktop.exe"
if [[ ! -f "$BINARY_PATH" ]]; then
  echo "error: expected Windows binary was not produced at $BINARY_PATH" >&2
  exit 1
fi
echo "Built binary: $BINARY_PATH"

if [[ "$STAGE_RUNTIME" == "1" ]]; then
  SOURCE_RUNTIME_DIR="$ROOT_DIR/assets/codex-runtime/windows"
  SOURCE_LAUNCHER="$SOURCE_RUNTIME_DIR/codex.cmd"
  SOURCE_BINARY="$SOURCE_RUNTIME_DIR/codex.exe"
  DEST_RUNTIME_DIR="$TARGET_DIR/$TARGET_TRIPLE/$PROFILE/codex-runtime/windows"

  if [[ ! -f "$SOURCE_LAUNCHER" || ! -f "$SOURCE_BINARY" ]]; then
    echo "warn: windows runtime assets not found at $SOURCE_RUNTIME_DIR; skipping runtime staging" >&2
  else
    rm -rf "$DEST_RUNTIME_DIR"
    mkdir -p "$DEST_RUNTIME_DIR"
    cp -R "$SOURCE_RUNTIME_DIR"/. "$DEST_RUNTIME_DIR"
    echo "Staged Windows runtime: $DEST_RUNTIME_DIR"
  fi
fi
