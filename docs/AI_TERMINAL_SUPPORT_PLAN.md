# AI Terminal Support Plan

## Status

- In progress
- Owner: Hunk
- Last Updated: 2026-03-21

## Summary

This document defines the implementation plan for adding terminal support to the AI workspace.

The product goal is simple:

- let the user run commands from the AI view without switching to an external terminal
- make it fast to rerun build, test, lint, and repo-local commands after the agent finishes
- keep the implementation scalable and native to Hunk's GPUI architecture

The initial release should not aim to be a full editor-grade terminal emulator. The first milestone should be a PTY-backed command drawer that lives inside the AI workspace and supports the workflows that matter most for Hunk:

- run commands in the current AI workspace target
- stream output live
- stop and rerun commands quickly
- keep the command session visible next to the AI timeline

Hunk now has the first implementation slice:

- a native bottom terminal drawer in the AI workspace
- a PTY-backed runtime in `crates/hunk-terminal`
- command run, stop, rerun, clear, and per-workspace drawer state
- cross-platform bottom-panel toggle shortcut aligned with Zed and VS Code
  - macOS: `cmd-j`
  - Linux/Windows: `ctrl-j`
- live PTY input forwarding for already-running sessions
- transcript-style rendering with bounded output

That slice is useful, but it is not yet a real VT terminal surface. It does not yet behave like Zed or VS Code for:

- interactive shells
- TUIs such as `vim`, `less`, `top`, or `git add -p`
- true ANSI/VT screen state
- cursor-addressed redraws and alternate-screen behavior

The next phase should move Hunk from `PTY + transcript` to `PTY + VT emulator + cell renderer`.

## Product Decisions (Locked)

1. Terminal support lives inside the AI workspace, not as a separate app-wide panel.
2. V1 is a native GPUI terminal drawer, not a webview.
3. V1 is PTY-backed on macOS, Linux, and Windows.
4. V1 optimizes for build/test/lint and command reruns, not for full TUI compatibility.
5. Terminal state is scoped to the AI workspace target and should persist alongside existing AI workspace UI state.
6. The first implementation should avoid copying large chunks of Zed terminal UI code. Reuse architecture and narrow upstream pieces where they are genuinely worth the integration cost.
7. The next implementation target is a real VT-style terminal surface, not more transcript polish.
8. Since Hunk is now GPL-compatible for Zed-derived work, selective reuse of Zed terminal code is allowed where it materially reduces risk, but Hunk should still prefer small, understandable integrations over wholesale import.

## Goals

- Add a bottom terminal drawer to the AI workspace.
- Support running commands in the active AI workspace cwd.
- Preserve terminal visibility, height, and active session per AI workspace.
- Provide one-click rerun for recent commands and useful quick actions.
- Make it easy to jump from AI command execution output to a live terminal session.
- Keep the feature repo-agnostic and independent of Rust/Cargo-specific workflows.

## Non-Goals For V1

- Full terminal parity with Zed, WezTerm, or iTerm.
- Terminal splits or tabs.
- Alternate-screen TUI support guarantees.
- Shell integration features such as prompt marks or command decorations.
- Replacing the existing AI timeline command execution cards.
- Global terminal support outside the AI workspace.

## Current Status

Implemented today:

- `crates/hunk-terminal` exists and owns PTY spawning, process lifecycle, resize hooks, and output streaming.
- The AI workspace has a native bottom terminal drawer instead of a floating card.
- Terminal state is captured and restored with AI workspace state.
- Opening the drawer now starts an interactive shell session by default when a workspace is available.
- The drawer supports run, stop, rerun, and clear flows, while presenting the UI as a shell-first bottom pane instead of a boxed command form.
- The drawer supports live PTY input forwarding for running sessions.
- The drawer can be toggled with `cmd-j` on macOS and `ctrl-j` on Linux/Windows.
- Output is bounded and now rendered through a VT-backed GPUI cell surface with transcript fallback for empty and failure states.
- Live terminal sessions can now take direct keyboard input from the terminal surface, including bracketed paste when the shell requests it.
- VT scrollback now stays interactive after process exit, with mouse-wheel scrolling plus `Shift+PageUp/PageDown/Home/End` viewport controls.
- The VT surface now supports mouse text selection plus terminal-style copy shortcuts (`cmd-c` on macOS, `ctrl-shift-c` on Linux/Windows) when a terminal selection is active.
- Terminal key translation is now mode-aware for TUIs, including app-cursor arrow/home/end sequences and alternate-screen `Shift+PageUp/PageDown/Home/End` input.
- The terminal now reports focus in/out, mouse button presses, mouse motion, and wheel events back into the PTY using the active VT mouse protocol, including SGR/UTF-8 mouse formats and alternate-scroll mode.
- Shift now bypasses terminal mouse reporting so scrollback selection still works outside alternate-screen TUIs, while the terminal owns wheel events whenever alternate-screen content is active.
- The VT surface is now painted as a terminal grid with cell backgrounds and cursor-aware text runs, instead of being rendered as ordinary styled text rows.
- Cursor rendering now respects terminal focus state, using a beam caret for focused beam cursors and hollow outlines for unfocused block-style cursors.
- Opening the terminal now defers focus into the terminal surface, and hiding it restores focus to the AI composer.
- The terminal pane header was reduced to slimmer editor-style chrome and the standalone stop control was removed.
- The terminal pane is now vertically resizable from a drag handle along its top border.
- The VT surface now resizes the PTY and terminal grid to the actual rendered panel size, which restores correct prompt placement, auto-follow behavior, and scroll reachability for long output.
- Command execution rows in the AI timeline can now reopen the terminal and rerun that command directly inside the interactive shell pane.
- The fallback command-launcher input has been removed; the AI terminal is now shell-first and `exit` closes the terminal session instead of dropping into a second text box.
- Terminal runtimes are now parked by AI thread instead of being killed on thread/workspace switches, so switching between worktree threads restores that thread's live shell.
- Each AI thread now owns its own terminal session state instead of sharing a single terminal bucket per workspace.
- Shell exit now tears down the old PTY runtime immediately, so reopening the terminal always starts a fresh shell instead of getting stuck on a stale "Starting shell..." state.
- Hidden terminal runtimes and saved per-thread terminal state are now pruned when archived or deleted threads disappear from the visible or background AI workspace model.
- The VT surface now detects URL and file-style terminal spans, underlines them, shows a link cursor on hover, and reuses the existing Hunk link-opening path on click.
- The terminal cursor now blinks while the terminal surface is focused, using UI-side timer state that follows thread switches, shell startup, shell exit, and focus/open changes without affecting PTY behavior.
- Workspace-wide validation already passes for the current slice.

Not implemented yet:

- persisted terminal state across full app relaunch

## Current Integration Points

The existing codebase already has the right seams for this feature.

- The AI workspace layout is assembled in `crates/hunk-desktop/src/app/render/ai.rs` and `crates/hunk-desktop/src/app/render/ai_workspace_sections.rs`.
- AI per-workspace UI/runtime state already exists in `crates/hunk-desktop/src/app/types.rs`.
- AI workspace state capture and restore already happens in `crates/hunk-desktop/src/app/controller/ai/core_workspace.rs`.
- Codex command executions are already represented as timeline items in `crates/hunk-codex/src/threads/helpers.rs`.
- Command output deltas already stream into state through `crates/hunk-codex/src/threads/notifications.rs`.
- The AI timeline already renders command output in a terminal-like transcript style in `crates/hunk-desktop/src/app/render/ai_helpers/timeline_rows.rs`.

This means terminal support should be modeled as a sibling to the existing AI timeline and composer, not as a new standalone subsystem with unrelated persistence and rendering rules.

## Architecture Boundary

### New Crate

- `crates/hunk-terminal`

Responsibilities:

- cross-platform PTY creation
- shell and command spawning
- stdin writes
- terminal resize
- stdout/stderr streaming
- process lifecycle and exit status
- bounded scrollback buffering

Non-responsibilities in V1:

- app-specific UI rendering
- timeline integration logic
- command history persistence policy
- shell completion or editor integration

### Existing Crates

- `crates/hunk-desktop`
  - owns GPUI drawer UI, view state, actions, focus, keyboard handling, and persistence
- `crates/hunk-codex`
  - remains responsible for Codex thread state and command execution timeline items
- `crates/hunk-domain`
  - may need persisted app-state additions if terminal preferences should survive app relaunch

## Runtime Strategy

### Recommended Stack

- PTY backend: `portable-pty`
- VT engine for the next phase: `alacritty_terminal`

Why this shape:

- `portable-pty` gives Hunk a clean cross-platform process and resize interface.
- A PTY-backed drawer was enough for the first workflow target.
- Zed already proves the `terminal runtime + terminal view` split with `alacritty_terminal`.
- The next step should now absorb the complexity of full VT rendering rather than polishing the transcript path further.

### Current Display Model

The current implementation is:

1. PTY-backed transcript mode
2. minimal ANSI stripping for readability

This was the correct first slice, but it is not the final architecture.

### VT Upgrade Target

The target architecture for the next phase is:

1. `portable-pty` for process hosting
2. `alacritty_terminal` for terminal state and VT parsing
3. a GPUI terminal surface that renders terminal cells, cursor, and selection from the VT state
4. keyboard and paste handling that writes directly to the PTY

### Terminal Ownership Model

The terminal model is now:

1. one live PTY-backed terminal per AI thread
2. thread switches park the previous runtime and promote the newly selected thread's runtime
3. workspace/worktree switches use the same park/promote behavior instead of killing the old terminal
4. shell exit closes that thread's terminal session

This is intentionally close to editor-style behavior while remaining cross-platform:

- macOS/Linux continue to use the configured `$SHELL` or `/bin/bash`
- Windows continues to use `%COMSPEC%` or `cmd.exe`
- all shells still run behind `portable-pty`, so the runtime model is the same across platforms

## UX Model

### Placement

The terminal should appear as a collapsible bottom drawer inside the existing AI center pane:

- thread sidebar on the left
- timeline + composer in the main pane
- terminal drawer below the timeline/composer stack

This mirrors how users already think about “chat plus command execution” and avoids introducing another top-level workspace concept.

### V1 Controls

- toggle terminal drawer
- shell-first terminal surface
- drawer hide action
- rerun last command
- clear terminal output
- cwd label with the current workspace target path
- drag-resize for the terminal drawer height

### Quick Action Seeds

The app should support configurable or hardcoded quick actions for:

- build
- test
- lint
- repo-specific task runners where already known

The first cut can start with generic command entry and one “rerun last command” action. Preset command chips can land shortly after.

## State Model

Terminal state should live alongside the existing AI workspace state so it follows the same workspace-keyed restore behavior.

Suggested additions to `AiWorkspaceState` in `crates/hunk-desktop/src/app/types.rs`:

- `terminal_open: bool`
- `terminal_height_px: f32`
- `terminal_follow_output: bool`
- `active_terminal_session_id: Option<String>`
- `terminal_sessions: BTreeMap<String, AiTerminalSessionState>`

Suggested `AiTerminalSessionState` fields:

- `id: String`
- `cwd: PathBuf`
- `title: String`
- `last_command: Option<String>`
- `status: AiTerminalSessionStatus`
- `buffer: Arc<String>` or a more structured terminal buffer model
- `exit_code: Option<i32>`
- `started_at: Option<Instant>`
- `completed_at: Option<Instant>`

If app-level persistence is needed across launches, terminal preferences should be persisted through `hunk-domain` the same way other workspace preferences are persisted today.

Current implementation note:

- Hunk currently stores one terminal session state per visible AI workspace, not multiple sessions or tabs.
- The current `AiTerminalSessionState` is transcript-oriented and will need to evolve into a VT-oriented model with:
  - terminal size
  - scrollback buffer
  - viewport offset
  - selection state
  - cursor state
  - title / mode flags

## Event Model

`hunk-terminal` should expose a small runtime event stream:

- `Started`
- `Output { text }`
- `Exit { code }`
- `FailedToStart { message }`
- `Resized { cols, rows }`

Desktop actions should remain narrow and explicit:

- `ai_toggle_terminal_drawer_action`
- `ai_run_terminal_command_action`
- `ai_stop_terminal_session_action`
- `ai_rerun_terminal_command_action`
- `ai_clear_terminal_session_action`
- `ai_select_terminal_session_action`

The VT upgrade will need additional actions:

- `ai_terminal_write_input_action`
- `ai_terminal_paste_action`
- `ai_terminal_copy_selection_action`
- `ai_terminal_page_up_action`
- `ai_terminal_page_down_action`
- `ai_terminal_scroll_action`

## Command Routing Rules

- New terminal sessions default to the current AI workspace cwd.
- If a thread is selected, terminal defaults should follow that thread's execution workspace.
- If a new-thread draft is active, terminal defaults should follow the draft workspace target.
- Switching AI workspaces must not kill hidden terminal sessions unless the user explicitly closes them.
- Background terminal output should continue flowing while another workspace is visible, following the same high-level rule as hidden AI runtimes.

Current implementation note:

- Hunk does not satisfy this yet. The current implementation stops the running terminal session when the visible AI workspace changes.
- Keeping background terminal sessions alive should be treated as a follow-up after the VT surface lands or alongside that work if the state model is already being refactored.

## Relationship To Existing AI Command Execution

The terminal drawer does not replace Codex command execution items.

Instead:

- command execution timeline items remain the record of what the agent did
- the live terminal drawer is for user-initiated manual commands
- timeline command rows can later gain actions such as:
  - `Run Again In Terminal`
  - `Open Terminal Here`
  - `Copy Command`

This separation keeps agent actions auditable while still giving the user a fast manual command loop.

## Performance Requirements

- Opening the AI workspace should not eagerly initialize heavy terminal infrastructure if the drawer is closed.
- PTY sessions should be created lazily.
- Hidden sessions must not cause render churn in the active workspace.
- Scrollback storage must be bounded.
- The drawer must preserve the “fast diff viewer” feel of the AI workspace. Any terminal implementation that introduces noticeable jank in timeline scrolling or composer interaction is incorrect.

## Risks

### Risk 1: Full Terminal Scope Explosion

Terminal emulation looks deceptively small but becomes expensive quickly once shell editing, ANSI handling, alternate screen, selection, hyperlinks, and mouse support are all required.

Mitigation:

- keep V1 to a command drawer
- gate full VT emulation behind a later decision

### Risk 2: State Coupling With AI Workspace

The AI workspace already has substantial state. Adding terminal state carelessly could make capture/restore logic fragile.

Mitigation:

- add a dedicated terminal state struct
- keep capture/restore explicit and test-covered

### Risk 3: Cross-Platform PTY Differences

Windows process and pseudoconsole behavior will not match Unix exactly.

Mitigation:

- isolate PTY concerns inside `hunk-terminal`
- keep the desktop layer platform-agnostic

### Risk 4: Buffer Growth And Render Cost

Unbounded output can degrade memory use and rendering performance.

Mitigation:

- cap scrollback by bytes or lines
- support trimming old output
- avoid rebuilding large strings unnecessarily in hot paths

## Phase Plan

### Phase 1: Runtime Foundation

Files:

- new `crates/hunk-terminal`
- `Cargo.toml`

Changes:

- add a small terminal runtime crate
- define PTY session abstractions and event types
- support spawn, stdin write, resize, kill, and output stream
- define bounded output buffering behavior

Exit criteria:

- crate-level tests cover spawn, output collection, exit status, and buffer truncation logic

### Phase 2: AI Workspace State Integration

Files:

- `crates/hunk-desktop/src/app/types.rs`
- `crates/hunk-desktop/src/app/controller/ai/core_workspace.rs`
- `crates/hunk-desktop/src/app/controller/ai/runtime.rs`

Changes:

- add terminal drawer state to `AiWorkspaceState`
- capture and restore terminal state per AI workspace
- define controller helpers for resolving terminal cwd from current workspace context

Exit criteria:

- workspace switching preserves terminal openness, active session, and selected cwd correctly

### Phase 3: Drawer UI

Files:

- `crates/hunk-desktop/src/app/render/ai.rs`
- `crates/hunk-desktop/src/app/render/ai_workspace_sections.rs`
- new `crates/hunk-desktop/src/app/render/ai_helpers/terminal_panel.rs`

Changes:

- add bottom drawer UI
- add terminal toolbar and command input controls
- add transcript output rendering
- add follow-output behavior and resizing

Exit criteria:

- the user can open the drawer, run a command, watch output, stop it, rerun it, and clear it

### Phase 4: AI Command Affordances

Files:

- `crates/hunk-desktop/src/app/render/ai_helpers/timeline_rows.rs`
- `crates/hunk-desktop/src/app/controller/ai/*`

Changes:

- add action buttons from command execution rows into the terminal drawer
- enable `Run Again In Terminal`
- enable `Open Terminal Here`
- optionally seed quick actions from recent command executions

Exit criteria:

- the AI timeline and terminal drawer feel connected rather than duplicated

### Phase 5: Persistence, Polish, And Validation

Files:

- `crates/hunk-domain/src/state.rs`
- `crates/hunk-domain/tests/*`
- `crates/hunk-desktop/src/app/controller/ai/tests/*`

Changes:

- persist terminal preferences if desired across app relaunch
- refine labels, empty states, and disabled states
- ensure keyboard focus rules are predictable
- validate background session behavior across workspace switches

Exit criteria:

- feature survives normal navigation and relaunch flows cleanly

### Phase 6: Full VT Decision

This phase is no longer conditional.

Decision:

- Hunk should implement a real VT-style terminal surface.

### Phase 6: VT Runtime Model

Files:

- `crates/hunk-terminal/src/lib.rs`
- new `crates/hunk-terminal/src/vt.rs`
- new `crates/hunk-terminal/src/session.rs`

Changes:

- integrate `alacritty_terminal`
- replace transcript-only runtime state with a VT session model
- track rows, cols, cursor state, alternate-screen mode, and scrollback
- expose structured screen-dirty events instead of only raw text chunks
- keep PTY input writing and VT parsing behind the same `hunk-terminal` crate boundary

Exit criteria:

- runtime can accept PTY output and maintain a correct VT screen model for shell redraws and cursor movement

### Phase 7: GPUI Terminal Surface

Files:

- `crates/hunk-desktop/src/app/render/ai_helpers/terminal_panel.rs`
- new `crates/hunk-desktop/src/app/render/ai_helpers/terminal_surface.rs`

Changes:

- replace transcript rendering with a cell-grid render surface
- render glyphs, colors, cursor, and selection from VT state
- support viewport scrolling over scrollback
- keep the terminal as a real bottom pane in the AI layout
- preserve the `cmd-j` / `ctrl-j` bottom-panel behavior while focus moves into the terminal surface

Exit criteria:

- shell prompts redraw correctly and line editing looks stable

### Phase 8: Input, Clipboard, And Focus

Files:

- `crates/hunk-desktop/src/app/controller/ai/terminal.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/terminal_panel.rs`

Changes:

- write keyboard input directly to the PTY
- support paste and copy
- route focus cleanly between timeline, composer, and terminal
- support resize propagation from the drawer height and terminal surface width

Exit criteria:

- user can interact with a live shell inside Hunk without falling back to the transcript command box model

### Phase 9: TUI Compatibility And Polish

Files:

- `crates/hunk-terminal/*`
- `crates/hunk-desktop/src/app/controller/ai/*`
- `crates/hunk-desktop/src/app/render/ai_helpers/*`

Changes:

- verify alternate-screen handling
- add terminal search and selection polish
- add timeline affordances such as `Run Again In Terminal`
- optionally add session persistence or multi-session support

Exit criteria:

- common TUIs and shell workflows behave predictably enough that the terminal feels editor-grade

## Implementation Checklist

### Foundation

- [x] Create `crates/hunk-terminal`.
- [x] Define terminal session runtime API.
- [x] Define terminal event types.
- [x] Add bounded buffer policy.
- [x] Add VT engine integration.
- [x] Replace transcript-only session state with VT state.

### AI State

- [x] Extend `AiWorkspaceState` with terminal fields.
- [x] Capture and restore terminal state per workspace.
- [x] Add helpers for active terminal cwd resolution.
- [ ] Keep hidden terminal sessions alive across AI workspace switches.
- [ ] Persist terminal preferences across full app relaunch if desired.

### UI

- [x] Add drawer open/close affordance.
- [x] Add terminal toolbar and command entry.
- [x] Add transcript output rendering.
- [x] Add stop, rerun, and clear actions.
- [ ] Add drawer height controls.
- [x] Replace transcript rendering with a VT cell surface.
- [x] Add terminal keyboard input routing into the live PTY session.
- [x] Replace command-line style input routing with terminal-surface keystroke routing.
- [x] Add terminal text selection and copy behavior.
- [x] Add proper terminal scrolling and viewport behavior.

### AI Integration

- [ ] Add command-row actions into terminal drawer.
- [ ] Add “Open terminal in this worktree”.
- [ ] Add “Run again in terminal”.

### Validation

- [ ] Add targeted tests for terminal workspace state transitions.
- [ ] Add targeted tests for output truncation and session lifecycle.
- [x] Run final workspace verification once for the current transcript slice:
- [x] `cargo build --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [ ] Re-run final workspace verification after the VT surface lands.

## Recommended Order

1. Keep the current PTY drawer as the outer shell.
2. Land VT runtime integration inside `hunk-terminal`.
3. Replace transcript rendering with a GPUI terminal surface.
4. Add keyboard, paste, selection, and scrollback behavior.
5. Add timeline affordances and optional multi-session polish afterward.

## Expected Public Interface Changes

- New `hunk-terminal` crate with a small session runtime API.
- New terminal-related controller actions in `hunk-desktop`.
- New AI workspace state fields for terminal visibility and sessions.
- Later: a VT-backed terminal model and GPUI cell renderer.

## Open Questions

- Should terminal sessions persist only per visible app session, or across full app relaunch?
- Should one AI workspace support multiple terminal sessions in V1, or only one?
- Should terminal quick actions be user-configurable or hardcoded initially?
- Does Hunk need full ANSI color support immediately, or is transcript-first rendering sufficient?
- Should terminal output be searchable in V1, or can search remain a follow-up?
