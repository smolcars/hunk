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
deployment_target="${HUNK_MACOSX_DEPLOYMENT_TARGET:-12.0}"
export MACOSX_DEPLOYMENT_TARGET="$deployment_target"
export CMAKE_OSX_SYSROOT="$sdkroot"
export CMAKE_OSX_DEPLOYMENT_TARGET="$deployment_target"
export LIBRARY_PATH="$sdkroot/usr/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
export CPATH="$sdkroot/usr/include${CPATH:+:$CPATH}"
export CFLAGS="-isysroot $sdkroot -mmacosx-version-min=$deployment_target${CFLAGS:+ $CFLAGS}"
export CXXFLAGS="-isysroot $sdkroot -mmacosx-version-min=$deployment_target${CXXFLAGS:+ $CXXFLAGS}"
export LDFLAGS="-L$sdkroot/usr/lib -Wl,-macosx_version_min,$deployment_target${LDFLAGS:+ $LDFLAGS}"
export RUSTFLAGS="-L native=$sdkroot/usr/lib -C link-arg=-mmacosx-version-min=$deployment_target${RUSTFLAGS:+ $RUSTFLAGS}"

exec "$@"
