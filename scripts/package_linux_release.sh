#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${HUNK_LINUX_TARGET:-x86_64-unknown-linux-gnu}"
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
VERSION_LABEL="${HUNK_RELEASE_VERSION:-$("$ROOT_DIR/scripts/resolve_hunk_version.sh")}"
DIST_DIR="$TARGET_DIR/dist"
PACKAGE_DIR="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64"
ARCHIVE_PATH="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64.tar.gz"
PACKAGER_OUT_DIR="$TARGET_DIR/packager/linux"
APPIMAGE_PATH="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64.AppImage"
BINARY_SOURCE_PATH="$TARGET_DIR/$TARGET_TRIPLE/release/hunk_desktop"
PACKAGED_BINARY_PATH="$PACKAGE_DIR/hunk-desktop"
PACKAGE_LIB_DIR="$PACKAGE_DIR/lib"
CODEX_SOURCE_PATH="$TARGET_DIR/$TARGET_TRIPLE/release/codex-runtime/linux/codex"
PACKAGED_CODEX_PATH="$PACKAGE_DIR/codex-runtime/linux/codex"

should_bundle_linux_library() {
  local library_name="$1"

  case "$library_name" in
    linux-vdso.so.*|linux-gate.so.*|ld-linux*.so.*|ld-musl-*.so.*)
      return 1
      ;;
    libc.so.*|libm.so.*|libpthread.so.*|librt.so.*|libdl.so.*|libutil.so.*|libresolv.so.*|libnsl.so.*|libanl.so.*|libBrokenLocale.so.*)
      return 1
      ;;
    *)
      return 0
      ;;
  esac
}

list_linux_runtime_dependencies() {
  local target_path="$1"
  local ldd_output

  ldd_output="$(ldd "$target_path")"
  if grep -Fq "not found" <<<"$ldd_output"; then
    echo "error: unresolved Linux runtime dependencies for $target_path" >&2
    echo "$ldd_output" >&2
    exit 1
  fi

  while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"

    if [[ "$line" == *"=>"* ]]; then
      line="${line#*=> }"
    elif [[ "$line" != /* ]]; then
      continue
    fi

    line="${line%% *}"
    if [[ "$line" == /* ]]; then
      printf '%s\n' "$line"
    fi
  done <<<"$ldd_output"
}

bundle_linux_runtime_dependencies() {
  local -A seen_paths=()
  local -A seen_names=()
  local -a queue=("$1")

  while [[ ${#queue[@]} -gt 0 ]]; do
    local current="${queue[0]}"
    queue=("${queue[@]:1}")

    while IFS= read -r dependency_path; do
      [[ -n "$dependency_path" ]] || continue

      local dependency_name
      dependency_name="$(basename "$dependency_path")"
      if ! should_bundle_linux_library "$dependency_name"; then
        continue
      fi

      if [[ -n "${seen_paths[$dependency_path]:-}" ]]; then
        continue
      fi

      if [[ -n "${seen_names[$dependency_name]:-}" && "${seen_names[$dependency_name]}" != "$dependency_path" ]]; then
        echo "error: conflicting Linux dependency paths for $dependency_name:" >&2
        echo "  ${seen_names[$dependency_name]}" >&2
        echo "  $dependency_path" >&2
        exit 1
      fi

      seen_paths["$dependency_path"]=1
      seen_names["$dependency_name"]="$dependency_path"

      echo "Bundling Linux dependency $dependency_name from $dependency_path" >&2
      cp -L "$dependency_path" "$PACKAGE_LIB_DIR/$dependency_name"
      chmod 755 "$PACKAGE_LIB_DIR/$dependency_name"
      queue+=("$dependency_path")
    done < <(list_linux_runtime_dependencies "$current")
  done
}

patch_linux_runtime_paths() {
  local binary_path="$1"
  local libs_dir="$2"
  local binary_rpath="$3"

  patchelf --set-rpath "$binary_rpath" "$binary_path"

  if [[ -d "$libs_dir" ]]; then
    while IFS= read -r -d '' library_path; do
      patchelf --set-rpath '$ORIGIN' "$library_path"
    done < <(find "$libs_dir" -maxdepth 1 -type f -name '*.so*' -print0)
  fi
}

validate_linux_runtime_bundle() {
  local binary_path="$1"
  local libs_dir="$2"
  local ldd_output

  ldd_output="$(env LD_LIBRARY_PATH="$libs_dir" ldd "$binary_path")"
  if grep -Fq "not found" <<<"$ldd_output"; then
    echo "error: bundled Linux runtime dependencies are incomplete for $binary_path" >&2
    echo "$ldd_output" >&2
    exit 1
  fi
}

echo "Downloading bundled Codex runtime for Linux..." >&2
"$ROOT_DIR/scripts/download_codex_runtime_unix.sh" linux >/dev/null
echo "Validating bundled Codex runtime for Linux..." >&2
"$ROOT_DIR/scripts/validate_codex_runtime_bundle.sh" --strict --platform linux >/dev/null
echo "Building Linux release binary..." >&2
(
  cd "$ROOT_DIR"
  export CARGO_TARGET_DIR="$TARGET_DIR"
  "$ROOT_DIR/scripts/build_linux.sh" --target "$TARGET_TRIPLE"
)

if ! command -v patchelf >/dev/null 2>&1; then
  echo "error: patchelf is required to bundle Linux shared libraries" >&2
  exit 1
fi

rm -rf "$PACKAGE_DIR" "$ARCHIVE_PATH" "$APPIMAGE_PATH"
mkdir -p "$PACKAGE_DIR/codex-runtime/linux"
mkdir -p "$PACKAGE_LIB_DIR"

cp "$BINARY_SOURCE_PATH" "$PACKAGED_BINARY_PATH"
cp "$CODEX_SOURCE_PATH" "$PACKAGED_CODEX_PATH"
chmod +x "$PACKAGED_BINARY_PATH" "$PACKAGED_CODEX_PATH"

echo "Bundling Linux shared libraries into tarball fallback..." >&2
bundle_linux_runtime_dependencies "$BINARY_SOURCE_PATH"
patch_linux_runtime_paths "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR" '$ORIGIN/lib'
validate_linux_runtime_bundle "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR"

mkdir -p "$DIST_DIR"
tar -C "$DIST_DIR" -czf "$ARCHIVE_PATH" "$(basename "$PACKAGE_DIR")"

echo "Building Linux AppImage package..." >&2
(
  cd "$ROOT_DIR"
  export CARGO_TARGET_DIR="$TARGET_DIR"
  cargo packager -p hunk-desktop --release -f appimage --target "$TARGET_TRIPLE" --out-dir "$PACKAGER_OUT_DIR"
)

BUNDLE_APPIMAGE_PATH="$(find "$PACKAGER_OUT_DIR" -maxdepth 1 -type f -name '*.AppImage' -printf '%T@ %p\n' | sort -nr | head -n 1 | cut -d' ' -f2-)"
if [[ -z "$BUNDLE_APPIMAGE_PATH" ]]; then
  echo "error: expected cargo-packager to produce an AppImage under $PACKAGER_OUT_DIR" >&2
  exit 1
fi

cp "$BUNDLE_APPIMAGE_PATH" "$APPIMAGE_PATH"
chmod +x "$APPIMAGE_PATH"

echo "Created Linux AppImage artifact at $APPIMAGE_PATH" >&2
echo "Created Linux release artifact at $ARCHIVE_PATH" >&2

printf '%s\n' "$APPIMAGE_PATH"
