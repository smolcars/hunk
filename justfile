start:
    cargo run -p hunk-desktop

build:
    cargo build -p hunk-desktop

release:
    cargo build -p hunk-desktop --release

dev:
    bacon

bundle:
    cargo bundle -p hunk-desktop --release

prod:
    osascript -e 'tell application "Hunk" to quit' || true
    just bundle
    open target/release/bundle/osx/Hunk.app

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
