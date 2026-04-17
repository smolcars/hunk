set windows-shell := ["pwsh", "-Command"]
set export := true

start-mac:
    cargo run -p hunk-desktop

start-mac-release:
    RUST_LOG=hunk_desktop=debug,hunk_codex=debug cargo run --release -p hunk-desktop

start-windows:
    pwsh ./scripts/run_windows_dev.ps1

start-windows-release:
    pwsh ./scripts/run_windows_release.ps1

start-linux:
    cargo run -p hunk-desktop

fmt:
    cargo fmt --all

build:
    cargo build -p hunk-desktop

build-worktree worktree:
    ./scripts/build_worktree.sh {{ worktree }}

release:
    cargo build -p hunk-desktop --release

build-linux:
    ./scripts/build_linux.sh

build-windows:
    ./scripts/build_windows.sh

build-linux-icon:
    ./scripts/build_linux_icon.sh

bundle:
    cargo build -p hunk-desktop --release --locked
    cd crates/hunk-desktop && \
        cargo packager \
            -p hunk-desktop \
            --manifest-path Cargo.toml \
            --release \
            -f app \
            --out-dir ../../target/packager

package-macos-release:
    ./scripts/package_macos_release.sh

package-linux-release:
    ./scripts/package_linux_release_zed_like.sh --formats tarball,deb,rpm

package-linux-deb-release:
    ./scripts/package_linux_release_zed_like.sh --formats deb

package-linux-rpm-release:
    ./scripts/package_linux_release_zed_like.sh --formats rpm

install-linux-packaging-deps-ubuntu:
    ./scripts/install_linux_packaging_deps_ubuntu.sh

install-linux-deb-local:
    sudo apt-get install -y "$(./scripts/package_linux_release_zed_like.sh --formats deb | tail -n 1)"

smoke-test-linux-deb:
    ./scripts/smoke_test_linux_system_package.sh deb

smoke-test-linux-rpm:
    ./scripts/smoke_test_linux_system_package.sh rpm

package-windows-release:
    pwsh ./scripts/package_windows_release.ps1

prod:
    osascript -e 'tell application "Hunk" to quit' || true
    just bundle
    open "./target/packager/Hunk.app"

validate-codex-runtime:
    ./scripts/validate_codex_runtime_bundle.sh

install-codex-runtime-macos:
    ./scripts/install_codex_runtime_macos.sh

stage-codex-runtime-macos:
    ./scripts/stage_codex_runtime_macos.sh

phase12-macos-smoke:
    ./scripts/install_codex_runtime_macos.sh
    ./scripts/validate_codex_runtime_bundle.sh --strict --platform macos
    ./scripts/stage_codex_runtime_macos.sh
