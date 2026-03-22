# Hunk

A GPUI based desktop app for fast git diff viewing. In the age of vibe coding, nobody looks write anymore, but just review code.
Hunk is a fast diff viewer that is extremely simple written in Rust.

- Production Git behavior should live in `crates/hunk-git`, using `gix` first and narrow `git2` fallbacks only when necessary. Do not shell out to the Git CLI from app code.
- Make sure code is scalable.
- Don't make files over 1000 lines long.
- When working with frontend, always use colors in theme.rs
- Tests always in crate-level `tests` directories (for example `crates/hunk-git/tests`)
- Make sure workspace clippy passes
- Make sure workspace builds pass
- Always resolve `CARGO_TARGET_DIR` via `./scripts/resolve_cargo_target_dir.sh` or the existing `just` recipes so builds, clippy runs, and tests write only to the shared `target-shared` directory and save disk space across worktrees.
- On macOS, run cargo via `./scripts/run_with_macos_sdk_env.sh` so build scripts can link against the SDK `iconv` stubs without ad hoc env exports.
- For CARGO_HOME check this path /Volumes/hulk/dev/cache/cargo or the default CARGO_HOME path for rust, nowhere else on the machine.
- Do not run clippy and tests over and over again, run them after you finished your work and make sure they pass at the end. Just once is enough.
- GPUI docs https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui/README.md
- GPUI component library docs https://docs.rs/gpui-component/latest/gpui_component/
- List of available prebuilt components https://longbridge.github.io/gpui-component/docs/components/
- GPUI layout https://gpui-ce.github.io/examples/layout/
- GPUI async tasks https://gpui-ce.github.io/examples/async-tasks/
- GPUI Styling https://gpui-ce.github.io/examples/styling/
- GPUI text https://gpui-ce.github.io/examples/text/
- GPUI shadow https://gpui-ce.github.io/examples/shadow/
- GPUI paths bench https://gpui-ce.github.io/examples/paths-bench/
- Use frontend-skill whenever you're doing designs

Important paths:
- `crates/hunk-codex`: Codex host/process integration, thread service, and AI reducer/state logic.
- `crates/hunk-git`: shared Git read/write behavior; keep production Git logic here instead of app crates.
- `crates/hunk-domain`: shared config/state types, markdown preview, and SQLite comment storage/migrations.
- `crates/hunk-text`: headless rope-backed text buffer, positions/ranges, transactions, and undo/redo primitives.
- `crates/hunk-language`: Tree-sitter language registry, queries, syntax highlighting, folding, preview highlighting, and language-intelligence seams.
- `crates/hunk-editor`: headless editor state for selections, viewport/display rows, folds, overlays, and editor commands.
- `crates/hunk-desktop/src/app`: GPUI app entry/types; `controller/` owns behavior, `render/` owns UI, `theme.rs` owns app colors.
