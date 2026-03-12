set export

CARGO_TARGET_DIR := `./scripts/resolve_cargo_target_dir.sh`

start:
    cargo run -p hunk-desktop

build:
    cargo build -p hunk-desktop

build-worktree worktree:
    ./scripts/build_worktree.sh {{worktree}}

release:
    cargo build -p hunk-desktop --release

build-linux:
    ./scripts/build_linux.sh

build-windows:
    ./scripts/build_windows.sh

dev:
    bacon

bundle:
    cargo build -p hunk-desktop --release --locked
    cargo packager -p hunk-desktop --release -f app --out-dir "{{CARGO_TARGET_DIR}}/packager"

package-macos-release:
    ./scripts/package_macos_release.sh

package-linux-release:
    ./scripts/package_linux_release.sh

package-windows-release:
    pwsh ./scripts/package_windows_release.ps1

prod:
    osascript -e 'tell application "Hunk" to quit' || true
    just bundle
    open "{{CARGO_TARGET_DIR}}/packager/Hunk.app"

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
    cargo test -p hunk-codex --test real_runtime_smoke -- --ignored
