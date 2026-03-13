# hunk

A cross-platform Git diff viewer and Codex orchestrator built with `gpui` + `gpui-component`.

## Why?

Nobody writes code anymore, people just review code. So we need the best diff viewer possible so that vibe engineers can review code and tell AI what to fix.
Hunk is also has full codex integration so you can use Codex inside of Hunk instead of codex-cli or any other desktop app.

<img width="3320" height="2032" alt="Hunk (Window) 2026-03-07 04:15 PM" src="https://github.com/user-attachments/assets/7d7284e1-3f2f-4bae-ae6b-bb51eea9e06b" />
<img width="3320" height="2032" alt="Hunk (Window) 2026-03-07 04:16 PM" src="https://github.com/user-attachments/assets/ff696ad9-a0ef-4023-ae1d-3d2c6b5036de" />
<img width="3320" height="2032" alt="Hunk (Window) 2026-03-07 04:36 PM" src="https://github.com/user-attachments/assets/2ae2fd5f-13b1-4270-b11f-d6d6c0dffe35" />
<img width="3320" height="2032" alt="Hunk (Window) 2026-03-07 04:53 PM" src="https://github.com/user-attachments/assets/7270c825-61cb-4c64-b63a-17d038808269" />



## What it includes

- Uses a native Git backend built on `gix` with narrow `git2` fallbacks for unsupported write flows
- Managed Git worktrees with per-worktree branch publishing
- File tree for changed files
- Side-by-side diff viewer with per-line styling and line numbers
- Review compare mode for `base branch <-> workspace target` and custom branch/worktree pairs
- AI drafts and threads scoped to the selected project checkout or worktree
- Resizable split panes (tree + diff)
- Light/Dark mode toggle
- Refresh action
- `anyhow`-based error handling
- `tracing` + `tracing-subscriber` logging

## Workspace Layout

- `crates/hunk-domain`: config/state/db/diff/markdown domain logic
- `crates/hunk-git`: Git backend for repo discovery, diffing, branches, commits, push, and sync
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

## Worktrees

Hunk treats the primary checkout and each linked Git worktree as separate workspace targets.

- Create and switch worktrees from the Git tab.
- Managed worktrees live under `~/.hunkdiff/worktrees/<repo-key>/worktree-N`.
- The Files and Git tabs follow the currently active workspace target.
- The Review tab defaults to comparing the active workspace target against the repo base branch, but you can also compare custom branch/worktree pairs.
- The AI tab can start a new thread in the primary checkout with `Cmd/Ctrl+N` or in a worktree-targeted draft with `Cmd/Ctrl+Shift+N`.

### Validate Workspace

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### For Production builds

```bash
cargo install cargo-packager
just bundle
open "$(./scripts/resolve_cargo_target_dir.sh)/packager/Hunk.app"
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

Release packaging helpers:

```bash
./scripts/package_macos_release.sh
./scripts/package_linux_release.sh
pwsh ./scripts/package_windows_release.ps1
```

These produce:

- macOS ARM64: signed/notarized `Hunk-<version>-macos-arm64.dmg` when Apple secrets are configured
- Linux x86_64: `Hunk-<version>-linux-x86_64.AppImage` plus fallback `Hunk-<version>-linux-x86_64.tar.gz`
- Windows x86_64: `Hunk-<version>-windows-x86_64.msi`

Linux release packaging requires `patchelf` for the tarball fallback bundle.

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
git clone https://github.com/openai/codex.git
cd codex
git checkout <pinned-commit-from-docs/AI_CODEX_SPEC.md>
cd codex-rs
cargo build -p codex-cli --release
```

From the Hunk repo root, copy the generated binary into this repo for the platform you built on:

- macOS: `cp /path/to/codex/codex-rs/target/release/codex assets/codex-runtime/macos/codex && chmod +x assets/codex-runtime/macos/codex`
- Linux: `cp /path/to/codex/codex-rs/target/release/codex assets/codex-runtime/linux/codex && chmod +x assets/codex-runtime/linux/codex`
- Windows: `copy C:/path/to/codex/codex-rs/target/release/codex.exe assets/codex-runtime/windows/codex.exe`

For CI and release packaging we now prefer downloading the pinned Codex release assets directly:

- macOS ARM64: `codex-aarch64-apple-darwin.tar.gz`
- Linux x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`
- Windows x86_64: `codex-x86_64-pc-windows-msvc.exe.zip`

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

## GitHub Actions Release Flow

- `.github/workflows/pr-build.yml` stays as the main PR CI workflow.
- `.github/workflows/release.yml` builds DMG/MSI/AppImage artifacts and publishes them to a GitHub Release when you push a `v*` tag.

Apple signing/notarization secrets used by the workflows:

- `APPLE_CERTIFICATE_P12_BASE64`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_NOTARY_API_KEY_BASE64`
- `APPLE_NOTARY_KEY_ID`
- `APPLE_NOTARY_ISSUER`

Windows signing is not configured in this repo yet. The current release workflow produces an unsigned MSI until you add a paid Windows signing solution.

## Large Diff Stress Fixture

Generate a synthetic Git repository with a very large working-copy diff:

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
./scripts/build_windows_icon.sh
just bundle
```

Generated assets:

- `assets/icons/hunk-icon-default.png` (default/full color)
- `assets/icons/hunk-icon-dark.png` (dark appearance variant)
- `assets/icons/hunk-icon-mono.png` (monochrome/tint-friendly variant)

Current packaging uses `Hunk.icns`, `Hunk.ico`, and `hunk_new.png` through `cargo-packager`.

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
