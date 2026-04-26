#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT_DIR/target"
TARGET_TRIPLE="aarch64-apple-darwin"
VERSION_LABEL="${HUNK_RELEASE_VERSION:-$("$ROOT_DIR/scripts/resolve_hunk_version.sh")}"
DIST_DIR="$TARGET_DIR/dist"
PACKAGER_OUT_DIR="$TARGET_DIR/packager/macos"
APP_PATH="$PACKAGER_OUT_DIR/Hunk.app"
APP_EXECUTABLE_PATH="$APP_PATH/Contents/MacOS/hunk_desktop"
APP_FRAMEWORKS_DIR="$APP_PATH/Contents/Frameworks"
BROWSER_CEF_RUNTIME_DIR="${HUNK_CEF_RUNTIME_DIR:-$ROOT_DIR/assets/browser-runtime/cef/macos/runtime}"
APP_BUNDLE_IDENTIFIER="com.niteshbalusu.hunk"
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
    if [[ "$candidate_path" == "$APP_EXECUTABLE_PATH" ]] \
      && printf '%s\n' "$linked_libraries" | grep -F '@rpath/libghostty-vt.dylib' >/dev/null \
      && ! otool -l "$candidate_path" | grep -F '@executable_path/../Frameworks' >/dev/null; then
      echo "error: macOS app binary still depends on @rpath/libghostty-vt.dylib without an app-bundle LC_RPATH" >&2
      printf '%s\n' "$linked_libraries" >&2
      exit 1
    fi
  done

  echo "Verified macOS app binary dependencies are self-contained." >&2
}

resolve_macos_runtime_dylib() {
  local install_name="$1"
  local search_root="$TARGET_DIR/$TARGET_TRIPLE/release/build"
  local resolved_path=""

  case "$install_name" in
    @rpath/libghostty-vt.dylib)
      if [[ ! -d "$search_root" ]]; then
        search_root="$TARGET_DIR/release/build"
      fi
      if [[ -d "$search_root" ]]; then
        resolved_path="$(find "$search_root" -path '*/out/ghostty-install/lib/libghostty-vt.dylib' | sort | head -n 1)"
      fi
      ;;
  esac

  [[ -n "$resolved_path" ]] || return 1
  printf '%s\n' "$resolved_path"
}

list_non_system_macos_dylibs() {
  local binary_path="$1"
  local install_name resolved_path

  while IFS= read -r install_name; do
    [[ -n "$install_name" ]] || continue
    if [[ "$install_name" =~ ^/(opt/homebrew|usr/local|opt/local|nix/store)/ ]]; then
      printf '%s\n' "$install_name"
      continue
    fi
    if resolved_path="$(resolve_macos_runtime_dylib "$install_name")"; then
      printf '%s\n' "$resolved_path"
    fi
  done < <(
    otool -L "$binary_path" \
      | tail -n +2 \
      | awk '{print $1}'
  )
}

ensure_macos_bundle_rpath() {
  local binary_path="$1"
  local bundle_rpath='@executable_path/../Frameworks'

  if ! otool -L "$binary_path" | grep -F '@rpath/libghostty-vt.dylib' >/dev/null; then
    return
  fi

  if otool -l "$binary_path" | grep -F "$bundle_rpath" >/dev/null; then
    return
  fi

  install_name_tool -add_rpath "$bundle_rpath" "$binary_path"
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
  ensure_macos_bundle_rpath "$root_binary"
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

sign_macos_app_bundle() {
  local sign_target

  while IFS= read -r sign_target; do
    [[ -n "$sign_target" ]] || continue
    if [[ "$sign_target" == "$APP_EXECUTABLE_PATH" ]]; then
      continue
    fi
    codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$sign_target"
  done < <(
    find "$APP_PATH/Contents" -type f \( -name '*.dylib' -o -perm -111 \) | sort
  )

  while IFS= read -r sign_target; do
    [[ -n "$sign_target" ]] || continue
    codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$sign_target"
  done < <(
    find "$APP_PATH/Contents/Frameworks" -type d \( -name '*.framework' -o -name '*.app' \) | sort -r
  )

  codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$APP_EXECUTABLE_PATH"
  codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$APP_PATH"
}

adhoc_sign_macos_app_bundle() {
  local sign_target

  while IFS= read -r sign_target; do
    [[ -n "$sign_target" ]] || continue
    if [[ "$sign_target" == "$APP_EXECUTABLE_PATH" ]]; then
      continue
    fi
    codesign --force --timestamp=none --sign - "$sign_target"
  done < <(
    find "$APP_PATH/Contents" -type f \( -name '*.dylib' -o -perm -111 \) | sort
  )

  while IFS= read -r sign_target; do
    [[ -n "$sign_target" ]] || continue
    codesign --force --timestamp=none --sign - "$sign_target"
  done < <(
    find "$APP_PATH/Contents/Frameworks" -type d \( -name '*.framework' -o -name '*.app' \) | sort -r
  )

  codesign --force --timestamp=none --sign - --identifier "$APP_BUNDLE_IDENTIFIER" "$APP_EXECUTABLE_PATH"
  codesign --force --timestamp=none --sign - --identifier "$APP_BUNDLE_IDENTIFIER" "$APP_PATH"
}

staple_macos_artifact_with_retry() {
  local artifact_path="$1"
  local max_attempts=6
  local attempt
  local stapler_output

  for attempt in $(seq 1 "$max_attempts"); do
    stapler_output="$(mktemp)"
    if xcrun stapler staple -v "$artifact_path" >"$stapler_output" 2>&1; then
      cat "$stapler_output" >&2
      : >"$stapler_output"
      if xcrun stapler validate -v "$artifact_path" >"$stapler_output" 2>&1; then
        cat "$stapler_output" >&2
        rm -f "$stapler_output"
        return 0
      fi
      cat "$stapler_output" >&2
    else
      cat "$stapler_output" >&2
    fi
    rm -f "$stapler_output"

    if [[ "$attempt" -lt "$max_attempts" ]]; then
      echo "Stapling attempt $attempt failed for $artifact_path; retrying after notarization propagation delay..." >&2
      sleep 15
    fi
  done

  echo "error: failed to staple notarization ticket to $artifact_path after $max_attempts attempts" >&2
  return 1
}

print_notary_submission_details() {
  local submission_output_path="$1"
  python3 - "$submission_output_path" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
data = json.loads(path.read_text())
print("Notary submission response:", file=sys.stderr)
print(json.dumps(data, indent=2, sort_keys=True), file=sys.stderr)
submission_id = data.get("id") or ""
status = data.get("status") or ""
print(f"NOTARY_SUBMISSION_ID={submission_id}")
print(f"NOTARY_STATUS={status}")
PY
}

print_notarytool_log() {
  local submission_id="$1"
  local notary_log_output

  if [[ -z "$submission_id" ]]; then
    return 0
  fi

  notary_log_output="$(mktemp)"
  echo "Fetching notarization log for submission $submission_id..." >&2
  if xcrun notarytool log \
    "$submission_id" \
    --key "$APPLE_NOTARY_API_KEY_PATH" \
    --key-id "$APPLE_NOTARY_KEY_ID" \
    --issuer "$APPLE_NOTARY_ISSUER" \
    --output-format json >"$notary_log_output" 2>&1; then
    cat "$notary_log_output" >&2
  else
    echo "warning: failed to fetch notarization log for submission $submission_id" >&2
    cat "$notary_log_output" >&2
  fi
  rm -f "$notary_log_output"
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS release packaging must run on macOS" >&2
  exit 1
fi

echo "Preparing bundled CEF runtime for macOS..." >&2
"$ROOT_DIR/scripts/prepare_browser_cef_runtime.sh" "$TARGET_TRIPLE" "$BROWSER_CEF_RUNTIME_DIR" >/dev/null
export CEF_PATH="$BROWSER_CEF_RUNTIME_DIR"
export DYLD_FALLBACK_LIBRARY_PATH="${DYLD_FALLBACK_LIBRARY_PATH:-}:$BROWSER_CEF_RUNTIME_DIR:$BROWSER_CEF_RUNTIME_DIR/Chromium Embedded Framework.framework/Libraries"

echo "Downloading bundled Codex runtime for macOS..." >&2
"$ROOT_DIR/scripts/download_codex_runtime_unix.sh" macos >/dev/null
echo "Validating bundled Codex runtime for macOS..." >&2
"$ROOT_DIR/scripts/validate_codex_runtime_bundle.sh" --strict --platform macos >/dev/null
echo "Building macOS app bundle..." >&2

(
  cd "$ROOT_DIR"
  export SDKROOT="$MACOS_SDKROOT"
  export MACOSX_DEPLOYMENT_TARGET="${HUNK_MACOSX_DEPLOYMENT_TARGET:-14.0}"
  export CMAKE_OSX_SYSROOT="$MACOS_SDKROOT"
  export CMAKE_OSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET"
  export CC="$MACOS_CC"
  export CXX="$MACOS_CXX"
  export AR="$MACOS_AR"
  export RANLIB="$MACOS_RANLIB"
  export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="$MACOS_LINKER"
  export LIBRARY_PATH="$MACOS_SDKROOT/usr/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
  export CPATH="$MACOS_SDKROOT/usr/include${CPATH:+:$CPATH}"
  export CFLAGS="-isysroot $MACOS_SDKROOT -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET${CFLAGS:+ $CFLAGS}"
  export CXXFLAGS="-isysroot $MACOS_SDKROOT -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET${CXXFLAGS:+ $CXXFLAGS}"
  export LDFLAGS="-L$MACOS_SDKROOT/usr/lib -Wl,-macosx_version_min,$MACOSX_DEPLOYMENT_TARGET${LDFLAGS:+ $LDFLAGS}"
  export RUSTFLAGS="-L native=$MACOS_SDKROOT/usr/lib -C link-arg=-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET${RUSTFLAGS:+ $RUSTFLAGS}"

  rm -rf "$APP_PATH"
  cargo build -p hunk-desktop --release --target "$TARGET_TRIPLE" --locked --features hunk-desktop/cef-browser
  cargo build -p hunk-browser-helper --release --target "$TARGET_TRIPLE" --locked --features hunk-browser-helper/cef-subprocess
  (
    cd "$ROOT_DIR/crates/hunk-desktop"
    cargo packager \
      -p hunk-desktop \
      --manifest-path Cargo.toml \
      --release \
      -f app \
      --target "$TARGET_TRIPLE" \
      --out-dir "$PACKAGER_OUT_DIR" \
      1>&2
  )
)

if [[ ! -d "$APP_PATH" ]]; then
  echo "error: expected macOS app bundle at $APP_PATH" >&2
  exit 1
fi

"$ROOT_DIR/scripts/package_browser_cef_macos.sh" \
  "$APP_PATH" \
  "$BROWSER_CEF_RUNTIME_DIR" \
  "$TARGET_DIR/$TARGET_TRIPLE/release/hunk-browser-helper"
rm -rf "$APP_PATH/Contents/Resources/browser-runtime"
"$ROOT_DIR/scripts/validate_release_bundle_layout.sh" macos-app "$APP_PATH"
bundle_macos_non_system_dylibs "$APP_EXECUTABLE_PATH"
echo "Validating macOS app binary dependencies..." >&2
validate_macos_binary_dependencies "$APP_EXECUTABLE_PATH"

if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  echo "Signing macOS app bundle with Developer ID..." >&2
  sign_macos_app_bundle
else
  echo "Ad-hoc signing macOS app bundle for local identity-sensitive features..." >&2
  adhoc_sign_macos_app_bundle
fi

codesign --verify --deep --strict --verbose=2 "$APP_PATH"

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
  local_notary_submission_output="$(mktemp)"
  local_notary_status_file="$(mktemp)"
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
    --wait \
    --output-format json >"$local_notary_submission_output"
  print_notary_submission_details "$local_notary_submission_output" >"$local_notary_status_file"
  # shellcheck disable=SC1090
  source "$local_notary_status_file"
  echo "Stapling notarization tickets..." >&2
  if ! staple_macos_artifact_with_retry "$DMG_PATH"; then
    print_notarytool_log "${NOTARY_SUBMISSION_ID:-}"
    rm -f "$local_notary_submission_output" "$local_notary_status_file"
    exit 1
  fi
  rm -f "$local_notary_submission_output" "$local_notary_status_file"
fi

echo "Created macOS release artifact at $DMG_PATH" >&2

printf '%s\n' "$DMG_PATH"
