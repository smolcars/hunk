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
