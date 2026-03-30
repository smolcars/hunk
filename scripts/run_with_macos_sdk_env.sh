#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 0 ]]; then
  echo "usage: $0 <command> [args...]" >&2
  exit 64
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  exec "$@"
fi

sdkroot="$(xcrun --sdk macosx --show-sdk-path)"

if [[ ! -e "$sdkroot/usr/lib/libiconv.tbd" ]]; then
  xcode_sdk_root="/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk"
  if [[ -e "$xcode_sdk_root/usr/lib/libiconv.tbd" ]]; then
    sdkroot="$xcode_sdk_root"
  fi
fi

export SDKROOT="$sdkroot"
export LIBRARY_PATH="$sdkroot/usr/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
# Keep the SDK available for Rust/linker resolution, but do not inject the SDK
# include paths globally. Zig's vendored libc++ build for libghostty-vt breaks
# if CPATH/CXXFLAGS force the macOS C headers ahead of libc++'s own headers.
export LDFLAGS="-L$sdkroot/usr/lib${LDFLAGS:+ $LDFLAGS}"
export RUSTFLAGS="-L native=$sdkroot/usr/lib${RUSTFLAGS:+ $RUSTFLAGS}"

exec "$@"
