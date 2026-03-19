set windows-shell := ["pwsh", "-Command"]
set export

start-mac:
    CARGO_TARGET_DIR="$(./scripts/resolve_cargo_target_dir.sh)" ./scripts/run_with_macos_sdk_env.sh cargo run -p hunk-desktop

start-windows:
    pwsh ./scripts/run_windows_dev.ps1

start-linux:
    ./scripts/run_linux_dev.sh

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
