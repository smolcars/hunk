#!/usr/bin/env bash

if [[ -n "${HUNK_LINUX_RELEASE_COMMON_SOURCED:-}" ]]; then
  return 0
fi
HUNK_LINUX_RELEASE_COMMON_SOURCED=1

ROOT_DIR="${ROOT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
TARGET_TRIPLE="${HUNK_LINUX_TARGET:-x86_64-unknown-linux-gnu}"
TARGET_DIR="${CARGO_TARGET_DIR:-$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")}"
VERSION_LABEL="${HUNK_RELEASE_VERSION:-$("$ROOT_DIR/scripts/resolve_hunk_version.sh")}"
PRODUCT_NAME="${HUNK_LINUX_PRODUCT_NAME:-Hunk}"
PACKAGE_NAME="${HUNK_LINUX_PACKAGE_NAME:-hunk-desktop}"
PACKAGE_VENDOR="${HUNK_LINUX_PACKAGE_VENDOR:-Nitesh Balusu}"
PACKAGE_MAINTAINER="${HUNK_LINUX_PACKAGE_MAINTAINER:-Nitesh Balusu <hunk@example.com>}"
PACKAGE_LICENSE="${HUNK_LINUX_PACKAGE_LICENSE:-LicenseRef-Unknown}"
PACKAGE_HOMEPAGE="${HUNK_LINUX_PACKAGE_HOMEPAGE:-https://github.com/BlixtWallet/hunk}"
PACKAGE_SUMMARY="${HUNK_LINUX_PACKAGE_SUMMARY:-Very fast git diff viewer and codex orchestrator.}"
PACKAGE_DESCRIPTION="${HUNK_LINUX_PACKAGE_DESCRIPTION:-A super fast, simple Git diff viewer and Codex orchestrator built with GPUI.}"
PACKAGE_SECTION="${HUNK_LINUX_PACKAGE_SECTION:-utils}"
PACKAGE_PRIORITY="${HUNK_LINUX_PACKAGE_PRIORITY:-optional}"
PACKAGE_RELEASE="${HUNK_LINUX_PACKAGE_RELEASE:-1}"
WORK_DIR="$TARGET_DIR/linux-packaging"
DIST_DIR="$TARGET_DIR/dist"
ARCH_LABEL=""
PACKAGE_DIR=""
ARCHIVE_PATH=""
APPIMAGE_PATH=""
APPDIR_PATH=""
APP_DESKTOP_ENTRY_PATH=""
APP_ICON_PATH=""
APPDIR_REAL_BINARY_PATH=""
APPDIR_LAUNCHER_PATH=""
SYSTEM_INSTALL_ROOT=""
SYSTEM_BIN_DIR=""
SYSTEM_LIB_DIR=""
SYSTEM_PRIVATE_LIB_DIR=""
SYSTEM_REAL_BINARY_PATH=""
SYSTEM_LAUNCHER_PATH=""
SYSTEM_RUNTIME_PATH=""
SYSTEM_DESKTOP_ENTRY_PATH=""
SYSTEM_ICON_DIR=""
SYSTEM_ICON_PATH=""
SYSTEM_ICON_ALIAS_PATH=""
SYSTEM_PIXMAP_DIR=""
SYSTEM_PIXMAP_PATH=""
SYSTEM_WRAPPER_PATH=""
SYSTEM_WRAPPER_ALIAS_PATH=""
DEB_BUILD_ROOT=""
DEB_ARCH=""
DEB_VERSION=""
DEB_PATH=""
RPM_TOPDIR=""
RPM_ARCH=""
RPM_VERSION=""
RPM_PATH=""
BINARY_SOURCE_PATH=""
REAL_BINARY_NAME="hunk_desktop_bin"
LAUNCHER_SOURCE_PATH="$ROOT_DIR/scripts/linux_gui_binary_launcher.sh"
LINUX_ICON_SOURCE_PATH="$ROOT_DIR/assets/icons/hunk_linux_512.png"
PACKAGED_BINARY_PATH=""
PACKAGED_LAUNCHER_PATH=""
PACKAGE_LIB_DIR=""
CODEX_SOURCE_PATH=""
PACKAGED_CODEX_PATH=""
APPIMAGE_TOOL_CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/hunk-appimage-tools"
APPIMAGE_APPRUN_PATH="$APPIMAGE_TOOL_CACHE_DIR/AppRun-x86_64"
APPIMAGE_PLUGIN_PATH="$APPIMAGE_TOOL_CACHE_DIR/linuxdeploy-plugin-appimage.AppImage"
APPIMAGE_TOOL_EXTRACT_DIR=""
APPIMAGE_TOOL_PATH=""
EXTRA_LINUX_RUNTIME_LIBS=(
  "libX11.so.6"
  "libEGL.so.1"
  "libvulkan.so.1"
  "libwayland-client.so.0"
  "libwayland-cursor.so.0"
  "libwayland-egl.so.1"
)

linux_target_arch() {
  printf '%s\n' "${TARGET_TRIPLE%%-*}"
}

linux_dist_arch_label() {
  case "$(linux_target_arch)" in
    x86_64)
      printf '%s\n' "x86_64"
      ;;
    aarch64)
      printf '%s\n' "arm64"
      ;;
    *)
      printf '%s\n' "$(linux_target_arch)"
      ;;
  esac
}

linux_deb_arch() {
  case "$(linux_target_arch)" in
    x86_64)
      printf '%s\n' "amd64"
      ;;
    aarch64)
      printf '%s\n' "arm64"
      ;;
    armv7*)
      printf '%s\n' "armhf"
      ;;
    *)
      echo "error: unsupported Debian architecture for target '$TARGET_TRIPLE'" >&2
      exit 1
      ;;
  esac
}

linux_rpm_arch() {
  case "$(linux_target_arch)" in
    x86_64)
      printf '%s\n' "x86_64"
      ;;
    aarch64)
      printf '%s\n' "aarch64"
      ;;
    *)
      printf '%s\n' "$(linux_target_arch)"
      ;;
  esac
}

linux_deb_version() {
  printf '%s-%s\n' "$VERSION_LABEL" "$PACKAGE_RELEASE"
}

linux_rpm_version() {
  local version="$VERSION_LABEL"
  if [[ "$version" == *-* ]]; then
    local base="${version%%-*}"
    local suffix="${version#*-}"
    suffix="${suffix//-/_}"
    printf '%s~%s\n' "$base" "$suffix"
  else
    printf '%s\n' "$version"
  fi
}

linux_rpm_changelog_date() {
  LC_ALL=C date -u +"%a %b %d %Y"
}

init_linux_release_paths() {
  ARCH_LABEL="$(linux_dist_arch_label)"
  PACKAGE_DIR="$WORK_DIR/tarball/${PRODUCT_NAME}-${VERSION_LABEL}-linux-$ARCH_LABEL"
  ARCHIVE_PATH="$DIST_DIR/${PRODUCT_NAME}-${VERSION_LABEL}-linux-$ARCH_LABEL.tar.gz"
  APPIMAGE_PATH="$DIST_DIR/${PRODUCT_NAME}-${VERSION_LABEL}-linux-$ARCH_LABEL.AppImage"
  APPDIR_PATH="$WORK_DIR/appimage/${PRODUCT_NAME}.AppDir"
  APP_DESKTOP_ENTRY_PATH="$APPDIR_PATH/usr/share/applications/hunk_desktop.desktop"
  APP_ICON_PATH="$APPDIR_PATH/usr/share/icons/hicolor/512x512/apps/hunk_desktop.png"
  APPDIR_REAL_BINARY_PATH="$APPDIR_PATH/usr/bin/$REAL_BINARY_NAME"
  APPDIR_LAUNCHER_PATH="$APPDIR_PATH/usr/bin/hunk_desktop"
  SYSTEM_INSTALL_ROOT="$WORK_DIR/system-root"
  SYSTEM_BIN_DIR="$SYSTEM_INSTALL_ROOT/usr/bin"
  SYSTEM_LIB_DIR="$SYSTEM_INSTALL_ROOT/usr/lib/$PACKAGE_NAME"
  SYSTEM_PRIVATE_LIB_DIR="$SYSTEM_LIB_DIR/lib"
  SYSTEM_REAL_BINARY_PATH="$SYSTEM_LIB_DIR/$REAL_BINARY_NAME"
  SYSTEM_LAUNCHER_PATH="$SYSTEM_LIB_DIR/$PACKAGE_NAME"
  SYSTEM_RUNTIME_PATH="$SYSTEM_LIB_DIR/codex-runtime/linux/codex"
  SYSTEM_DESKTOP_ENTRY_PATH="$SYSTEM_INSTALL_ROOT/usr/share/applications/$PACKAGE_NAME.desktop"
  SYSTEM_ICON_DIR="$SYSTEM_INSTALL_ROOT/usr/share/icons/hicolor/512x512/apps"
  SYSTEM_ICON_PATH="$SYSTEM_ICON_DIR/$PACKAGE_NAME.png"
  SYSTEM_ICON_ALIAS_PATH="$SYSTEM_ICON_DIR/${PACKAGE_NAME//-/_}.png"
  SYSTEM_PIXMAP_DIR="$SYSTEM_INSTALL_ROOT/usr/share/pixmaps"
  SYSTEM_PIXMAP_PATH="$SYSTEM_PIXMAP_DIR/$PACKAGE_NAME.png"
  SYSTEM_WRAPPER_PATH="$SYSTEM_BIN_DIR/$PACKAGE_NAME"
  SYSTEM_WRAPPER_ALIAS_PATH="$SYSTEM_BIN_DIR/${PACKAGE_NAME//-/_}"
  DEB_BUILD_ROOT="$WORK_DIR/deb-root"
  DEB_ARCH="$(linux_deb_arch)"
  DEB_VERSION="$(linux_deb_version)"
  DEB_PATH="$DIST_DIR/${PACKAGE_NAME}_${DEB_VERSION}_${DEB_ARCH}.deb"
  RPM_TOPDIR="$WORK_DIR/rpmbuild"
  RPM_ARCH="$(linux_rpm_arch)"
  RPM_VERSION="$(linux_rpm_version)"
  RPM_PATH="$DIST_DIR/${PACKAGE_NAME}-${RPM_VERSION}-${PACKAGE_RELEASE}.${RPM_ARCH}.rpm"
  BINARY_SOURCE_PATH="$TARGET_DIR/$TARGET_TRIPLE/release/hunk_desktop"
  PACKAGED_BINARY_PATH="$PACKAGE_DIR/$REAL_BINARY_NAME"
  PACKAGED_LAUNCHER_PATH="$PACKAGE_DIR/$PACKAGE_NAME"
  PACKAGE_LIB_DIR="$PACKAGE_DIR/lib"
  CODEX_SOURCE_PATH="$TARGET_DIR/$TARGET_TRIPLE/release/codex-runtime/linux/codex"
  PACKAGED_CODEX_PATH="$PACKAGE_DIR/codex-runtime/linux/codex"
  APPIMAGE_TOOL_EXTRACT_DIR="$WORK_DIR/appimage/tooling"
  APPIMAGE_TOOL_PATH="$APPIMAGE_TOOL_EXTRACT_DIR/squashfs-root/usr/bin/appimagetool"
}

require_linux_tool() {
  local tool_name="$1"
  if ! command -v "$tool_name" >/dev/null 2>&1; then
    echo "error: required Linux packaging tool '$tool_name' is not installed" >&2
    exit 1
  fi
}

find_linux_library_path_by_name() {
  local library_name="$1"
  local search_path_var

  for search_path_var in "${LD_LIBRARY_PATH:-}" "${LIBRARY_PATH:-}"; do
    [[ -n "$search_path_var" ]] || continue

    local search_dir
    while IFS= read -r search_dir; do
      [[ -n "$search_dir" && -d "$search_dir" ]] || continue

      local candidate_path="$search_dir/$library_name"
      if [[ -f "$candidate_path" ]]; then
        printf '%s\n' "$candidate_path"
        return 0
      fi
    done < <(tr ':' '\n' <<<"$search_path_var")
  done

  if command -v ldconfig >/dev/null 2>&1; then
    local ldconfig_path
    ldconfig_path="$(
      ldconfig -p 2>/dev/null \
        | awk -v name="$library_name" '$1 == name { print $NF; exit }'
    )"
    if [[ -n "$ldconfig_path" && -f "$ldconfig_path" ]]; then
      printf '%s\n' "$ldconfig_path"
      return 0
    fi
  fi

  local search_root
  for search_root in /lib /usr/lib /usr/lib64 /usr/local/lib /nix/store; do
    [[ -d "$search_root" ]] || continue
    local match
    match="$(find "$search_root" -path "*/$library_name" -type f 2>/dev/null | head -n 1)"
    if [[ -n "$match" ]]; then
      printf '%s\n' "$match"
      return 0
    fi
  done

  return 1
}

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
  local root_binary="$1"
  local destination_dir="$2"
  local -A seen_paths=()
  local -A seen_names=()
  local -a queue=("$root_binary")

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
      cp -L "$dependency_path" "$destination_dir/$dependency_name"
      chmod 755 "$destination_dir/$dependency_name"
      queue+=("$dependency_path")
    done < <(list_linux_runtime_dependencies "$current")
  done
}

bundle_linux_extra_runtime_libraries() {
  local destination_dir="$1"
  local library_name

  for library_name in "${EXTRA_LINUX_RUNTIME_LIBS[@]}"; do
    if [[ -f "$destination_dir/$library_name" ]]; then
      continue
    fi

    local source_path
    source_path="$(find_linux_library_path_by_name "$library_name" || true)"
    if [[ -z "$source_path" ]]; then
      echo "warning: optional Linux runtime library '$library_name' was not found on this host" >&2
      continue
    fi

    echo "Bundling extra Linux runtime library $library_name from $source_path" >&2
    cp -L "$source_path" "$destination_dir/$library_name"
    chmod 755 "$destination_dir/$library_name"
    bundle_linux_runtime_dependencies "$source_path" "$destination_dir"
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

prepare_linux_release_build_inputs() {
  require_linux_tool patchelf

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
}

prepare_linux_release_bundle() {
  prepare_linux_release_build_inputs

  rm -rf "$PACKAGE_DIR"
  mkdir -p "$PACKAGE_DIR/codex-runtime/linux" "$PACKAGE_LIB_DIR" "$DIST_DIR"

  cp "$BINARY_SOURCE_PATH" "$PACKAGED_BINARY_PATH"
  cp "$LAUNCHER_SOURCE_PATH" "$PACKAGED_LAUNCHER_PATH"
  cp "$CODEX_SOURCE_PATH" "$PACKAGED_CODEX_PATH"
  chmod +x "$PACKAGED_BINARY_PATH" "$PACKAGED_LAUNCHER_PATH" "$PACKAGED_CODEX_PATH"

  echo "Bundling Linux shared libraries into release bundle..." >&2
  bundle_linux_runtime_dependencies "$BINARY_SOURCE_PATH" "$PACKAGE_LIB_DIR"
  bundle_linux_extra_runtime_libraries "$PACKAGE_LIB_DIR"
  patch_linux_runtime_paths "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR" '$ORIGIN/lib'
  validate_linux_runtime_bundle "$PACKAGED_BINARY_PATH" "$PACKAGE_LIB_DIR"
  "$ROOT_DIR/scripts/validate_release_bundle_layout.sh" linux-package "$PACKAGE_DIR"
}

create_linux_appdir() {
  rm -rf "$APPDIR_PATH"
  mkdir -p "$APPDIR_PATH/usr/bin"
  mkdir -p "$APPDIR_PATH/usr/lib"
  mkdir -p "$APPDIR_PATH/usr/share/applications"
  mkdir -p "$APPDIR_PATH/usr/share/icons/hicolor/512x512/apps"
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

  cp "$LINUX_ICON_SOURCE_PATH" "$APP_ICON_PATH"
  cp "$LINUX_ICON_SOURCE_PATH" "$APPDIR_PATH/.DirIcon"
  cp "$LINUX_ICON_SOURCE_PATH" "$APPDIR_PATH/hunk_desktop.png"
  ln -sf "usr/share/applications/hunk_desktop.desktop" "$APPDIR_PATH/hunk_desktop.desktop"
}

build_linux_appimage() {
  ensure_appimage_tooling
  create_linux_appdir

  ARCH=x86_64 "$APPIMAGE_TOOL_PATH" "$APPDIR_PATH" "$APPIMAGE_PATH"
  chmod +x "$APPIMAGE_PATH"
}

write_linux_system_wrapper() {
  local wrapper_path="$1"
  local launcher_path="$2"

  cat >"$wrapper_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "$launcher_path" "\$@"
EOF
  chmod 755 "$wrapper_path"
}

write_linux_system_desktop_entry() {
  cat >"$SYSTEM_DESKTOP_ENTRY_PATH" <<EOF
[Desktop Entry]
Categories=Development;
Comment=$PACKAGE_SUMMARY
Exec=$PACKAGE_NAME
Icon=/usr/share/pixmaps/$PACKAGE_NAME.png
Name=$PRODUCT_NAME
StartupNotify=true
StartupWMClass=hunk_desktop
Terminal=false
Type=Application
EOF
}

prepare_linux_system_install_root() {
  rm -rf "$SYSTEM_INSTALL_ROOT"
  mkdir -p "$SYSTEM_BIN_DIR" "$SYSTEM_PRIVATE_LIB_DIR" "$(dirname "$SYSTEM_RUNTIME_PATH")" "$SYSTEM_ICON_DIR" "$SYSTEM_PIXMAP_DIR" "$(dirname "$SYSTEM_DESKTOP_ENTRY_PATH")"

  cp "$PACKAGED_BINARY_PATH" "$SYSTEM_REAL_BINARY_PATH"
  cp "$PACKAGED_LAUNCHER_PATH" "$SYSTEM_LAUNCHER_PATH"
  cp -R "$PACKAGE_LIB_DIR/." "$SYSTEM_PRIVATE_LIB_DIR/"
  cp "$PACKAGED_CODEX_PATH" "$SYSTEM_RUNTIME_PATH"
  chmod +x "$SYSTEM_REAL_BINARY_PATH" "$SYSTEM_LAUNCHER_PATH" "$SYSTEM_RUNTIME_PATH"

  patch_linux_runtime_paths "$SYSTEM_REAL_BINARY_PATH" "$SYSTEM_PRIVATE_LIB_DIR" '$ORIGIN/lib'
  validate_linux_runtime_bundle "$SYSTEM_REAL_BINARY_PATH" "$SYSTEM_PRIVATE_LIB_DIR"

  write_linux_system_wrapper "$SYSTEM_WRAPPER_PATH" "/usr/lib/$PACKAGE_NAME/$PACKAGE_NAME"
  write_linux_system_wrapper "$SYSTEM_WRAPPER_ALIAS_PATH" "/usr/lib/$PACKAGE_NAME/$PACKAGE_NAME"
  write_linux_system_desktop_entry

  cp "$LINUX_ICON_SOURCE_PATH" "$SYSTEM_ICON_PATH"
  cp "$LINUX_ICON_SOURCE_PATH" "$SYSTEM_ICON_ALIAS_PATH"
  cp "$LINUX_ICON_SOURCE_PATH" "$SYSTEM_PIXMAP_PATH"

  "$ROOT_DIR/scripts/validate_release_bundle_layout.sh" linux-install-root "$SYSTEM_INSTALL_ROOT"
}

linux_deb_installed_size_kib() {
  du -sk "$DEB_BUILD_ROOT" | awk '{print $1}'
}

write_linux_deb_control_file() {
  local control_path="$1"

  {
    printf 'Package: %s\n' "$PACKAGE_NAME"
    printf 'Version: %s\n' "$DEB_VERSION"
    printf 'Section: %s\n' "$PACKAGE_SECTION"
    printf 'Priority: %s\n' "$PACKAGE_PRIORITY"
    printf 'Architecture: %s\n' "$DEB_ARCH"
    printf 'Maintainer: %s\n' "$PACKAGE_MAINTAINER"
    printf 'Installed-Size: %s\n' "$(linux_deb_installed_size_kib)"
    if [[ -n "${HUNK_LINUX_DEB_DEPENDS:-}" ]]; then
      printf 'Depends: %s\n' "$HUNK_LINUX_DEB_DEPENDS"
    fi
    if [[ -n "$PACKAGE_HOMEPAGE" ]]; then
      printf 'Homepage: %s\n' "$PACKAGE_HOMEPAGE"
    fi
    printf 'Description: %s\n' "$PACKAGE_SUMMARY"
    printf ' %s\n' "$PACKAGE_DESCRIPTION"
  } >"$control_path"
}

build_linux_deb_package() {
  require_linux_tool dpkg-deb

  rm -rf "$DEB_BUILD_ROOT" "$DEB_PATH"
  mkdir -p "$DEB_BUILD_ROOT"
  cp -a "$SYSTEM_INSTALL_ROOT/." "$DEB_BUILD_ROOT/"
  mkdir -p "$DEB_BUILD_ROOT/DEBIAN"
  write_linux_deb_control_file "$DEB_BUILD_ROOT/DEBIAN/control"

  dpkg-deb --root-owner-group --build "$DEB_BUILD_ROOT" "$DEB_PATH" >/dev/null
  echo "Created Linux Debian package at $DEB_PATH" >&2
}

write_linux_rpm_spec() {
  local spec_path="$1"

  {
    printf '%%global _build_id_links none\n'
    printf 'Name:           %s\n' "$PACKAGE_NAME"
    printf 'Version:        %s\n' "$RPM_VERSION"
    printf 'Release:        %s\n' "$PACKAGE_RELEASE"
    printf 'Summary:        %s\n' "$PACKAGE_SUMMARY"
    printf 'License:        %s\n' "$PACKAGE_LICENSE"
    printf 'Packager:       %s\n' "$PACKAGE_MAINTAINER"
    if [[ -n "$PACKAGE_HOMEPAGE" ]]; then
      printf 'URL:            %s\n' "$PACKAGE_HOMEPAGE"
    fi
    printf 'BuildArch:      %s\n' "$RPM_ARCH"
    printf '\n'
    printf '%%description\n'
    printf '%s\n' "$PACKAGE_DESCRIPTION"
    printf '\n'
    printf '%%prep\n'
    printf '\n'
    printf '%%build\n'
    printf '\n'
    printf '%%install\n'
    printf 'rm -rf %%{buildroot}\n'
    printf 'mkdir -p %%{buildroot}\n'
    printf 'cp -a %%{_hunk_install_root}/. %%{buildroot}/\n'
    printf '\n'
    printf '%%files\n'
    printf '/usr/bin/%s\n' "$PACKAGE_NAME"
    printf '/usr/bin/%s\n' "${PACKAGE_NAME//-/_}"
    printf '/usr/lib/%s\n' "$PACKAGE_NAME"
    printf '/usr/share/applications/%s.desktop\n' "$PACKAGE_NAME"
    printf '/usr/share/icons/hicolor/512x512/apps/%s.png\n' "$PACKAGE_NAME"
    printf '/usr/share/icons/hicolor/512x512/apps/%s.png\n' "${PACKAGE_NAME//-/_}"
    printf '/usr/share/pixmaps/%s.png\n' "$PACKAGE_NAME"
    printf '\n'
    printf '%%changelog\n'
    printf '* %s %s - %s-%s\n' "$(linux_rpm_changelog_date)" "$PACKAGE_MAINTAINER" "$RPM_VERSION" "$PACKAGE_RELEASE"
    printf '%s\n' '- Package release build.'
  } >"$spec_path"
}

build_linux_rpm_package() {
  require_linux_tool rpmbuild

  rm -rf "$RPM_TOPDIR" "$RPM_PATH"
  mkdir -p "$RPM_TOPDIR/BUILD" "$RPM_TOPDIR/BUILDROOT" "$RPM_TOPDIR/RPMS" "$RPM_TOPDIR/SOURCES" "$RPM_TOPDIR/SPECS" "$RPM_TOPDIR/SRPMS"

  local spec_path="$RPM_TOPDIR/SPECS/$PACKAGE_NAME.spec"
  write_linux_rpm_spec "$spec_path"

  rpmbuild \
    --define "_topdir $RPM_TOPDIR" \
    --define "_hunk_install_root $SYSTEM_INSTALL_ROOT" \
    --nodebuginfo \
    -bb "$spec_path" >/dev/null

  local built_rpm
  built_rpm="$(find "$RPM_TOPDIR/RPMS" -type f -name "*.rpm" | sort | head -n 1)"
  if [[ -z "$built_rpm" ]]; then
    echo "error: rpmbuild did not produce an RPM under $RPM_TOPDIR/RPMS" >&2
    exit 1
  fi

  cp "$built_rpm" "$RPM_PATH"
  echo "Created Linux RPM package at $RPM_PATH" >&2
}
