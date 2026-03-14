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
        in
        {
          default = pkgs.mkShell {
            name = "hunk-dev-shell";
            packages =
              with pkgs;
              [
                rustToolchain
                just
                openssl
                pkgconf
              ]
              ++ lib.optionals stdenv.isLinux [
                gcc
                gnumake
                clang
                cmake
                alsa-lib
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
              ];

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
            '';
          };
        }
      );
    };
}
