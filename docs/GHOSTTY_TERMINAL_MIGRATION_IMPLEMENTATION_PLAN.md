# Ghostty Terminal Migration Implementation Plan

## Status

- Proposed
- Owner: Hunk
- Last Updated: 2026-03-28

## Summary

This document defines the implementation plan for migrating Hunk's terminal backend from:

- `portable-pty` + `alacritty_terminal` + Hunk-owned terminal protocol glue

to:

- `portable-pty` + `libghostty-vt` + a smaller Hunk adapter layer

This is a breaking backend migration. It should be executed in phases, but the end state is a real backend replacement, not a long-term dual-maintenance setup.

The migration is worth doing if the goal is to stop owning terminal protocol details in Hunk. The major payoff is not "Ghostty look-and-feel for free." The payoff is:

- Ghostty-owned VT parsing
- Ghostty-owned key encoding
- Ghostty-owned mouse encoding from the live terminal state
- better terminal metadata and damage tracking
- less Hunk-specific protocol code to debug over time

## Decision

Proceed as a clean breaking migration on a dedicated branch.

Do not add:

- a runtime backend toggle
- a config flag
- a long-term Cargo feature for backend selection

If the migration branch fails, discard the branch. If it succeeds, merge the branch and remove the old backend as part of the migration.

Do not attempt a big-bang rewrite that changes:

- PTY hosting
- terminal rendering
- input encoding
- shell compatibility
- AI terminal state management
- Files terminal state management

all at once.

The migration should preserve Hunk's existing GPUI surface and state model first, then replace backend pieces underneath it, then cut over, then delete the old code.

## Relationship To Existing Docs

This plan complements:

- `docs/AI_TERMINAL_SUPPORT_PLAN.md`
- `docs/TERMINAL_SHELL_COMPATIBILITY_IMPLEMENTATION_PLAN.md`

This document supersedes the VT backend direction in `docs/AI_TERMINAL_SUPPORT_PLAN.md`, which currently names `alacritty_terminal` as the next-phase VT engine.

## Why This Migration Is Different

This is not a drop-in library swap.

The current terminal stack is coupled across:

- `crates/hunk-terminal/src/session.rs`
- `crates/hunk-terminal/src/vt.rs`
- `crates/hunk-desktop/src/app/controller/ai/terminal_protocol.rs`
- `crates/hunk-desktop/src/app/render/ai_helpers/terminal_surface.rs`
- `crates/hunk-desktop/src/app/controller/ai/terminal.rs`
- `crates/hunk-desktop/src/app/controller/file_terminal.rs`

The migration must account for these facts:

1. `portable-pty` is still needed.
2. `libghostty-vt` is the terminal core, not the PTY host.
3. Hunk still owns the GPUI renderer and interaction model.
4. `libghostty-vt` is `!Send + !Sync`, so the ownership model must change to a single-owner terminal actor.
5. Hunk currently duplicates terminal runtime orchestration between AI and Files views and should keep that duplication stable during the backend swap instead of refactoring everything at once.

## Goals

- Replace `alacritty_terminal` with `libghostty-vt`.
- Replace Hunk-owned terminal input/protocol encoding with Ghostty-backed encoding.
- Keep `portable-pty` as the cross-platform PTY host.
- Preserve the current GPUI terminal surface long enough to reduce migration risk.
- Keep AI and Files terminal behavior aligned during the migration.
- Validate macOS, Linux, and Windows before cutover.

## Non-Goals

- A visual redesign of the terminal UI.
- Terminal tabs, splits, or shell decorations.
- A rewrite of AI terminal persistence or Files terminal persistence.
- Deduplicating all AI and Files terminal controller code during the same change.
- Replacing `portable-pty`.
- Depending on unstable GUI-facing `libghostty` APIs.

## Current Baseline

### Current Hunk Backend

- `crates/hunk-terminal/Cargo.toml` depends on `alacritty_terminal` and `portable-pty`.
- `crates/hunk-terminal/src/session.rs` owns PTY spawn, child lifecycle, terminal event threads, and VT mutation.
- `crates/hunk-terminal/src/vt.rs` owns the Alacritty-backed screen model and snapshot conversion.
- `crates/hunk-desktop/src/app/controller/ai/terminal_protocol.rs` owns key, paste, focus, and mouse reporting logic.
- `crates/hunk-desktop/src/app/render/ai_helpers/terminal_surface.rs` paints Hunk snapshots into GPUI.

### Current Hunk State Model

The visible Hunk terminal state already has a backend-agnostic shape:

- transcript
- optional `TerminalScreenSnapshot`
- session status
- exit code
- pending input
- open/follow state

That is the right seam to preserve first.

### Current External Validation

The working fork for the migration is:

- `https://github.com/niteshbalusu11/libghostty-rs`

The current fork already has:

- Windows target handling in the Rust binding
- a Windows GitHub Actions job proving `cargo check -p libghostty-vt --target x86_64-pc-windows-msvc`

Hunk is currently pinned to fork commit:

- `2d0716f1c96e1406957c46f82dc6c6b53379b489`

That removes the largest immediate blocker to starting the Hunk rewrite on top of the fork.

## Target Architecture

### Hunk Terminal Crate Shape

The target `hunk-terminal` crate should be split into small focused modules before or during the migration:

- `src/lib.rs`
- `src/runtime.rs`
- `src/pty.rs`
- `src/snapshot.rs`
- `src/backend/mod.rs`
- `src/backend/alacritty.rs`
- `src/backend/ghostty.rs`
- `src/input.rs`

This keeps files below the repo limit and makes the cutover easier.

### Ownership Model

The terminal runtime should become:

1. one actor thread per live Hunk terminal runtime
2. that actor owns:
   - the PTY reader/writer
   - the Ghostty terminal instance
   - the Ghostty render state
3. the UI sends commands over channels:
   - resize
   - write input
   - scroll viewport
   - clear selection if needed later
4. the actor sends immutable snapshots/events back to the UI

This matches Ghostty's `!Send + !Sync` requirement and removes the current shared `Arc<Mutex<TerminalVt>>` model.

### Migration Rule

For the first cut, keep Hunk's public snapshot/event types stable:

- `TerminalEvent`
- `TerminalScreenSnapshot`
- `TerminalModeSnapshot`
- `TerminalCursorSnapshot`
- `TerminalCellSnapshot`

Ghostty should initially feed those types through an adapter. Only after the cutover should we decide whether to expose Ghostty-specific shapes directly.

### Input Encoding Rule

The migration should separate:

- GPUI event interpretation owned by `hunk-desktop`
- terminal protocol byte generation owned by `hunk-terminal`

That split is especially important for mouse input.

Keyboard, paste, and focus reporting can be modeled as largely stateless input helpers in `hunk-terminal`.

Mouse reporting should not be treated the same way. The final migration target is:

- desktop converts pointer coordinates into terminal grid positions
- desktop sends semantic mouse events to `hunk-terminal`
- the terminal actor encodes mouse bytes against the live Ghostty terminal instance

Do not treat `TerminalModeSnapshot` as the long-term source of truth for mouse protocol encoding. It is useful for rendering and temporary migration scaffolding, but it is too lossy to be the final authority for:

- mouse format selection
- tracking mode selection
- any future Ghostty mouse features such as pixel reporting

Temporary snapshot-driven helpers are acceptable only to keep the branch compiling while the actor-side input path is introduced.

## Phase 0: Fork Hardening

### Deliverable

A forked `libghostty-rs` that Hunk can depend on during the rewrite.

### TODO

- [ ] Create a dedicated long-lived branch in the fork for Hunk integration work.
- [ ] Keep the Windows target handling patch and the Windows MSVC GitHub Actions workflow.
- [ ] Add macOS and Linux CI jobs to the fork so all three Hunk platforms are exercised.
- [ ] Add `cargo test -p libghostty-vt-sys` to the fork CI where practical.
- [ ] Decide whether Hunk will depend on:
  - the fork branch directly, or
  - a pinned fork commit
- [ ] Document the exact fork commit used by Hunk in the Hunk migration branch.
- [ ] Upstream the Windows target support later, but do not block Hunk migration on upstream review.

### Exit Criteria

- The fork passes CI on macOS, Linux, and Windows MSVC.
- Hunk can pin the fork without depending on local patches in `/tmp`.

## Phase 1: Prepare Hunk For Backend Swapping

### Deliverable

`hunk-terminal` can host a Ghostty migration path without changing its public API yet.

### TODO

- [ ] Refactor `crates/hunk-terminal/src/session.rs` into smaller modules without changing behavior.
- [ ] Move snapshot types out of the current Alacritty-specific file into `src/snapshot.rs`.
- [ ] Introduce a backend-agnostic terminal engine interface inside `crates/hunk-terminal`.
- [ ] Keep the current Alacritty-backed code only as temporary migration scaffolding inside the branch.
- [ ] Move PTY hosting code into its own module so the backend swap does not touch PTY concerns.
- [ ] Keep the public `spawn_terminal_session` API stable so `hunk-desktop` does not need to change yet.

### Files To Touch

- `crates/hunk-terminal/Cargo.toml`
- `crates/hunk-terminal/src/lib.rs`
- `crates/hunk-terminal/src/session.rs`
- new files under `crates/hunk-terminal/src/`

### Exit Criteria

- Existing Hunk behavior is unchanged.
- `hunk-terminal` has a clean seam where a Ghostty engine can be added.

## Phase 2: Introduce The Ghostty Backend

### Deliverable

A Ghostty-backed engine exists in `hunk-terminal` and can replace the old engine in-place once snapshot parity is ready.

### TODO

- [ ] Add the forked `libghostty-vt` dependency to `crates/hunk-terminal/Cargo.toml`.
- [ ] Add a backend module such as `src/backend/ghostty.rs`.
- [ ] Implement Ghostty terminal construction, resize, scroll, and byte ingestion.
- [ ] Build a snapshot adapter from Ghostty render/state APIs into Hunk's:
  - `TerminalScreenSnapshot`
  - `TerminalCellSnapshot`
  - `TerminalDamageSnapshot`
  - `TerminalCursorSnapshot`
  - `TerminalModeSnapshot`
- [ ] Support the same scrollback semantics Hunk currently exposes to the GPUI layer.
- [ ] Support transcript accumulation for fallback/error states exactly as today.
- [ ] Keep the existing Alacritty-backed code compiling only until the Ghostty adapter is complete.

### Files To Touch

- `crates/hunk-terminal/Cargo.toml`
- `crates/hunk-terminal/src/backend/ghostty.rs`
- `crates/hunk-terminal/src/snapshot.rs`
- `crates/hunk-terminal/src/runtime.rs`

### Exit Criteria

- A hidden Ghostty backend can parse PTY output and produce Hunk snapshots.
- The AI and Files views still compile without behavior changes.

## Phase 3: Replace The Ownership Model

### Deliverable

The runtime uses a single-owner actor that is valid for Ghostty's threading rules.

### TODO

- [ ] Replace the current shared VT state pattern with a terminal actor thread.
- [ ] Keep the current event surface:
  - `Output`
  - `Screen`
  - `Exit`
  - `Failed`
- [ ] Move all Ghostty access inside the actor thread.
- [ ] Ensure PTY reads, PTY writes, query responses, and resize events all stay on the actor side.
- [ ] Preserve runtime generation and parking behavior used by:
  - AI terminals by thread
  - Files terminals by project
- [ ] Keep hidden runtime parking/promoting behavior intact during workspace and thread switches.

### Files To Touch

- `crates/hunk-terminal/src/runtime.rs`
- `crates/hunk-terminal/src/pty.rs`
- `crates/hunk-desktop/src/app/controller/ai/terminal.rs`
- `crates/hunk-desktop/src/app/controller/file_terminal.rs`

### Exit Criteria

- The Ghostty backend runs without violating `!Send + !Sync`.
- Thread switching and project switching still park and restore terminals correctly.

## Phase 4: Move Input Encoding To Ghostty

### Deliverable

Ghostty-backed input handling owns key, mouse, paste, and focus encoding, with mouse reports generated inside the terminal actor from the live Ghostty terminal.

### TODO

- [ ] Add an input encoding layer in `hunk-terminal`, likely in `src/input.rs`.
- [ ] Replace the custom key/mouse/focus protocol code in `crates/hunk-desktop/src/app/controller/ai/terminal_protocol.rs`.
- [ ] Keep the GPUI event-to-grid-point conversion in `hunk-desktop`.
- [ ] Introduce a semantic terminal input command path from `hunk-desktop` into `hunk-terminal` so desktop no longer sends pre-encoded protocol bytes for interactive events.
- [ ] Move terminal protocol byte generation into `hunk-terminal` so `hunk-desktop` no longer knows terminal protocol details.
- [ ] Encode mouse input inside the terminal actor using Ghostty's mouse API against the live terminal instance.
- [ ] Do not use `TerminalModeSnapshot` as the final source of truth for mouse protocol encoding decisions.
- [ ] Keep snapshot-driven keyboard, paste, and focus helpers only where they remain stateless and correct.
- [ ] Support at minimum:
  - plain text input
  - bracketed paste
  - focus in/out
  - arrow/home/end/page keys
  - app-cursor behavior
  - mouse press/release/move
  - wheel input
  - alternate scroll behavior
- [ ] Preserve current selection bypass behavior when `Shift` is used.
- [ ] Delete temporary snapshot-driven mouse helpers once actor-side mouse encoding is live.

### Files To Touch

- `crates/hunk-terminal/src/input.rs`
- `crates/hunk-terminal/src/lib.rs`
- `crates/hunk-desktop/src/app/controller/ai/terminal_protocol.rs`
- any Files-terminal event wiring that depends on shared helpers

### Exit Criteria

- Hunk no longer owns terminal protocol encoding logic in `hunk-desktop`.
- Mouse protocol bytes are no longer reconstructed from `TerminalModeSnapshot`.
- Common TUIs behave at least as well as the current implementation.

## Phase 5: Switch Hunk To Ghostty In-Branch

### Deliverable

Hunk runs end-to-end on the Ghostty backend in the migration branch.

### TODO

- [ ] Switch the AI terminal to the Ghostty backend.
- [ ] Switch the Files terminal to the Ghostty backend.
- [ ] Verify that `terminal_surface.rs` does not need semantic changes beyond snapshot compatibility.
- [ ] Keep link detection, selection overlays, cursor blinking, and focus restoration behavior unchanged from the user's perspective.
- [ ] Reconcile any backend differences in:
  - damage handling
  - display offset semantics
  - cursor shape mapping
  - color mapping
  - zero-width / wide character behavior

### Files To Touch

- `crates/hunk-desktop/src/app/render/ai_helpers/terminal_surface.rs`
- `crates/hunk-desktop/src/app/render/ai.rs`
- `crates/hunk-desktop/src/app/render/file_editor.rs`
- `crates/hunk-desktop/src/app/types.rs`

### Exit Criteria

- Hunk runs on the Ghostty backend in the migration branch.
- The UI does not regress in selection, focus, cursor, or scroll behavior.

## Phase 6: Parity And Validation Pass

### Deliverable

A clear go/no-go decision for merging the migration branch.

### TODO

- [ ] Validate shell startup on macOS, Linux, and Windows using the shell-compatibility plan.
- [ ] Validate normal shell usage:
  - prompt rendering
  - multiline editing
  - command history
  - resize behavior
- [ ] Validate TUI cases:
  - `vim`
  - `less`
  - `top` or equivalent
  - `git add -p`
  - `fzf`
  - `tmux`
- [ ] Validate text selection and copy behavior in normal screen and alternate screen.
- [ ] Validate link detection and click handling.
- [ ] Validate parked runtime restoration for:
  - AI thread switches
  - Files project switches
- [ ] Validate Windows shell behavior with `cmd.exe`, PowerShell, and `pwsh` if installed.
- [ ] Measure render cost and confirm the terminal surface still meets the 8ms frame budget.
- [ ] Add crate-level tests in `crates/hunk-terminal/tests` for:
  - snapshot translation
  - color/cursor mapping
  - input encoding
  - resize semantics

### Exit Criteria

- No blocker remains on the supported desktop platforms.
- The Ghostty backend matches or exceeds current behavior in the terminal scenarios Hunk cares about.

## Phase 7: Remove The Old Backend And Finalize

### Deliverable

Ghostty is the only terminal VT backend in Hunk.

### TODO

- [ ] Remove the Alacritty backend implementation.
- [ ] Remove the `alacritty_terminal` dependency from `crates/hunk-terminal/Cargo.toml`.
- [ ] Remove obsolete translation code and dead compatibility helpers.
- [ ] Update docs that still mention `alacritty_terminal` as Hunk's VT engine.
- [ ] Keep `portable-pty` in place.

### Files To Delete Or Simplify

- `crates/hunk-terminal/src/vt.rs` in its current form
- Alacritty-specific backend code introduced during the transition
- any dead code in `crates/hunk-desktop/src/app/controller/ai/terminal_protocol.rs`

### Exit Criteria

- Hunk's terminal backend is Ghostty-based only.
- No user-facing terminal behavior depends on Alacritty-specific code anymore.

## Risk Register

### Risk 1: Ghostty snapshot semantics do not line up with Hunk's current render assumptions

Mitigation:

- Keep the initial adapter layer narrow and explicit.
- Add targeted snapshot translation tests before cutover.

### Risk 2: The actor-thread refactor destabilizes parked terminal runtime behavior

Mitigation:

- Preserve the current public event model.
- Change ownership model before changing desktop behavior.

### Risk 3: Ghostty improves backend correctness but exposes UI bugs in Hunk's renderer

Mitigation:

- Keep `terminal_surface.rs` stable at first.
- Treat UI regressions as follow-up fixes, not reasons to revert the backend immediately.

### Risk 4: Cross-platform library behavior differs across macOS, Linux, and Windows

Mitigation:

- Keep the fork CI active on all three platforms.
- Do not merge the migration branch until all three platforms are green.

### Risk 5: Backend migration scope gets mixed with shell-compatibility scope

Mitigation:

- Land backend swapping first.
- Reuse `docs/TERMINAL_SHELL_COMPATIBILITY_IMPLEMENTATION_PLAN.md` for environment parity work.

### Risk 6: Snapshot-based mouse reconstruction diverges from Ghostty's real terminal state

Mitigation:

- Treat snapshot-driven mouse encoding as temporary scaffolding only.
- Move mouse encoding into the actor before calling Phase 4 complete.
- Validate mouse-heavy TUIs after the actor-side input path is in place.

## Recommended Execution Order

1. Harden the fork and make Hunk depend on a pinned fork commit.
2. Refactor `hunk-terminal` into backend-friendly modules.
3. Add Ghostty snapshot translation while keeping Alacritty code only as temporary branch scaffolding.
4. Refactor to the actor-thread ownership model.
5. Move terminal input encoding into `hunk-terminal`, and move mouse encoding fully into the actor-backed Ghostty path.
6. Switch Hunk to the Ghostty backend in the migration branch.
7. Run the parity and validation pass.
8. Delete the Alacritty backend.

## Immediate Next Task

The first implementation slice should be:

- Phase 0 completion
- Phase 1 completion

Concretely:

- pin the fork in Hunk
- refactor `hunk-terminal` into backend-friendly modules
- keep the current Alacritty path working
- create the seam where Ghostty can be added without disturbing `hunk-desktop`

That is the highest-leverage starting point because it reduces migration risk without yet forcing a user-visible cutover.
