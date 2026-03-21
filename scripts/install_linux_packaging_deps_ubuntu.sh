#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "error: Ubuntu packaging dependency install only runs on Linux hosts" >&2
  exit 1
fi

if [[ ! -r /etc/os-release ]]; then
  echo "error: /etc/os-release is unavailable; cannot determine distro" >&2
  exit 1
fi

# shellcheck disable=SC1091
source /etc/os-release
if [[ "${ID:-}" != "ubuntu" && "${ID_LIKE:-}" != *debian* ]]; then
  echo "error: expected an Ubuntu/Debian-style host, got '${PRETTY_NAME:-unknown}'" >&2
  exit 1
fi

sudo apt-get update
sudo apt-get install -y \
  build-essential \
  clang \
  cmake \
  libasound2-dev \
  libfontconfig-dev \
  libgit2-dev \
  libglib2.0-dev \
  libssl-dev \
  libvulkan1 \
  libwayland-dev \
  libx11-xcb-dev \
  libxkbcommon-x11-dev \
  libzstd-dev \
  patchelf \
  pkg-config \
  rpm
