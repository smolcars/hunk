#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${HUNK_LINUX_TARGET:-x86_64-unknown-linux-gnu}"
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
VERSION_LABEL="${HUNK_RELEASE_VERSION:-$("$ROOT_DIR/scripts/resolve_hunk_version.sh")}"
DIST_DIR="$TARGET_DIR/dist"
PACKAGE_DIR="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64"
ARCHIVE_PATH="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64.tar.gz"
APPIMAGE_PATH="$DIST_DIR/Hunk-$VERSION_LABEL-linux-x86_64.AppImage"
BINARY_SOURCE_PATH="$TARGET_DIR/$TARGET_TRIPLE/release/hunk_desktop"
REAL_BINARY_NAME="hunk_desktop_bin"
LAUNCHER_SOURCE_PATH="$ROOT_DIR/scripts/linux_gui_binary_launcher.sh"
PACKAGED_BINARY_PATH="$PACKAGE_DIR/$REAL_BINARY_NAME"
PACKAGED_LAUNCHER_PATH="$PACKAGE_DIR/hunk-desktop"
PACKAGE_LIB_DIR="$PACKAGE_DIR/lib"
CODEX_SOURCE_PATH="$TARGET_DIR/$TARGET_TRIPLE/release/codex-runtime/linux/codex"
PACKAGED_CODEX_PATH="$PACKAGE_DIR/codex-runtime/linux/codex"
APPDIR_PATH="$TARGET_DIR/appimage/Hunk.AppDir"
APPIMAGE_TOOL_CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/hunk-appimage-tools"
APPIMAGE_APPRUN_PATH="$APPIMAGE_TOOL_CACHE_DIR/AppRun-x86_64"
APPIMAGE_PLUGIN_PATH="$APPIMAGE_TOOL_CACHE_DIR/linuxdeploy-plugin-appimage.AppImage"
APPIMAGE_TOOL_EXTRACT_DIR="$TARGET_DIR/appimage/tooling"
APPIMAGE_TOOL_PATH="$APPIMAGE_TOOL_EXTRACT_DIR/squashfs-root/usr/bin/appimagetool"
APP_DESKTOP_ENTRY_PATH="$APPDIR_PATH/usr/share/applications/hunk_desktop.desktop"
APP_ICON_PATH="$APPDIR_PATH/usr/share/icons/hicolor/1024x1024/apps/hunk_desktop.png"
APPDIR_REAL_BINARY_PATH="$APPDIR_PATH/usr/bin/$REAL_BINARY_NAME"
APPDIR_LAUNCHER_PATH="$APPDIR_PATH/usr/bin/hunk_desktop"

download_cached_appimage_tool() {
  local url="$1"
  local destination="$2"
  local tmp_path

  mkdir -p "$(dirname "$destination")"
  tmp_path="$(mktemp "${destination}.XXXXXX")"
  curl --fail --location --retry 3 --retry-delay 1 --output "$tmp_path" "$url"
  chmod 755 "$tmp_path"
  mv "$tmp_path" "$destination"
}

ensure_appimage_tooling() {
  if [[ ! -f "$APPIMAGE_APPRUN_PATH" ]]; then
    echo "Downloading AppRun helper..." >&2
    download_cached_appimage_tool \
      "https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64" \
      "$APPIMAGE_APPRUN_PATH"
  fi

  if [[ ! -f "$APPIMAGE_PLUGIN_PATH" ]]; then
    echo "Downloading appimagetool bundle..." >&2
    download_cached_appimage_tool \
      "https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage" \
      "$APPIMAGE_PLUGIN_PATH"
  fi

  rm -rf "$APPIMAGE_TOOL_EXTRACT_DIR"
  mkdir -p "$APPIMAGE_TOOL_EXTRACT_DIR"
  (
    cd "$APPIMAGE_TOOL_EXTRACT_DIR"
    "$APPIMAGE_PLUGIN_PATH" --appimage-extract >/dev/null
  )

  if [[ ! -x "$APPIMAGE_TOOL_PATH" ]]; then
    echo "error: expected appimagetool at $APPIMAGE_TOOL_PATH" >&2
    exit 1
  fi
}

create_linux_appdir() {
  rm -rf "$APPDIR_PATH"
  mkdir -p "$APPDIR_PATH/usr/bin"
  mkdir -p "$APPDIR_PATH/usr/lib"
  mkdir -p "$APPDIR_PATH/usr/share/applications"
  mkdir -p "$APPDIR_PATH/usr/share/icons/hicolor/1024x1024/apps"
  mkdir -p "$APPDIR_PATH/usr/lib/hunk_desktop/codex-runtime/linux"

  cp "$APPIMAGE_APPRUN_PATH" "$APPDIR_PATH/AppRun"
  cp "$PACKAGED_BINARY_PATH" "$APPDIR_REAL_BINARY_PATH"
  cp "$PACKAGED_LAUNCHER_PATH" "$APPDIR_LAUNCHER_PATH"
  cp -R "$PACKAGE_LIB_DIR/." "$APPDIR_PATH/usr/lib/"
  cp "$PACKAGED_CODEX_PATH" "$APPDIR_PATH/usr/lib/hunk_desktop/codex-runtime/linux/codex"
  chmod +x "$APPDIR_PATH/AppRun" "$APPDIR_REAL_BINARY_PATH" "$APPDIR_LAUNCHER_PATH" \
    "$APPDIR_PATH/usr/lib/hunk_desktop/codex-runtime/linux/codex"

  patch_linux_runtime_paths "$APPDIR_REAL_BINARY_PATH" "$APPDIR_PATH/usr/lib" '$ORIGIN/../lib'
  validate_linux_runtime_bundle "$APPDIR_REAL_BINARY_PATH" "$APPDIR_PATH/usr/lib"
  "$ROOT_DIR/scripts/validate_release_bundle_layout.sh" linux-appdir "$APPDIR_PATH"

  cat >"$APP_DESKTOP_ENTRY_PATH" <<'EOF'
[Desktop Entry]
Categories=Development;
Comment=Very fast git diff viewer and codex orchestrator.
Exec=hunk_desktop
Icon=hunk_desktop
Name=Hunk
StartupNotify=true
StartupWMClass=hunk_desktop
Terminal=false
Type=Application
EOF

  cp "$ROOT_DIR/assets/icons/hunk_new.png" "$APP_ICON_PATH"
  cp "$ROOT_DIR/assets/icons/hunk_new.png" "$APPDIR_PATH/.DirIcon"
  cp "$ROOT_DIR/assets/icons/hunk_new.png" "$APPDIR_PATH/hunk_desktop.png"
  ln -sf "usr/share/applications/hunk_desktop.desktop" "$APPDIR_PATH/hunk_desktop.desktop"
}

build_linux_appimage() {
  ensure_appimage_tooling
  create_linux_appdir

  ARCH=x86_64 "$APPIMAGE_TOOL_PATH" "$APPDIR_PATH" "$APPIMAGE_PATH"
}

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
cp "$LAUNCHER_SOURCE_PATH" "$PACKAGED_LAUNCHER_PATH"
cp "$CODEX_SOURCE_PATH" "$PACKAGED_CODEX_PATH"
chmod +x "$PACKAGED_BINARY_PATH" "$PACKAGED_LAUNCHER_PATH" "$PACKAGED_CODEX_PATH"

echo "Bundling Linux shared libraries into tarball fallback..." >&2
bundle_linux_runtime_dependencies "$BINARY_SOURCE_PATH"
patch_linux_runtime_paths "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR" '$ORIGIN/lib'
validate_linux_runtime_bundle "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR"
"$ROOT_DIR/scripts/validate_release_bundle_layout.sh" linux-package "$PACKAGE_DIR"

mkdir -p "$DIST_DIR"
tar -C "$DIST_DIR" -czf "$ARCHIVE_PATH" "$(basename "$PACKAGE_DIR")"

echo "Building Linux AppImage package..." >&2
build_linux_appimage
chmod +x "$APPIMAGE_PATH"

echo "Created Linux AppImage artifact at $APPIMAGE_PATH" >&2
echo "Created Linux release artifact at $ARCHIVE_PATH" >&2

printf '%s\n' "$APPIMAGE_PATH"
