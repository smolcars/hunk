# Hunk

A GPUI based desktop app for fast git diff viewing. In the age of vibe coding, nobody looks write anymore, but just review code.
Hunk is a fast diff viewer that is extremely simple written in Rust.

- Production Git behavior should live in `crates/hunk-git`, using `gix` first and narrow `git2` fallbacks only when necessary. Do not shell out to the Git CLI from app code.
- When fixing a bug or adding a new feature, always switch to plan mode and plan first before writing code.
- After making an implementation plan, keep it updated as you work. When you finish a step or the status changes, update the plan before continuing.
- Simplicity first, make every change as simple as possible.
- Make sure code is scalable.
- Don't make files over 1000 lines long.
- When working with frontend, always use colors in theme.rs
- Tests always in crate-level `tests` directories (for example `crates/hunk-git/tests`)
- Make sure workspace clippy passes
- Make sure workspace builds pass
- Use Cargo's default `target/` directory instead of overriding `CARGO_TARGET_DIR`.
- For CARGO_HOME check this path /Volumes/hulk/dev/cache/cargo or the default CARGO_HOME path for rust, nowhere else on the machine.
- Do not run clippy and tests over and over again, run them after you finished your work and make sure they pass at the end. Just once is enough.
- When asked to update Codex, follow `docs/AI_CODEX_UPGRADE_WORKFLOW.md`. Hunk now consumes a fork branch, so upgrades require rebasing the fork onto the target upstream `rust-v...` tag, reapplying Hunk patches there, pushing the fork branch, and then refreshing Hunk's lockfile against that fork commit.
- Refresh the bundled runtimes after a Codex upgrade. The runtime download scripts pull official `openai/codex` release assets that match the locked Codex crate version unless explicitly overridden with `HUNK_CODEX_RUNTIME_REPO`.
- Update `docs/AI_CODEX_SPEC.md` with the new upstream tag/SHA and current fork commit, search for stale Codex version strings in docs, and expect small protocol/API fixes in `hunk-desktop` or `hunk-codex` after the bump.
- GPUI docs https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui/README.md
- GPUI component library docs https://docs.rs/gpui-component/latest/gpui_component/
- List of available prebuilt components https://longbridge.github.io/gpui-component/docs/components/
- GPUI layout https://gpui-ce.github.io/examples/layout/
- GPUI async tasks https://gpui-ce.github.io/examples/async-tasks/
- GPUI Styling https://gpui-ce.github.io/examples/styling/
- GPUI text https://gpui-ce.github.io/examples/text/
- GPUI shadow https://gpui-ce.github.io/examples/shadow/
- GPUI paths bench https://gpui-ce.github.io/examples/paths-bench/
- Lessons learned note for terminal focus restoration: `docs/Lessons Learned.md`
- Use frontend-skill whenever you're doing designs
- Frames must take no more than 8ms (120fps)

Important paths:
- `crates/hunk-terminal`: terminal integration, shell/session support, and terminal-facing workspace surfaces.
- `crates/hunk-text`: headless rope-backed text buffer, positions/ranges, transactions, and undo/redo primitives.
- `crates/hunk-language`: Tree-sitter language registry, queries, syntax highlighting, folding, preview highlighting, and language-intelligence seams.
- `crates/hunk-editor`: headless editor state for selections, viewport/display rows, folds, overlays, and editor commands.
- `crates/hunk-domain`: shared config/state types, markdown preview, and SQLite comment storage/migrations.
- `crates/hunk-git`: shared Git read/write behavior; keep production Git logic here instead of app crates.
- `crates/hunk-forge`: forge integration logic, remote/review workflows, and hosting-service coordination.
- `crates/hunk-updater`: update/download/install logic for desktop releases.
- `crates/hunk-desktop`: GPUI app binary, app lifecycle, controllers, and render surfaces.
- `crates/hunk-codex`: embedded Codex app-server integration, thread service, protocol boundary, and AI reducer/state logic.
- `crates/hunk-desktop/src/app`: GPUI app entry/types; `controller/` owns behavior, `render/` owns UI, `theme.rs` owns app colors.
