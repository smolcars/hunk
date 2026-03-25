set windows-shell := ["pwsh", "-Command"]
set export

start-mac:
    CARGO_TARGET_DIR="$(./scripts/resolve_cargo_target_dir.sh)" ./scripts/run_with_macos_sdk_env.sh cargo run -p hunk-desktop

start-windows:
    pwsh ./scripts/run_windows_dev.ps1

start-windows-release:
    pwsh ./scripts/run_windows_release.ps1

start-linux:
    ./scripts/run_linux_dev.sh

start-linux-release:
    ./scripts/run_linux_release.sh
    
fmt:
    cargo fmt --all

build:
    CARGO_TARGET_DIR="$(./scripts/resolve_cargo_target_dir.sh)" ./scripts/run_with_macos_sdk_env.sh cargo build -p hunk-desktop

build-worktree worktree:
    ./scripts/build_worktree.sh {{worktree}}

release:
    CARGO_TARGET_DIR="$(./scripts/resolve_cargo_target_dir.sh)" ./scripts/run_with_macos_sdk_env.sh cargo build -p hunk-desktop --release

build-linux:
    ./scripts/build_linux.sh

build-windows:
    ./scripts/build_windows.sh

build-linux-icon:
    ./scripts/build_linux_icon.sh

bundle:
    CARGO_TARGET_DIR="$(./scripts/resolve_cargo_target_dir.sh)" ./scripts/run_with_macos_sdk_env.sh cargo build -p hunk-desktop --release --locked
    cd crates/hunk-desktop && \
        CARGO_TARGET_DIR="$(../../scripts/resolve_cargo_target_dir.sh)" ../../scripts/run_with_macos_sdk_env.sh cargo packager \
            -p hunk-desktop \
            --manifest-path Cargo.toml \
            --release \
            -f app \
            --out-dir "$(../../scripts/resolve_cargo_target_dir.sh)/packager"

package-macos-release:
    ./scripts/package_macos_release.sh

package-linux-release:
    ./scripts/package_linux_release.sh

package-linux-appimage-release:
    ./scripts/package_linux_release.sh --formats appimage,tarball

package-linux-deb-release:
    ./scripts/package_linux_release.sh --formats deb

package-linux-rpm-release:
    ./scripts/package_linux_release.sh --formats rpm

install-linux-packaging-deps-ubuntu:
    ./scripts/install_linux_packaging_deps_ubuntu.sh

install-linux-deb-local:
    sudo apt-get install -y "$(./scripts/package_linux_release.sh --formats deb | tail -n 1)"

smoke-test-linux-deb:
    ./scripts/smoke_test_linux_system_package.sh deb

smoke-test-linux-rpm:
    ./scripts/smoke_test_linux_system_package.sh rpm

package-windows-release:
    pwsh ./scripts/package_windows_release.ps1

prod:
    osascript -e 'tell application "Hunk" to quit' || true
    just bundle
    open "$(./scripts/resolve_cargo_target_dir.sh)/packager/Hunk.app"

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
    CARGO_TARGET_DIR="$(./scripts/resolve_cargo_target_dir.sh)" ./scripts/run_with_macos_sdk_env.sh cargo test -p hunk-codex --test real_runtime_smoke -- --ignored
