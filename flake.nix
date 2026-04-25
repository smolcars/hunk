{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" ];
          };
          linuxRuntimeLibraries =
            with pkgs;
            [
              alsa-lib
              at-spi2-atk
              atk
              bzip2
              cairo
              cups
              dbus.lib
              expat
              fontconfig
              glib
              libcap
              libdrm
              libffi
              libgbm
              libglvnd
              libgit2
              libxkbcommon
              llvmPackages.libllvm
              mesa
              nspr
              nss
              openssl
              pango
              stdenv.cc.cc.lib
              udev
              vulkan-loader
              wayland
              libx11
              libxcomposite
              libxcursor
              libxdamage
              libxext
              libxfixes
              libxi
              libxrandr
              libxrender
              libxcb
              libxshmfence
              zlib
              zstd
            ];
          linuxPackagingTools =
            with pkgs;
            [
              curl
              dpkg
              file
              rpm
            ];
        in
        {
          default = pkgs.mkShell {
            name = "hunk-dev-shell";
            buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.dbus.dev
              pkgs.dbus.lib
              pkgs.libcap
            ];
            packages =
              with pkgs;
              [
                rustToolchain
                just
                zig
                cmake
                ninja
                openssl
                pkgconf
              ]
              ++ lib.optionals stdenv.isLinux [
                gcc
                gnumake
                clang
                alsa-lib
                at-spi2-atk
                atk
                bzip2
                cairo
                cups
                libcap
                dbus
                expat
                fontconfig
                libgit2
                glib
                libgbm
                nspr
                nss
                pango
                udev
                vulkan-loader
                wayland
                libx11
                libxcomposite
                libxcb
                libxkbcommon
                zstd
                patchelf
              ]
              ++ lib.optionals stdenv.isLinux linuxPackagingTools;

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.isLinux (
              pkgs.lib.makeLibraryPath linuxRuntimeLibraries
            );
            LIBGL_DRIVERS_PATH = pkgs.lib.optionalString pkgs.stdenv.isLinux (
              "${pkgs.mesa}/lib/dri"
            );
            shellHook = ''
              if [ -d "$HOME/.cargo/bin" ]; then
                export PATH="$PATH:$HOME/.cargo/bin"
              fi

              if [ "$(uname -s)" = "Linux" ]; then
                export HUNK_LINUX_HOST_GRAPHICS_LIBRARY_PATHS="/usr/lib/x86_64-linux-gnu:/lib/x86_64-linux-gnu:/usr/lib64"
                export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER="$PWD/scripts/run_with_linux_graphics_env.sh"
              fi

              if [ "$(uname -s)" = "Darwin" ]; then
                sdkroot="$(xcrun --sdk macosx --show-sdk-path)"
                deployment_target="''${HUNK_MACOSX_DEPLOYMENT_TARGET:-14.0}"
                export SDKROOT="$sdkroot"
                export MACOSX_DEPLOYMENT_TARGET="$deployment_target"
                export CMAKE_OSX_SYSROOT="$sdkroot"
                export CMAKE_OSX_DEPLOYMENT_TARGET="$deployment_target"
                export LIBRARY_PATH="$sdkroot/usr/lib''${LIBRARY_PATH:+:$LIBRARY_PATH}"
                export CPATH="$sdkroot/usr/include''${CPATH:+:$CPATH}"
                export CFLAGS="-isysroot $sdkroot -mmacosx-version-min=$deployment_target''${CFLAGS:+ $CFLAGS}"
                export CXXFLAGS="-isysroot $sdkroot -mmacosx-version-min=$deployment_target''${CXXFLAGS:+ $CXXFLAGS}"
                export LDFLAGS="-L$sdkroot/usr/lib -Wl,-macosx_version_min,$deployment_target''${LDFLAGS:+ $LDFLAGS}"
                export RUSTFLAGS="-L native=$sdkroot/usr/lib -C link-arg=-mmacosx-version-min=$deployment_target''${RUSTFLAGS:+ $RUSTFLAGS}"
              fi
            '';
          };
        }
      );
    };
}
