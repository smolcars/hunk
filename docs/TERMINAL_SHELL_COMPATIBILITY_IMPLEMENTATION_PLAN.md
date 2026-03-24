# Terminal Shell Compatibility Implementation Plan

## Status

- Proposed
- Owner: Hunk
- Last Updated: 2026-03-23

## Summary

This document defines the implementation plan for improving Hunk's built-in terminal so it behaves much closer to the user's normal shell environment on macOS, Linux, and Windows.

The immediate problem is not the PTY or VT surface. Hunk already has a PTY-backed terminal surface in `crates/hunk-terminal`. The compatibility gap is in:

- shell selection
- shell startup mode
- GUI-launched environment hydration
- per-directory environment resolution
- Windows default shell behavior

Today Hunk starts the AI terminal from `crates/hunk-desktop/src/app/controller/ai/terminal.rs` and delegates shell launch to `crates/hunk-terminal/src/session.rs`. That path currently:

- uses `SHELL` on Unix or `COMSPEC` on Windows
- launches an empty Unix terminal as interactive-only, not login+interactive
- launches commands on Windows with PowerShell `-NoProfile` behavior or `cmd /C`
- does not resolve a shell-specific environment before spawning the visible terminal
- does not expose terminal shell configuration in app config

The recommended implementation is to copy Zed's architecture, not Zed's entire code:

1. add explicit terminal shell settings
2. add a shell environment capture layer
3. optionally hydrate the app process environment on GUI launch
4. pass resolved environment and shell arguments into terminal spawn
5. prefer `pwsh` over `cmd.exe` on Windows when available

## Goals

- Make the built-in terminal behave much closer to the user's normal terminal session.
- Support aliases, PATH changes, shell functions, and toolchain bootstrap that come from login shell startup.
- Respect per-directory environment changes where practical.
- Improve Windows defaults so Hunk does not unnecessarily fall back to `cmd.exe`.
- Keep the implementation understandable and smaller than Zed's full terminal/project environment subsystem.

## Non-Goals For The First Iteration

- Full parity with Terminal.app, iTerm, WezTerm, or Windows Terminal profiles.
- Reproducing every external terminal profile feature.
- Supporting remote shell/environment resolution.
- Adding `direnv`, `mise`, or venv activation in the first slice unless they fall out naturally from shell startup.
- Changing non-terminal internal subprocess behavior unless explicitly scoped in a later phase.

## Current Hunk State

### Relevant Hunk Files

- `crates/hunk-terminal/src/session.rs`
  - owns PTY creation and shell command construction
  - current `TerminalSpawnRequest` only carries `cwd`, `command`, `rows`, `cols`, and optional shell override
- `crates/hunk-desktop/src/app/controller/ai/terminal.rs`
  - starts the visible AI terminal session with `TerminalSpawnRequest::shell(cwd)`
- `crates/hunk-desktop/src/app/render/ai.rs`
  - labels the active shell from `SHELL` or `COMSPEC`
- `crates/hunk-domain/src/config.rs`
  - current app config has no terminal shell or terminal environment configuration
- `crates/hunk-git/src/command_env.rs`
  - already contains related prior art for GUI-launch PATH repair for Git subprocesses
- `crates/hunk-desktop/src/main.rs`
  - likely home for app-level environment hydration before the UI starts
- `crates/hunk-desktop/src/app/controller/core_bootstrap.rs`
  - current config load path for app state and config

### Current Behavior That Causes User-Facing Gaps

- On Unix, the default shell path comes from `SHELL`, but an empty shell session is launched with `-i` only.
- On Unix, commands run with `-l -c`, but the visible shell session itself is not login-style.
- On Windows, the default shell comes from `COMSPEC`, which commonly means `cmd.exe`, not PowerShell 7.
- The terminal spawn path does not currently accept a resolved environment map.
- The terminal does not currently capture environment in the target working directory before spawning the visible shell.

## What Zed Does That We Should Reuse

Zed has a more complete solution spread across terminal settings, shell detection, app startup env hydration, and per-directory env capture.

### Zed Files Reviewed

- `/tmp/zed/crates/settings_content/src/terminal.rs`
- `/tmp/zed/crates/terminal/src/terminal_settings.rs`
- `/tmp/zed/crates/project/src/environment.rs`
- `/tmp/zed/crates/project/src/terminals.rs`
- `/tmp/zed/crates/util/src/shell_env.rs`
- `/tmp/zed/crates/util/src/shell.rs`
- `/tmp/zed/crates/util/src/util.rs`
- `/tmp/zed/crates/zed/src/main.rs`

### Zed Patterns Worth Adopting

1. Explicit terminal shell configuration
   - Zed models terminal shell as settings, not just `SHELL` or `COMSPEC`.
   - It supports `system`, `program`, and `program + args`.

2. Startup environment hydration for GUI launches
   - On Unix, if Zed is not already running under a PTY, it loads login-shell environment into the app process during startup.
   - This narrows the gap between launching from Finder and launching from a terminal.

3. Per-directory environment capture
   - Zed captures environment for a specific `(shell, directory)` pair and caches it.
   - It does this by launching the configured shell in that directory and asking Zed CLI to print environment as JSON.

4. Shell-aware environment capture
   - Zed does not assume all shells behave the same.
   - On Unix it generally uses login+interactive shell startup for capture.
   - On Windows it uses shell-specific invocation rules and normalizes `Path` to `PATH`.

5. Better Windows shell defaulting
   - Zed prefers `pwsh.exe` and `powershell.exe` before falling back to `cmd.exe`.
   - This is better than following `COMSPEC` blindly.

### Zed Scope We Should Not Copy In V1

- remote environment resolution
- full project/task terminal integration
- `direnv` direct mode
- Python venv activation plumbing
- the full settings schema and settings UI stack

## Recommended Hunk Architecture

The Hunk version should stay smaller than Zed's implementation while copying its key architecture.

### New Responsibilities

#### `crates/hunk-domain`

- add terminal configuration types to `AppConfig`
- persist terminal shell preferences and environment policy

#### `crates/hunk-terminal`

- accept shell program, shell args, and resolved environment in `TerminalSpawnRequest`
- apply that environment to the spawned PTY child process
- separate visible shell launch from shell environment capture helpers

#### `crates/hunk-desktop`

- resolve terminal shell settings
- resolve terminal environment before spawning a visible shell
- optionally hydrate app-level login environment during startup
- keep AI terminal UX behavior unchanged aside from improved shell parity

#### New small helper module or crate

Recommended location:

- `crates/hunk-terminal/src/shell_env.rs`, or
- `crates/hunk-desktop/src/app/controller/ai/shell_env.rs`, or
- a new small shared crate if later reuse is expected

Preferred initial choice:

- keep it inside `crates/hunk-terminal` if we want terminal runtime ownership to include shell launch concerns
- keep it inside `crates/hunk-desktop` if we want to avoid overloading the terminal crate with app-specific config/policy

Recommendation:

- create a small shared shell helper module in `crates/hunk-terminal` for shell spawning and capture primitives
- keep config resolution and caching in `crates/hunk-desktop`

## Proposed Config Model

Add a terminal config section to `AppConfig` in `crates/hunk-domain/src/config.rs`.

### Proposed Fields

- `terminal.shell`
  - enum:
    - `system`
    - `program`
    - `with_arguments`
- `terminal.inherit_login_environment`
  - `bool`
  - default: `true`
- `terminal.hydrate_app_environment_on_launch`
  - `bool`
  - default: `true` on Unix, `false` on Windows for the first slice
- `terminal.environment_cache_scope`
  - enum:
    - `per_shell`
    - `per_shell_and_directory`
  - default: `per_shell_and_directory`

Optional later field:

- `terminal.env`
  - key/value overrides merged after capture

### Why Keep This Configurable

- shell startup can be slow
- shell config can be broken or interactive
- users may want a stable, isolated Hunk terminal
- we need an escape hatch if startup parity causes regressions

## Step-By-Step Implementation Plan

### Phase 1: Add Terminal Config Types

Files:

- `crates/hunk-domain/src/config.rs`
- tests in `crates/hunk-domain/tests`

Tasks:

1. Add terminal config structs and enums to `AppConfig`.
2. Add serde defaults so older config files continue to load unchanged.
3. Add tests that verify:
   - default config deserializes correctly
   - shell config round-trips
   - missing terminal section falls back to defaults

Deliverable:

- Hunk can persist terminal shell/environment policy without changing runtime behavior yet.

### Phase 2: Extend Terminal Spawn API

Files:

- `crates/hunk-terminal/src/session.rs`
- `crates/hunk-terminal/src/lib.rs`
- `crates/hunk-terminal/tests/terminal_session.rs`

Tasks:

1. Extend `TerminalSpawnRequest` with:
   - `shell_program_override`
   - `shell_args_override`
   - `env_overrides`
2. Update `shell_command_builder` so visible shell launches can use explicit shell args.
3. Apply resolved environment variables to `CommandBuilder` before spawn.
4. Keep current TERM/COLORTERM behavior.
5. Preserve current PTY/VT behavior unchanged.

Deliverable:

- Hunk terminal runtime can launch a visible shell with an explicit shell + args + env.

### Phase 3: Add Shell Resolution Helpers

Files:

- new helper module in `crates/hunk-terminal` or `crates/hunk-desktop`
- tests near that module

Tasks:

1. Introduce a shell config model similar to Zed's:
   - system shell
   - explicit program
   - explicit program with args
2. Add platform-aware shell resolution:
   - Unix:
     - use configured shell
     - else use `SHELL` if valid
     - else fall back to `/bin/bash`, then `/bin/sh`
   - Windows:
     - prefer `pwsh.exe`
     - then `powershell.exe`
     - then `cmd.exe`
3. Do not rely on `COMSPEC` as the primary source on Windows.

Reference from Zed:

- `/tmp/zed/crates/util/src/shell.rs`

Deliverable:

- Hunk has a deterministic shell selection layer instead of ambient env-only behavior.

### Phase 4: Implement Shell Environment Capture

Files:

- new shell env helper module
- tests for shell argument construction and env parsing

Tasks:

1. Implement environment capture for a given `(shell, shell_args, cwd)`.
2. On Unix:
   - capture using login+interactive shell startup by default
   - `cd` into the target directory before capturing
3. On Windows:
   - add shell-specific capture rules
   - normalize `Path` to `PATH`
   - support Windows in V1 rather than deferring it to a later pass
4. Make the capture output structured and reusable by runtime code.
5. Keep this logic separate from the visible terminal spawn path.

Recommended implementation shape:

- create a tiny Hunk-side helper executable mode or helper function that prints `std::env::vars()` as JSON
- invoke it through the configured shell after startup files have run

Reference from Zed:

- `/tmp/zed/crates/util/src/shell_env.rs`

Important simplification for Hunk:

- skip `direnv` direct integration in the first slice
- rely on shell startup and directory `cd` side effects first
- keep a safe fallback path when Windows profile startup proves noisy or slow during environment capture

Deliverable:

- Hunk can resolve the shell environment the user expects in the target directory.

### Phase 5: Cache Captured Environments

Files:

- likely `crates/hunk-desktop/src/app/controller/ai`
- possibly small shared state in `app/types.rs`

Tasks:

1. Cache shell environment by `(resolved shell, shell args, cwd)`.
2. Avoid re-capturing environment every time the terminal drawer opens.
3. Invalidate cache when:
   - terminal config changes
   - cwd changes
   - shell selection changes
4. Keep cache in memory only for the first slice.

Reference from Zed:

- `/tmp/zed/crates/project/src/environment.rs`

Deliverable:

- terminal startup does not become unnecessarily slow after the first launch in a directory.

### Phase 6: Use Resolved Shell + Env In The AI Terminal

Files:

- `crates/hunk-desktop/src/app/controller/ai/terminal.rs`
- `crates/hunk-desktop/src/app/render/ai.rs`

Tasks:

1. Replace the direct `TerminalSpawnRequest::shell(cwd)` call with a resolved terminal launch request.
2. When starting the default shell:
   - resolve configured shell
   - resolve env for the target cwd
   - pass shell args and env into `TerminalSpawnRequest`
3. Update shell labeling in the UI so it shows the actual configured/resolved shell.
4. Keep thread runtime parking behavior unchanged.

Deliverable:

- the visible AI terminal uses the same shell and startup environment as the compatibility layer.

### Phase 7: Add Optional App-Level Login Environment Hydration

Files:

- `crates/hunk-desktop/src/main.rs`
- possibly a new helper module near app startup

Tasks:

1. On Unix, if Hunk is launched from a GUI context, optionally load login-shell environment into the app process before the UI starts.
2. Keep this behind config/default policy so it can be disabled.
3. Avoid polluting `SHLVL`.
4. Reuse the same shell capture helper from Phase 4.

Reference from Zed:

- `/tmp/zed/crates/zed/src/main.rs`
- `/tmp/zed/crates/util/src/util.rs`

Recommendation:

- do this after shell capture exists and only if Phase 6 still leaves too many GUI-launch gaps on macOS/Linux
- do not block Windows V1 on app-level process hydration
- treat this as a Unix-focused enhancement rather than the primary Windows compatibility mechanism

Deliverable:

- Finder-launched macOS builds get much closer to terminal-launched behavior.

### Phase 8: Windows Defaults And Compatibility Pass

Files:

- shell resolution helper
- terminal controller tests

Tasks:

1. Make `pwsh` the default preferred shell when installed.
2. Fall back to legacy PowerShell, then `cmd.exe`.
3. Verify visible shell behavior for:
   - `clear`
   - `ls`
   - simple aliases/functions if using PowerShell profiles later
4. Do not enable PowerShell profile loading for non-terminal internal command execution in this slice.

Deliverable:

- Windows users land in a more capable shell by default.

## Runtime Boundary

The compatibility work should distinguish between two kinds of process launch behavior.

### Visible Terminal

The visible AI terminal should aim for user-shell parity:

- configured shell selection
- shell startup behavior that loads the user's shell environment
- directory-aware environment capture
- Windows behavior that prefers `pwsh` over `cmd.exe`

This is the user-facing compatibility surface.

### Internal App Subprocesses

Internal app subprocesses should remain controlled in V1 unless explicitly opted into the new behavior.

Examples:

- Codex and AI worker/runtime processes in `crates/hunk-codex` and `crates/hunk-desktop/src/app/ai_runtime`
- Git-related subprocess paths and helpers
- any future app-managed subprocess that is not running inside the visible PTY terminal

Reason:

- visible terminals should feel like the user's machine
- internal app subprocesses should remain predictable and reproducible
- broken or slow shell startup files should not start breaking AI worker bootstrap, Git helpers, or unrelated runtime paths

Recommendation:

- implement the shell/env compatibility layer as a shared primitive
- consume it only from the visible AI terminal in V1
- keep internal subprocess behavior on the current controlled path unless a later change explicitly broadens the scope

### Phase 9: Documentation And User Messaging

Files:

- `docs/`
- release notes if needed

Tasks:

1. Document new terminal config keys.
2. Document fallback behavior when shell env capture fails.
3. Document known limitations:
   - shell startup latency
   - some shell frameworks may still behave differently
   - non-terminal subprocesses may not inherit the same behavior yet

Deliverable:

- users understand what the terminal compatibility layer does and how to disable it if needed.

## Testing Plan

### Unit Tests

- config parsing defaults and overrides
- shell resolution on Unix and Windows
- shell launch argument construction
- env capture parsing
- Windows PATH normalization

### Integration Tests

Recommended additions:

- `crates/hunk-terminal/tests`
  - spawn using explicit shell args and env
- `crates/hunk-desktop/tests`
  - controller tests that verify terminal launch request construction

Focus on testable invariants, not machine-specific shell content:

- correct shell is selected
- correct args are passed
- resolved env is merged into spawn request
- failure falls back to baseline shell launch path

### Manual QA Matrix

- macOS Finder launch with zsh + Homebrew/Nix PATH changes
- macOS terminal launch with zsh
- Linux desktop launch with bash/zsh
- Windows with `pwsh` installed
- Windows with only `cmd.exe`
- directories with project-local env side effects

## Failure And Fallback Strategy

If shell env capture fails:

1. log the failure
2. surface a lightweight terminal status message if useful
3. fall back to current terminal spawn behavior
4. do not block terminal launch

This fallback is important. Compatibility improvements should degrade safely to today's behavior.

## What We Can Directly Reuse From Zed

### Reuse Conceptually

- terminal shell config model
- startup login env hydration model
- per-directory env capture cache
- shell-specific launch behavior
- Windows shell discovery order

### Reuse Carefully

- shell env capture command construction
- shell detection logic
- Windows shell preference order

### Do Not Reuse Wholesale

- Zed project environment subsystem
- remote shell/environment layers
- task terminal integration
- direnv direct integration

## Recommended Implementation Order

This is the recommended execution order for the actual code work:

1. Add config types in `hunk-domain`.
2. Extend `TerminalSpawnRequest` to accept shell args and env.
3. Add shell resolution helpers.
4. Add shell env capture helper.
5. Cache env resolution in the AI terminal controller layer.
6. Update AI terminal launch path to use resolved shell + env.
7. Add Windows shell preference improvements.
8. Add optional app-level Unix login env hydration.
9. Add docs and polish.

## Concrete File Touch List For Hunk

Expected primary files:

- `crates/hunk-domain/src/config.rs`
- `crates/hunk-domain/tests/...`
- `crates/hunk-terminal/src/session.rs`
- `crates/hunk-terminal/src/lib.rs`
- `crates/hunk-terminal/src/...` new shell helper module
- `crates/hunk-terminal/tests/terminal_session.rs`
- `crates/hunk-desktop/src/app/controller/ai/terminal.rs`
- `crates/hunk-desktop/src/app/render/ai.rs`
- `crates/hunk-desktop/src/main.rs`

Potential supporting files:

- `crates/hunk-desktop/src/app/types.rs`
- `crates/hunk-desktop/src/app/controller/core_bootstrap.rs`
- `docs/`

## Locked Decisions

1. Windows is part of V1.
   - Windows support should ship in the first implementation, not as a later follow-up.
   - The initial Windows scope is explicit shell discovery, shell-aware environment capture, and safe fallback behavior.

2. Internal app subprocesses remain on controlled environment behavior in V1.
   - The visible AI terminal is the only product surface that should aim for user-shell parity in the first slice.
   - Internal worker/bootstrap/process-launch paths should stay deterministic unless explicitly expanded later.

3. The shell/env compatibility layer should be implemented as a shared primitive, but only consumed by the AI terminal in V1.
   - This keeps the implementation reusable without broadening product scope.

4. Shell args may exist in the runtime/config model, but V1 should keep the user-facing configuration simple.
   - The important user-facing controls are shell choice and login-environment inheritance policy.

## Recommendation

Implement the Zed-style architecture in a reduced Hunk scope:

- explicit shell config
- per-directory shell env capture
- deterministic Windows shell preference
- safe fallback to current behavior

Do not start with full Zed parity. Start with the visible AI terminal only, keep the system small, include Windows in V1, and leave app-wide Unix env hydration as a second pass once the shell capture path is proven stable.
