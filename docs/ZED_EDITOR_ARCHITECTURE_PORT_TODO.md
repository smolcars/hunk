# Zed Editor Architecture Port TODO

Status: In progress

## Goal

Port Hunk's file view and diff view toward the same architectural shape Zed uses:

- one editor-native rendering path for both regular file viewing and diff viewing
- persistent editor state across tab switches
- no per-file editor hydration while scrolling
- no split between "real editor" rendering and "custom preview" rendering

The target is not to copy Zed wholesale. The target is to reproduce the core behavior with Hunk's own crates:

- `hunk-text`
- `hunk-editor`
- `hunk-language`
- `hunk-desktop`

## Zed Reference Points

These are the main Zed files to follow when deciding structure:

- `/tmp/zed-full/crates/git_ui/src/file_diff_view.rs`
- `/tmp/zed-full/crates/git_ui/src/multi_diff_view.rs`
- `/tmp/zed-full/crates/multi_buffer/src/multi_buffer.rs`
- `/tmp/zed-full/crates/editor/src/editor.rs`

Key Zed behaviors to match:

1. Regular file view and diff view both mount an editor over a multibuffer-style model.
2. Multi-file diff uses one persistent editor entity, not one editor per file.
3. Diff hunks are registered as excerpts in the shared buffer model.
4. Scrolling stays inside one display pipeline instead of hydrating files on demand.
5. Diff state stays alive when the tab stays alive.

## Current Hunk Gaps

Today Hunk still differs from that model in important ways:

- `crates/hunk-desktop/src/app/native_files_editor.rs` is single-buffer only.
- `selected_path` still drives too much of Files and Diff behavior in `crates/hunk-desktop/src/app.rs`.
- Files view is editor-backed, but Diff/Review still depends on list rows and per-file orchestration.
- There is no workspace-level coordinate system that spans multiple files in one editor surface.
- Syntax, comments, hunk navigation, and selection do not share one workspace document model.

## What We Can Adapt From Zed

These parts are good candidates to copy or closely adapt:

### Safe To Adapt Closely

- `MultiDiffView::open` flow from `multi_diff_view.rs`
  - background-load all diff entries
  - register excerpts into one multibuffer
  - mount one editor for the whole diff tab
- `register_entry(...)` pattern from `multi_diff_view.rs`
  - compute per-file hunk ranges
  - register them as excerpts with context lines
  - attach diff metadata once
- `common_prefix(...)` path-label logic from `multi_diff_view.rs`
  - use relative display paths for multi-file diff sections
- `FileDiffView` debounce pattern from `file_diff_view.rs`
  - keep the editor entity alive
  - debounce diff recomputation when compared buffers change
- stable path ordering similar to `PathKey`

### Must Be Reimplemented In Hunk Terms

- `MultiBuffer` internals from `multi_buffer.rs`
- `Editor::for_multibuffer(...)` internals from `editor.rs`
- display-map, crease, diff hunk controls, and editor rendering internals

Those pieces are too tied to Zed's own buffer/editor stack. We should copy the model, not the implementation.

## Port Phases

### Phase 0: Baseline And Guardrails

Status: Done

- [x] Identify the exact Zed reference files.
- [x] Document which Zed code is adaptable vs. only conceptually reusable.
- [x] Keep the port scoped to one shared editor pipeline for Files and Diff.

### Phase 1: Add Workspace Document And Excerpt Primitives

Status: Done

Targets:

- `crates/hunk-editor/src`
- `crates/hunk-editor/tests`

Tasks:

- [x] Add workspace-level document ids and excerpt ids.
- [x] Add a workspace/excerpt layout model that can map global rows back to file/line coordinates.
- [x] Add tests for excerpt ordering, row lookup, and line-range validation.
- [x] Keep this layer UI-agnostic so both Files and Diff can build on it.

This is the minimum foundation Hunk needs before it can host one editor surface across multiple files.

### Phase 2: Introduce A Shared Workspace Editor Surface

Status: Done

Targets:

- `crates/hunk-desktop/src/app/native_files_editor.rs`
- `crates/hunk-desktop/src/app/controller/editor.rs`
- `crates/hunk-desktop/src/app/render/file_editor_surface.rs`

Tasks:

- [x] Add a workspace-aware editor session type above the current single-buffer editor state.
- [x] Teach the editor surface to render one full-file excerpt for Files mode.
- [x] Keep existing Files behavior intact while routing through the new workspace model.
- [x] Preserve keyboard navigation, clipboard, search, folds, and syntax behavior.

Zed analogue:

- `FileDiffView` still uses an editor over a multibuffer, even for a single file.

### Phase 3: Build A Read-Only Multi-File Diff Surface On The Same Editor Path

Status: In progress

Targets:

- `crates/hunk-desktop/src/app/controller/review_compare.rs`
- `crates/hunk-desktop/src/app/render/diff.rs`
- `crates/hunk-desktop/src/app/controller/file_tree.rs`

Tasks:

- [x] Build one workspace editor model for the entire compared file set.
- [x] Represent each compared file as one or more excerpts with context lines.
- [ ] Replace list-row-driven diff scrolling with one editor-backed scroll surface.
- [ ] Keep file headers and section metadata as lightweight decorations on top of the shared surface.

Current state:
- `hunk-editor` now has a workspace display snapshot primitive over `WorkspaceLayout`, which can project one multi-file viewport without falling back to the flat diff row list.
- Review compare loading now rebuilds a shared workspace session with file ranges and hunk ranges.
- The desktop workspace editor session now supports arbitrary multi-document/multi-excerpt layouts, not just one full-file excerpt.
- Review now builds and persists one shared workspace editor session alongside the compare session and keeps its active document in sync with Review selection/path changes.
- Sticky file headers, hunk navigation, and comment hunk lookup in Review now read from that shared session.
- Review rendering now also reads row content, row metadata, and syntax segment caches from the shared session.
- The remaining gap is the surface itself: Review still scrolls as a list over flattened rows instead of one editor-backed multi-file surface.

Zed analogue:

- `MultiDiffView::open`
- `register_entry`
- `MultiBuffer::set_excerpts_for_path`

### Phase 4: Move Diff Metadata Onto Workspace Coordinates

Status: In progress

Targets:

- `crates/hunk-desktop/src/app/controller/comments.rs`
- `crates/hunk-desktop/src/app/controller/scroll.rs`
- `crates/hunk-desktop/src/app/controller/selection.rs`

Tasks:

- [ ] Move comment anchors from file-local assumptions to workspace row/file mappings.
- [x] Move comment anchors from file-local assumptions to workspace row/file mappings.
- [x] Move hunk navigation to workspace coordinates.
- [ ] Move diff selection and reveal logic off `selected_path`-driven row lists.
- [x] Keep a stable file-path mapping for actions that still need file scope.

Current state:
- Review visible-row segment prefetch now upgrades the shared workspace session cache instead of the legacy `diff_row_segment_cache`.
- Review comment anchor building, row-context collection, and row selection/clamping now use active row/session accessors instead of directly indexing `diff_rows`.
- Review comment-anchor indexing and file-anchor reconcile state now come directly from `ReviewWorkspaceSession`, so comment matching no longer rebuilds those anchors by rescanning the legacy flat row list.
- Review’s current-file decisions for tab revisit, editor active-path sync, scroll-to-file, and next/previous-file navigation now prefer the current shared-surface row/session state before falling back to `selected_path`.
- Review file reveal, scroll-to-file, and comment fallback jumps now resolve through shared workspace file ranges before touching the legacy flat row ranges.
- The remaining gap is that selection state, comments UI state, and the scroll surface are still hosted by the flat diff list instead of one editor-native multi-file surface.

### Phase 5: Unify Syntax, Folding, Search, And Visible-Range Work

Status: Not started

Targets:

- `crates/hunk-language`
- `crates/hunk-editor`
- `crates/hunk-desktop/src/app/native_files_editor*.rs`

Tasks:

- [ ] Make visible-range syntax work operate on workspace excerpts, not separate preview paths.
- [ ] Make fold placeholders and search results work across excerpt boundaries.
- [ ] Ensure inactive diff sections do not need a separate rendering/highlighting system.
- [ ] Keep the 8ms frame budget for 120fps scrolling.

This is the phase that removes the last architectural reason for preview-only rendering.

### Phase 6: Persist Editor Entities Across Tab Switches

Status: In progress

Targets:

- `crates/hunk-desktop/src/app/controller/core_workspace_projects.rs`
- `crates/hunk-desktop/src/app/controller/file_tree.rs`
- `crates/hunk-desktop/src/app.rs`

Tasks:

- [x] Keep Files editor workspace state alive across mode switches.
- [x] Keep Diff editor workspace state alive when compare inputs are unchanged.
- [x] Recompute only when compare sources or repo snapshot fingerprints actually change.
- [x] Avoid scroll-position and layout churn when revisiting tabs.

Zed analogue:

- open editor items stay mounted and preserve state until the item itself is replaced.

Current state:
- Switching back to Files now reuses the already-loaded or still-loading file editor tab for the same path instead of forcing a reload, which preserves the editor entity and viewport state more like Zed's persistent editor items.
- Review now records which compare pair and repo snapshot fingerprint the loaded workspace session was built from.
- Switching back to Diff reuses the loaded Review surface when that identity still matches, instead of unconditionally rebuilding the compare.
- Reusing an already-loaded Diff surface no longer forcibly re-primes visible-row state, so revisiting Diff preserves the existing viewport/header/prefetch state instead of re-triggering selection churn for the current top row.
- Review also remembers its last selected path separately from Files mode so tab switches can preserve Diff-oriented selection when the session is reused.
- Redundant Review refresh requests now short-circuit when the compare pair, snapshot fingerprint, and collapsed-file layout still match the loaded surface.

### Phase 7: Delete Legacy Diff Rendering Paths

Status: Not started

Targets:

- `crates/hunk-desktop/src/app/render/diff.rs`
- legacy diff row builders and preview-only helpers

Tasks:

- [ ] Remove row-list-driven multi-file diff rendering.
- [ ] Remove duplicated preview syntax/highlight scheduling paths.
- [ ] Remove per-file scroll hydration logic from Diff mode.
- [ ] Keep only one editor-native rendering pipeline for Files and Diff.

## Acceptance Criteria

- Files and Diff both render through the same editor-native workspace model.
- Multi-file Diff uses one persistent editor surface, not one editor per file.
- Switching away from Diff and back does not rebuild the whole surface when compare inputs are unchanged.
- Scrolling a previously visited area does not trigger per-file reload/hydration churn.
- Syntax highlighting and keyboard interaction work consistently in both Files and Diff.
- The app stays within the 8ms frame budget on normal repositories.

## Notes For Implementation

- Prefer copying Zed's orchestration patterns before copying code.
- Keep each phase independently shippable.
- Do not migrate comments, search, and syntax in the same patch as the initial workspace primitive unless the change stays small.
- Avoid making `crates/hunk-editor/src/lib.rs` larger than necessary; add new modules instead.
