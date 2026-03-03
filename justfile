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
