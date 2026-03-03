# hunk

A cross-platform Git diff viewer built with `gpui` + `gpui-component`.

## Why?

Nobody writes code anymore, people just review code. So we need the best diff viewer possible so that vibe engineers can review code and tell AI what to fix.

## What it includes

- Uses `jj` as the underlying Git implementation
- File tree for changed files
- Side-by-side diff viewer with per-line styling and line numbers
- Resizable split panes (tree + diff)
- Light/Dark mode toggle
- Refresh action
- `anyhow`-based error handling
- `tracing` + `tracing-subscriber` logging

## Workspace Layout

- `crates/hunk-domain`: config/state/db/diff/markdown domain logic
- `crates/hunk-jj`: JJ backend and graph/tree logic
- `crates/hunk-desktop`: GPUI desktop app binary

## Requirements

- macOS
- Xcode + command line tools
- Metal toolchain for GPUI shader compilation

### Run Dev Locally

```bash
cargo run -p hunk-desktop
```

Launch from anywhere, then use `File > Open Project...` (or `Cmd/Ctrl+Shift+O`) to choose a Git repository.

`cargo run -p hunk-desktop` starts from Terminal, so macOS may still present it like a terminal-launched app.

### Validate Workspace

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### For Production builds

```bash
cargo install cargo-bundle
cargo bundle -p hunk-desktop --release
open target/release/bundle/osx/Hunk.app
```

## Large Diff Stress Fixture

Generate a synthetic JJ repository with a very large working-copy diff:

```bash
./scripts/create_large_diff_repo.sh --lines 25000 --files 1 --force
```

The script prints the generated repo path and total diff size. Open that folder in Hunk to stress scrolling/render performance and watch the FPS badge in the toolbar.

Generate code-like diffs instead of plain text (`txt`, `js`, or `ts`):

```bash
./scripts/create_large_diff_repo.sh --lines 25000 --files 20 --lang ts --force
./scripts/create_large_diff_repo.sh --lines 25000 --files 20 --lang js --force
```

To spread the same total load across multiple files:

```bash
./scripts/create_large_diff_repo.sh --lines 6000 --files 4 --force
```

### Automated Perf Harness

Run the repeatable large-diff perf harness with threshold gating:

```bash
./scripts/run_perf_harness.sh
```

Run without threshold gating (metrics only):

```bash
./scripts/run_perf_harness.sh --no-gate
```

Protocol and metric definitions are documented in [PERFORMANCE_BENCHMARK.md](./docs/PERFORMANCE_BENCHMARK.md).
The harness script currently targets Unix-like shells (`bash`).

## Config

Hunk reads config from `~/.hunkdiff/config.toml`.
Keyboard shortcuts are configured in the `[keyboard_shortcuts]` table:

```toml
[keyboard_shortcuts]
toggle_sidebar_tree = ["cmd-b", "ctrl-b"]
open_project = ["cmd-shift-o", "ctrl-shift-o"]
save_current_file = ["cmd-s", "ctrl-s"]
open_settings = ["cmd-,", "ctrl-,"]
quit_app = ["cmd-q"]
```

Use an empty list to disable a shortcut action:

```toml
[keyboard_shortcuts]
quit_app = []
```

## Icons

Generate git-diff icon variants and rebuild the bundle:

```bash
./scripts/generate_diff_icons.py
./scripts/build_macos_icon.sh
cargo bundle -p hunk-desktop --release
```

Generated assets:

- `assets/icons/hunk-icon-default.png` (default/full color)
- `assets/icons/hunk-icon-dark.png` (dark appearance variant)
- `assets/icons/hunk-icon-mono.png` (monochrome/tint-friendly variant)

Current bundling uses `hunk-icon-default.png` -> `Hunk.icns`.

## Hot Reload (Bacon)

Install bacon once:

```bash
cargo install bacon
```

Start hot reload (default job is `run`):

```bash
bacon
```

Useful jobs:

```bash
bacon check
bacon test
bacon clippy
```

Keybindings in bacon UI:

- `r` -> run
- `c` -> check
- `t` -> test
- `l` -> clippy
