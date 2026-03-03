# Hunk

A GPUI based desktop app for fast git diff viewing. In the age of vibe coding, nobody looks write anymore, but just review code.
Hunk is a fast diff viewer that is extremely simple written in Rust.

- ONLY USE JJ AS THE UNDERLYING GIT IMPLEMENTATION WHEN RUNNING GIT RELATED COMMANDS. JJ CLI IS AVAILABLE VIA `jj` ON THE PATH.
- Make sure code is scalable.
- Don't make files over 1000 lines long.
- Tests always in crate-level `tests` directories (for example `crates/hunk-jj/tests`)
- Make sure workspace clippy passes
- Make sure workspace builds pass
- GPUI docs https://raw.githubusercontent.com/zed-industries/zed/refs/heads/main/crates/gpui/README.md
- GPUI component library docs https://docs.rs/gpui-component/latest/gpui_component/
- List of available prebuilt components https://longbridge.github.io/gpui-component/docs/components/
- GPUI layout https://gpui-ce.github.io/examples/layout/
- GPUI async tasks https://gpui-ce.github.io/examples/async-tasks/
- GPUI Styling https://gpui-ce.github.io/examples/styling/
- GPUI text https://gpui-ce.github.io/examples/text/
- GPUI shadow https://gpui-ce.github.io/examples/shadow/
- GPUI paths bench https://gpui-ce.github.io/examples/paths-bench/

Use GPUI Skills whenever needed
