#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$("$ROOT_DIR/scripts/resolve_cargo_target_dir.sh" "$ROOT_DIR")"
TARGET_TRIPLE="aarch64-apple-darwin"
VERSION_LABEL="${HUNK_RELEASE_VERSION:-$("$ROOT_DIR/scripts/resolve_hunk_version.sh")}"
DIST_DIR="$TARGET_DIR/dist"
PACKAGER_OUT_DIR="$TARGET_DIR/packager/macos"
APP_PATH="$PACKAGER_OUT_DIR/Hunk.app"
APP_EXECUTABLE_PATH="$APP_PATH/Contents/MacOS/hunk_desktop"
APP_FRAMEWORKS_DIR="$APP_PATH/Contents/Frameworks"
DMG_PATH="$DIST_DIR/Hunk-$VERSION_LABEL-macos-arm64.dmg"
DMG_STAGE_DIR="$TARGET_DIR/dmg-stage"
MACOS_SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
MACOS_LINKER="/usr/bin/clang"
MACOS_CC="/usr/bin/clang"
MACOS_CXX="/usr/bin/clang++"
MACOS_AR="/usr/bin/ar"
MACOS_RANLIB="/usr/bin/ranlib"

validate_macos_binary_dependencies() {
  local paths_to_check=("$APP_EXECUTABLE_PATH")
  if [[ -d "$APP_FRAMEWORKS_DIR" ]]; then
    while IFS= read -r dylib_path; do
      [[ -n "$dylib_path" ]] && paths_to_check+=("$dylib_path")
    done < <(find "$APP_FRAMEWORKS_DIR" -type f -name '*.dylib' | sort)
  fi

  local candidate_path linked_libraries
  for candidate_path in "${paths_to_check[@]}"; do
    linked_libraries="$(otool -L "$candidate_path")"
    if printf '%s\n' "$linked_libraries" | grep -E '/(opt/homebrew|usr/local|opt/local|nix/store)/' >/dev/null; then
      echo "error: macOS binary links against non-system libraries: $candidate_path" >&2
      printf '%s\n' "$linked_libraries" >&2
      exit 1
    fi
  done

  echo "Verified macOS app binary dependencies are self-contained." >&2
}

list_non_system_macos_dylibs() {
  local binary_path="$1"
  otool -L "$binary_path" \
    | tail -n +2 \
    | awk '{print $1}' \
    | grep -E '^/(opt/homebrew|usr/local|opt/local|nix/store)/' || true
}

bundle_macos_non_system_dylibs() {
  local root_binary="$1"
  local dylib_queue=()
  local bundled_dylibs=()
  local index=0
  local dylib_path destination_path nested_path dylib_name current_path replacement_path

  while IFS= read -r dylib_path; do
    [[ -n "$dylib_path" ]] && dylib_queue+=("$dylib_path")
  done < <(list_non_system_macos_dylibs "$root_binary")

  if [[ ${#dylib_queue[@]} -eq 0 ]]; then
    echo "No external macOS dylibs need bundling." >&2
    return
  fi

  mkdir -p "$APP_FRAMEWORKS_DIR"
  echo "Bundling non-system macOS dylibs into Hunk.app..." >&2

  while [[ $index -lt ${#dylib_queue[@]} ]]; do
    dylib_path="${dylib_queue[$index]}"
    index=$((index + 1))
    if printf '%s\n' "${bundled_dylibs[@]}" | grep -Fx "$dylib_path" >/dev/null 2>&1; then
      continue
    fi

    bundled_dylibs+=("$dylib_path")
    while IFS= read -r nested_path; do
      [[ -n "$nested_path" ]] && dylib_queue+=("$nested_path")
    done < <(list_non_system_macos_dylibs "$dylib_path")
  done

  for dylib_path in "${bundled_dylibs[@]}"; do
    dylib_name="$(basename "$dylib_path")"
    destination_path="$APP_FRAMEWORKS_DIR/$dylib_name"
    cp -f "$dylib_path" "$destination_path"
    chmod u+w "$destination_path"
  done

  for dylib_path in "${bundled_dylibs[@]}"; do
    dylib_name="$(basename "$dylib_path")"
    install_name_tool -change "$dylib_path" "@executable_path/../Frameworks/$dylib_name" "$root_binary"
  done

  for dylib_path in "${bundled_dylibs[@]}"; do
    dylib_name="$(basename "$dylib_path")"
    destination_path="$APP_FRAMEWORKS_DIR/$dylib_name"
    install_name_tool -id "@loader_path/$dylib_name" "$destination_path"

    for current_path in "${bundled_dylibs[@]}"; do
      replacement_path="@loader_path/$(basename "$current_path")"
      install_name_tool -change "$current_path" "$replacement_path" "$destination_path" || true
    done
  done
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS release packaging must run on macOS" >&2
  exit 1
fi

echo "Downloading bundled Codex runtime for macOS..." >&2
"$ROOT_DIR/scripts/download_codex_runtime_unix.sh" macos >/dev/null
echo "Validating bundled Codex runtime for macOS..." >&2
"$ROOT_DIR/scripts/validate_codex_runtime_bundle.sh" --strict --platform macos >/dev/null
echo "Building macOS app bundle..." >&2

(
  cd "$ROOT_DIR"
  export CARGO_TARGET_DIR="$TARGET_DIR"
  export SDKROOT="$MACOS_SDKROOT"
  export CC="$MACOS_CC"
  export CXX="$MACOS_CXX"
  export AR="$MACOS_AR"
  export RANLIB="$MACOS_RANLIB"
  export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="$MACOS_LINKER"

  cargo build -p hunk-desktop --release --target "$TARGET_TRIPLE" --locked
  cargo packager -p hunk-desktop --release -f app --target "$TARGET_TRIPLE" --out-dir "$PACKAGER_OUT_DIR"
)

if [[ ! -d "$APP_PATH" ]]; then
  echo "error: expected macOS app bundle at $APP_PATH" >&2
  exit 1
fi

bundle_macos_non_system_dylibs "$APP_EXECUTABLE_PATH"
echo "Validating macOS app binary dependencies..." >&2
validate_macos_binary_dependencies "$APP_EXECUTABLE_PATH"

if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "Signing macOS app bundle with Developer ID..." >&2
  codesign --force --deep --options runtime --sign "$APPLE_SIGNING_IDENTITY" "$APP_PATH"
  codesign --verify --deep --strict --verbose=2 "$APP_PATH"
fi

rm -rf "$DMG_STAGE_DIR" "$DMG_PATH"
mkdir -p "$DMG_STAGE_DIR" "$DIST_DIR"
cp -R "$APP_PATH" "$DMG_STAGE_DIR/Hunk.app"
ln -s /Applications "$DMG_STAGE_DIR/Applications"

hdiutil create \
  -volname "Hunk" \
  -srcfolder "$DMG_STAGE_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH" >/dev/null

if [[ -n "${APPLE_NOTARY_KEY_ID:-}" || -n "${APPLE_NOTARY_ISSUER:-}" || -n "${APPLE_NOTARY_API_KEY_PATH:-}" ]]; then
  : "${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY is required for notarization}"
  : "${APPLE_NOTARY_KEY_ID:?APPLE_NOTARY_KEY_ID is required for notarization}"
  : "${APPLE_NOTARY_ISSUER:?APPLE_NOTARY_ISSUER is required for notarization}"
  : "${APPLE_NOTARY_API_KEY_PATH:?APPLE_NOTARY_API_KEY_PATH is required for notarization}"

  echo "Submitting DMG for notarization..." >&2
  xcrun notarytool submit \
    "$DMG_PATH" \
    --key "$APPLE_NOTARY_API_KEY_PATH" \
    --key-id "$APPLE_NOTARY_KEY_ID" \
    --issuer "$APPLE_NOTARY_ISSUER" \
    --wait
  echo "Stapling notarization tickets..." >&2
  xcrun stapler staple "$APP_PATH"
  xcrun stapler staple "$DMG_PATH"
  xcrun stapler validate "$DMG_PATH"
fi

echo "Created macOS release artifact at $DMG_PATH" >&2

printf '%s\n' "$DMG_PATH"
