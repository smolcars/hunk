# Files Tab Helix Editor Implementation Plan

Date: 2026-03-13
Owner: Codex
Status: In Progress
Scope: Replace the current Files-tab editor pane with a Helix-backed editor implementation while keeping the existing file tree, Files workspace shell, and Hunk app architecture intact.

## Decision

This direction makes sense for the Files tab.

The important constraint is this:

- Keep Hunk's existing Files-tab shell.
- Keep the current file tree, file selection flow, and save/reload integration.
- Replace only the right-hand editor pane implementation.
- Treat `helix-gpui` as an editor-engine and rendering reference, not as an application architecture to copy wholesale.

This is a breaking change by design. The goal is not to preserve the current `InputState`-based Files editor surface. The goal is to replace it with a stronger editor subsystem that can grow into a serious code-editing surface.

## Why This Makes Sense

Hunk already has the Files-tab layout and workflow we need:

- left repository tree
- right file editor pane
- file selection and switching
- save/reload actions
- dirty-state guardrails
- Files-mode-only state transitions

The current Files editor is initialized as a GPUI `InputState` code editor:

- `crates/hunk-desktop/src/app/controller/core_bootstrap.rs`

Its load/save/dirty-state behavior already lives in Files-specific controller code:

- `crates/hunk-desktop/src/app/controller/editor.rs`
- `crates/hunk-desktop/src/app/controller/file_tree.rs`
- `crates/hunk-desktop/src/app/render/file_editor.rs`

That gives us a clean replacement seam. We do not need to re-architect Review, Git, or AI to make this work.

## What `helix-gpui` Actually Gives Us

`helix-gpui` is useful because it already demonstrates three things we want:

1. A Helix-backed text editing engine embedded in GPUI.
2. Manual document painting with syntax highlighting, gutters, cursor rendering, and selection overlays.
3. Real Helix event processing, including jobs and language-server traffic.

### Syntax Highlighting

`helix-gpui` does not use `syntect` for editor rendering.

It asks Helix for syntax highlight events, merges them with selection and diagnostic highlight layers, converts the resulting style runs into GPUI `TextRun`s, shapes text with GPUI's text system, and paints the document manually.

This is materially different from Hunk's current diff/file highlighting path, which is built around `syntect` tokenization and Hunk-owned segment rendering.

Implication:

- Keep Hunk's existing `syntect` highlighting for diff/review surfaces.
- Use Helix's own highlighting pipeline inside the replacement Files editor pane.

### LSP Support

`helix-gpui` already includes real Helix LSP plumbing:

- language-server message handling
- diagnostic publication
- workspace edits
- capability registration
- initial `didOpen` behavior
- progress/status surfaces

That means a Helix-backed Files editor can support LSP. But LSP should not be phase 1.

Implication:

- syntax-highlighted editing is phase 1
- diagnostics and basic LSP UI are phase 2
- completions, hover, code actions, and deeper IDE behavior are later phases

## Architectural Direction

## Files-only boundary

The boundary for this work is:

- inside scope: Files tab editor pane
- outside scope: Review tab, Git tab, AI tab, diff rendering, app-wide workspace mode model

The replacement must fit into the existing Files workflow:

- Hunk tree selects a path
- Files controller opens that path in the editor pane
- editor pane reports dirty state and save intent
- Hunk controller keeps authority over file switching and save/reload policy

## Keep Hunk in charge of file lifecycle

The new editor pane must not become the owner of repository state.

Hunk should continue owning:

- selected file path
- file tree state
- file switching logic
- unsaved-change guardrails
- save/reload commands
- Files workspace layout

The editor pane should own:

- editor buffer state
- cursor and selection state
- viewport state
- Helix editor runtime state
- local edit commands and rendering
- editor-focused key handling

## Do not copy `helix-gpui`'s app model

Do not port these concepts into Hunk as top-level application structure:

- `helix-gpui` root `Application` as the main app model
- its window/workspace composition
- its overlay/picker/prompt shell
- its broad action/menu model

We only want the editor-engine slice:

- Helix `Editor`
- Helix `EditorView`
- Helix `Jobs`
- editor event loop
- document painting logic
- input translation
- optional diagnostics/LSP surfaces

## Proposed Hunk Structure

Add a dedicated Files editor subsystem under `crates/hunk-desktop/src/app`.

Recommended shape:

- `controller/files_editor.rs`
- `render/files_editor.rs`
- `render/files_editor_gutter.rs`
- `render/files_editor_status.rs`
- `render/files_editor_overlays.rs` only if needed later
- `types/files_editor.rs` or equivalent local types module if state grows

If the subsystem grows further, consider a dedicated folder:

- `crates/hunk-desktop/src/app/files_editor/`

Responsibility split:

- controller code owns file open/save/reload synchronization and runtime stepping
- render code owns GPUI view composition and painting
- Hunk controller continues to own Files-tab routing and tree interactions

## Integration Contract

The Helix-backed pane should expose a narrow contract to the existing Files controller.

Minimum contract:

- `open_file(path, text, language)`
- `reload_file(path, text, language)`
- `save_requested()`
- `current_text()`
- `is_dirty()`
- `set_focus()`
- `handle_external_save_result(...)`
- `clear()`

Optional contract for later:

- `cursor_position()`
- `scroll_position()`
- `restore_view_state(...)`
- `diagnostics_snapshot()`
- `go_to_line(...)`
- `find(...)`

The Files controller remains the caller for:

- `request_file_editor_reload`
- `save_current_editor_file`
- `prevent_unsaved_editor_discard`
- `clear_editor_state`

## Phase Plan

## Implementation Status

Completed in the first implementation slice:

- [x] Added Helix dependencies to `hunk-desktop`
- [x] Moved Hunk to a current upstream Helix revision instead of the old `helix-gpui` pin
- [x] Added a dedicated Files-tab Helix editor subsystem
- [x] Kept Hunk's existing Files tree, file selection flow, and save/reload authority
- [x] Wired file open/load into the Helix-backed pane
- [x] Wired current buffer text and dirty tracking into Hunk's save flow
- [x] Added Helix-backed rendering for syntax-colored text, line numbers, cursor, focus, and wheel scrolling
- [x] Added mouse cursor placement, drag selection, and visible selection rendering
- [x] Added editor-local mode/language/position status UI
- [x] Kept the existing Files header and save/reload UI
- [x] Preserved markdown preview mode by continuing to use Hunk's existing preview flow
- [x] Kept the old `InputState` Files editor as a fallback path if Helix fails to open

Still pending:

- [ ] richer selection parity beyond the current primary-selection rendering
- [ ] view-state persistence across file switches
- [ ] real LSP enablement
- [ ] diagnostics rendering
- [ ] completion / hover / code actions
- [ ] removing the old Files-mode `InputState` implementation
- [ ] final cleanup after the coexistence phase

### Phase 0: Spike and dependency validation

Goals:

- prove Helix crates can be embedded in `hunk-desktop`
- confirm the right crate versions and runtime assumptions
- verify GPUI integration can coexist with current Hunk code

Tasks:

- [x] add a throwaway Helix-backed editor pane prototype behind a local feature branch
- [x] validate startup/runtime behavior for the editor entity inside Files mode
- [x] confirm theme/color mapping strategy from Helix theme styles into Hunk theme colors
- [x] document exact new dependencies in `Cargo.toml`

Exit criteria:

- a single file can be opened and painted read-only through Helix-backed rendering inside the Files pane

### Phase 1: Replace the Files editor rendering path

Goals:

- swap the current `InputState` editor pane for a Helix-backed editor pane
- retain existing file toolbar, header, save button, and load/error states

Tasks:

- [x] add a Files-editor-specific GPUI entity/view for Helix rendering
- [x] open the selected file using Hunk's existing file-loading path
- [x] initialize Helix editor state for the opened document
- [x] paint text, cursor, selection, line numbers, and gutter
- [x] keep current file header and save/reload buttons

Exit criteria:

- [x] Files tab can open and display text files using Helix-backed rendering
- [x] syntax highlighting works
- [x] line numbers render
- [x] focus stays inside the editor when expected

### Phase 2: Editing parity and dirty-state integration

Goals:

- support normal text editing in Files mode
- preserve Hunk's current save/reload and unsaved-change semantics

Tasks:

- [x] wire GPUI key events into Helix key/input handling
- [ ] support insert/delete/newline/tab/navigation/selection
- [x] expose current buffer text to Hunk save flow
- [x] replace `editor_input_state` dirty detection with editor-pane dirty detection
- [x] keep existing save action entry points but back them with Helix buffer text

Exit criteria:

- [x] edit, save, reload, and file switching work correctly
- [x] unsaved-change guardrails still block destructive file switches

### Phase 3: View-state persistence and Files UX parity

Goals:

- make the replacement feel at least as good as the current Files experience

Tasks:

- [ ] preserve cursor and scroll position when switching files
- [x] preserve current markdown-preview behavior decision
- [x] decide whether markdown preview remains a separate right-pane mode or is temporarily removed
- [x] add statusline or inline state for mode, path, dirty state, and cursor position if useful

Exit criteria:

- [ ] common Files-tab flows feel stable and predictable
- [ ] there is no obvious regression in switching and reopening files

### Phase 4: LSP and diagnostics

Goals:

- enable real language-server support inside the Files editor pane

Tasks:

- [ ] port the minimal Helix job/event loop needed for editor async work
- [ ] handle diagnostics
- [ ] surface diagnostics in gutter and/or inline overlay
- [ ] surface status/progress without introducing noisy app-wide overlays

Exit criteria:

- [ ] diagnostics appear for supported languages
- [ ] editor remains responsive while background jobs run

### Phase 5: Hardening and cleanup

Goals:

- remove the old Files-tab editor implementation cleanly
- reduce duplicated editor logic

Tasks:

- [ ] delete Files-mode dependence on `editor_input_state`
- [ ] remove obsolete Files editor code paths in controller/render modules
- [ ] keep markdown preview only if it still fits the product direction
- [ ] document the final Files editor architecture

Exit criteria:

- [ ] there is one editor implementation for Files mode
- [ ] controller code is simpler than the temporary coexistence phase

## Explicit Non-Goals For First Release

- multi-cursor editing
- refactors to Review tab diff rendering
- replacing Hunk's `syntect` diff syntax pipeline
- Helix prompt/picker/overlay parity
- full Helix command palette behavior
- debugger integration
- code actions, rename symbol, and deep IDE workflows

## Key Risks

### 1. Input and keybinding conflicts

Helix-style modal/editor-focused key handling can conflict with Hunk app-level shortcuts.

Mitigation:

- make the Files editor a strong focus island
- explicitly scope which shortcuts remain global while the editor is focused
- prefer Files-pane-local handling first, then bubble only unhandled commands

### 2. Runtime complexity

The current Files editor is simple because `InputState` owns most editing behavior. A Helix-backed pane introduces a more complex runtime model.

Mitigation:

- keep the runtime local to the Files editor subsystem
- do not spread Helix types throughout unrelated Hunk modules
- keep Hunk controller APIs narrow

### 3. Theme mismatch

Helix theme tokens do not map directly onto Hunk's theme surfaces.

Mitigation:

- create an explicit color bridge layer
- use Hunk theme values for container/background chrome
- use mapped Helix token colors only for code-rendering surfaces

### 4. Markdown preview fit

The current Files tab includes markdown preview logic tightly coupled to the existing editor flow.

Mitigation:

- decide early whether markdown preview stays, is deferred, or becomes a separate Files mode
- avoid forcing the Helix editor pane to own markdown rendering concerns

### 5. Large-file performance

Manual document painting can regress if we are careless about shaping and re-layout.

Mitigation:

- measure large-file open and scroll behavior early
- keep viewport-based rendering only
- avoid full-document re-shaping on every frame

## Validation Plan

Manual validation targets:

- `.rs`
- `.toml`
- `.md`
- `.json`
- `.ts`
- `.js`

Critical manual flows:

- open file from tree
- edit file
- save file
- reload file
- switch files with dirty buffer
- switch away and back to Files mode
- open large text file near the size limit
- open binary/non-UTF8 file and verify fallback behavior

## Code Review Gates

Every phase should end with review against these questions:

1. Has Files-tab state ownership stayed local and coherent?
2. Did we keep unrelated app modes out of the integration?
3. Did we reduce or increase controller coupling?
4. Are async tasks and runtime stepping safe under GPUI lifecycle rules?
5. Are save/reload semantics still correct under stale-epoch races?
6. Is editor-focused key handling predictable?

## Final Recommendation

This should be implemented as a Files-tab-only editor replacement.

Do this:

- keep Hunk's current Files shell
- keep tree and file lifecycle in Hunk
- replace only the right-hand editor pane with a Helix-backed subsystem

Do not do this:

- do not import `helix-gpui`'s full workspace/app architecture
- do not expand the scope to Review, Git, or AI
- do not rewrite Hunk around Helix as the top-level application model

## Related Docs

- `docs/FULL_FILE_TREE_TODO.md`
- `docs/GIT_MIGRATION_IMPLEMENTATION_PLAN.md`
