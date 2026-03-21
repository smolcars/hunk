# AI Terminal Support Plan

## Status

- Proposed
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

If the product later needs full interactive terminal behavior for TUIs and shell-native editing flows, Hunk can extend the drawer into a real terminal emulator without throwing away the first phase.

## Product Decisions (Locked)

1. Terminal support lives inside the AI workspace, not as a separate app-wide panel.
2. V1 is a native GPUI terminal drawer, not a webview.
3. V1 is PTY-backed on macOS, Linux, and Windows.
4. V1 optimizes for build/test/lint and command reruns, not for full TUI compatibility.
5. Terminal state is scoped to the AI workspace target and should persist alongside existing AI workspace UI state.
6. The first implementation should avoid copying large chunks of Zed terminal UI code. Reuse architecture and narrow upstream pieces where they are genuinely worth the integration cost.

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
- Optional VT engine if needed early: `wezterm-term`

Why this shape:

- `portable-pty` gives Hunk a clean cross-platform process and resize interface.
- A PTY-backed drawer is enough for the first workflow target.
- Hunk should avoid prematurely taking on the complexity of full VT rendering until the product proves it is needed.

### V1 Display Model

The first release should support two display tiers:

1. Plain streamed transcript mode
2. Minimal ANSI handling only if needed for legibility

That keeps the rendering surface simple and lets Hunk validate the workflow before committing to the engineering cost of a full emulator.

### Full VT Upgrade Path

If transcript mode proves insufficient, phase 2 can add:

- `wezterm-term` or another VT core for escape sequence parsing
- a cell-grid render surface in GPUI
- keyboard handling for interactive shell sessions
- scrollback and selection semantics closer to a real terminal

## UX Model

### Placement

The terminal should appear as a collapsible bottom drawer inside the existing AI center pane:

- thread sidebar on the left
- timeline + composer in the main pane
- terminal drawer below the timeline/composer stack

This mirrors how users already think about “chat plus command execution” and avoids introducing another top-level workspace concept.

### V1 Controls

- toggle terminal drawer
- command input box
- run button
- stop button while a process is active
- rerun last command
- clear terminal output
- quick actions for common commands
- cwd label with the current workspace target path

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

## Command Routing Rules

- New terminal sessions default to the current AI workspace cwd.
- If a thread is selected, terminal defaults should follow that thread's execution workspace.
- If a new-thread draft is active, terminal defaults should follow the draft workspace target.
- Switching AI workspaces must not kill hidden terminal sessions unless the user explicitly closes them.
- Background terminal output should continue flowing while another workspace is visible, following the same high-level rule as hidden AI runtimes.

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

This phase is conditional.

Decision question:

- does the product actually need a real terminal emulator, or is the command drawer enough?

If yes:

- add VT emulation core
- replace transcript rendering with cell-based terminal rendering
- add richer input and selection behavior

If no:

- keep the simpler drawer and invest in workflow polish instead

## Implementation Checklist

### Foundation

- [ ] Create `crates/hunk-terminal`.
- [ ] Define terminal session runtime API.
- [ ] Define terminal event types.
- [ ] Add bounded buffer policy.

### AI State

- [ ] Extend `AiWorkspaceState` with terminal fields.
- [ ] Capture and restore terminal state per workspace.
- [ ] Add helpers for active terminal cwd resolution.

### UI

- [ ] Add drawer open/close affordance.
- [ ] Add terminal toolbar and command entry.
- [ ] Add transcript output rendering.
- [ ] Add stop, rerun, and clear actions.
- [ ] Add drawer resize handling.

### AI Integration

- [ ] Add command-row actions into terminal drawer.
- [ ] Add “Open terminal in this worktree”.
- [ ] Add “Run again in terminal”.

### Validation

- [ ] Add targeted tests for terminal workspace state transitions.
- [ ] Add targeted tests for output truncation and session lifecycle.
- [ ] Run final workspace verification once after implementation:
- [ ] `cargo build --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

## Recommended Order

1. Land runtime foundation first.
2. Add workspace state integration second.
3. Add the drawer UI third.
4. Add timeline affordances fourth.
5. Decide on full VT emulation only after the drawer is proven useful.

## Expected Public Interface Changes

- New `hunk-terminal` crate with a small session runtime API.
- New terminal-related controller actions in `hunk-desktop`.
- New AI workspace state fields for terminal visibility and sessions.

## Open Questions

- Should terminal sessions persist only per visible app session, or across full app relaunch?
- Should one AI workspace support multiple terminal sessions in V1, or only one?
- Should terminal quick actions be user-configurable or hardcoded initially?
- Does Hunk need full ANSI color support immediately, or is transcript-first rendering sufficient?
- Should terminal output be searchable in V1, or can search remain a follow-up?
