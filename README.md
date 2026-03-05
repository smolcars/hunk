# hunk

A cross-platform Git diff viewer and Codex orchestrator built with `gpui` + `gpui-component`.

## Why?

Nobody writes code anymore, people just review code. So we need the best diff viewer possible so that vibe engineers can review code and tell AI what to fix.
Hunk is also has full codex integration so you can use Codex inside of Hunk instead of codex-cli or any other desktop app.

<img width="3320" height="2032" alt="Hunk (Window) 2026-03-03 09:27 AM" src="https://github.com/user-attachments/assets/8c67f351-dde7-4d44-83ea-0c232a62a147" />
<img width="3320" height="2032" alt="Hunk (Window) 2026-03-03 09:42 AM" src="https://github.com/user-attachments/assets/5de96595-aa50-4f4c-b62e-170454126f3b" />
<img width="3320" height="2032" alt="Hunk (Window) 2026-03-03 09:44 AM" src="https://github.com/user-attachments/assets/313b421d-f53d-4b2e-8288-30dcc735e75c" />
<img width="3320" height="2032" alt="Hunk (Window) 2026-03-04 10:13 AM" src="https://github.com/user-attachments/assets/47d8d7ac-e5aa-4113-9485-362f281dd07e" />


## What it includes

- Uses `jj` as the underlying Git implementation
- File tree for changed files
- Side-by-side diff viewer with per-line styling and line numbers
- Resizable split panes (tree + diff)
- Light/Dark mode toggle
- Refresh action
- Git workspace staged loading for faster first paint
- `anyhow`-based error handling
- `tracing` + `tracing-subscriber` logging

## Workspace Layout

- `crates/hunk-domain`: config/state/db/diff/markdown domain logic
- `crates/hunk-jj`: JJ backend and graph/tree logic
- `crates/hunk-desktop`: GPUI desktop app binary
- `crates/hunk-codex`: Codex Websocket Server handling logic

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

Cross-platform binary build helpers:

```bash
./scripts/build_linux.sh
./scripts/build_windows.sh
```

Optional flags:

- `--target <triple>` to override the default target triple (must match the script platform).
- `--debug` to build debug artifacts.
- `--no-stage-runtime` to skip copying the bundled Codex runtime into the target output tree.

## Build Codex App-Server Binaries For Embedding

Hunk embeds a native `codex` runtime and launches app-server mode with:
`codex app-server --listen ws://127.0.0.1:<port>`.

Expected embedded runtime paths:

- `assets/codex-runtime/macos/codex`
- `assets/codex-runtime/linux/codex`
- `assets/codex-runtime/windows/codex.exe`

Runtime layout details also live in [`assets/codex-runtime/README.md`](./assets/codex-runtime/README.md).

### Build `codex` from source (upstream pin)

The pinned upstream baseline is tracked in [`docs/AI_CODEX_SPEC.md`](./docs/AI_CODEX_SPEC.md).

```bash
jj git clone https://github.com/openai/codex.git
cd codex
jj edit <pinned-commit-from-docs/AI_CODEX_SPEC.md>
cd codex-rs
cargo build -p codex-cli --release
```

From the Hunk repo root, copy the generated binary into this repo for the platform you built on:

- macOS: `cp /path/to/codex/codex-rs/target/release/codex assets/codex-runtime/macos/codex && chmod +x assets/codex-runtime/macos/codex`
- Linux: `cp /path/to/codex/codex-rs/target/release/codex assets/codex-runtime/linux/codex && chmod +x assets/codex-runtime/linux/codex`
- Windows: `copy C:/path/to/codex/codex-rs/target/release/codex.exe assets/codex-runtime/windows/codex.exe`

### Validate + stage + bundle (macOS workflow today)

If you already have `codex` installed on your PATH (npm/homebrew/releases), use:

```bash
./scripts/install_codex_runtime_macos.sh
./scripts/validate_codex_runtime_bundle.sh --strict --platform macos
./scripts/stage_codex_runtime_macos.sh
cargo test -p hunk-codex --test real_runtime_smoke -- --ignored
just bundle
```

You can also pass an explicit source binary path to the installer:
`./scripts/install_codex_runtime_macos.sh /absolute/path/to/codex`

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
