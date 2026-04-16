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
              dbus.lib
              expat
              fontconfig
              glib
              libdrm
              libffi
              libglvnd
              libgit2
              libxkbcommon
              llvmPackages.libllvm
              mesa
              stdenv.cc.cc.lib
              vulkan-loader
              wayland
              libx11
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
            ];
            packages =
              with pkgs;
              [
                rustToolchain
                just
                zig
                openssl
                pkgconf
              ]
              ++ lib.optionals stdenv.isLinux [
                gcc
                gnumake
                clang
                cmake
                alsa-lib
                dbus
                expat
                fontconfig
                libgit2
                glib
                vulkan-loader
                wayland
                libx11
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
                export SDKROOT="$sdkroot"
                export LIBRARY_PATH="$sdkroot/usr/lib''${LIBRARY_PATH:+:$LIBRARY_PATH}"
                export CPATH="$sdkroot/usr/include''${CPATH:+:$CPATH}"
                export CFLAGS="-isysroot $sdkroot''${CFLAGS:+ $CFLAGS}"
                export CXXFLAGS="-isysroot $sdkroot''${CXXFLAGS:+ $CXXFLAGS}"
                export LDFLAGS="-L$sdkroot/usr/lib''${LDFLAGS:+ $LDFLAGS}"
                export RUSTFLAGS="-L native=$sdkroot/usr/lib''${RUSTFLAGS:+ $RUSTFLAGS}"
              fi
            '';
          };
        }
      );
    };
}
