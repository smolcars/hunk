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

Current state:
- `FilesEditor` can now also preload multiple full-file workspace documents and switch the active workspace path while preserving per-path buffers and view state, which gives Hunk its first persistent multibuffer-like editor foundation underneath the existing file surface.
- `FilesEditor` can now also open arbitrary `WorkspaceLayout` document sets while keeping those document buffers parked behind one persistent editor instance, so the multibuffer-like foundation is no longer limited to full-file layouts.
- `FilesEditor` can now also project one visible workspace display snapshot across all stored workspace documents from a shared `WorkspaceLayout`, which is Hunk's first editor-side equivalent of Zed's multibuffer display stage.
- `ReviewWorkspaceSession` can now export left/right-side workspace document text from the loaded compare session, which gives the Diff path the document-seeding primitive it needs for a future editor-backed multi-file surface.

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
- [x] Keep file headers and section metadata as lightweight decorations on top of the shared surface.

Current state:
- `hunk-editor` now has a workspace display snapshot primitive over `WorkspaceLayout`, which can project one multi-file viewport without falling back to the flat diff row list.
- Review compare loading now rebuilds a shared workspace session with file ranges and hunk ranges.
- The desktop workspace editor session now supports arbitrary multi-document/multi-excerpt layouts, not just one full-file excerpt.
- Review now builds and persists one shared workspace editor session alongside the compare session and keeps its active document in sync with Review selection/path changes.
- Review workspace rendering now enters through its own dedicated surface module instead of sharing the generic root/diff entry point, which gives Hunk the same kind of explicit boundary Zed has between `FileDiffView` and `MultiDiffView`.
- Review list/split/header viewport state now also lives under a dedicated Review surface state object instead of being scattered across `DiffViewer`, so the future scroller replacement has one surface-local owner to swap.
- Review workspace sessions now also expose stable excerpt sections, and the normal Review surface scrolls over those sections instead of flattening the entire compare into one GPUI row list.
- Review visible-range bookkeeping now follows the active excerpt section window for the normal surface path, so idle syntax/prefetch work no longer keys off the legacy flat-list top-row assumptions.
- Review surface geometry is now owned by `ReviewWorkspaceSession`: section pixel ranges, row top offsets, and visible viewport windowing are computed once from the shared session instead of being spread across `diff.rs` and controller scroll heuristics.
- Review workspace sessions now also build explicit viewport snapshots for the visible section window, so the Diff render path consumes session-owned section/range/spacer geometry instead of recomputing that math inline.
- Review workspace viewport snapshots now also carry the visible row payload and syntax/cache data for each visible section, so the surface consumes session-prepared row state instead of re-querying controller-owned row caches while painting.
- Review viewport snapshots now also build session-owned left/right workspace display rows from the shared `WorkspaceLayout` before rendering visible sections, which moves the Review paint path one step closer to Zed's `layout -> multibuffer display snapshot -> editor render` shape instead of reading raw row text directly.
- Review viewport snapshots no longer clone raw diff rows or row metadata for painting; they now carry row indices plus per-column display rows, with the shared Review session staying the sole owner of raw row/state data behind the renderer.
- Visible Review code rows in that workspace-sections surface now paint through a dedicated GPUI element backed by the shared viewport snapshot, instead of building per-cell and per-segment GPUI child trees for the hot scrolling path.
- Visible Review hunk-divider, meta, and empty rows in that workspace-sections surface now also paint through dedicated GPUI elements backed by the shared row/session state, leaving file-header banners as the remaining legacy row subtree in the visible Diff surface.
- Review viewport snapshots now also carry per-row local surface offsets and row heights, and the visible Review section renderer consumes those geometry snapshots through one painted section element plus sparse file-header/comment overlays instead of one GPUI wrapper per visible row.
- The painted Review section machinery now lives in its own render module instead of being coupled to the code-row painter, and comment affordance overlays are now mounted only for rows that can actually show one.
- Visible Review file-header rows now paint from the shared viewport/session state too, leaving only the collapse and view-file controls as sparse header overlays in the hot scrolling surface.
- Visible Review viewport rows now also carry stable ids, row kinds, stream/file metadata, and per-cell identity directly in the shared snapshot, so the painted section and sparse overlays no longer reach back through `active_diff_row*` helpers while building visible rows.
- Visible Review section rendering now also follows that shared geometry: the workspace surface only builds viewport-intersecting rows within each visible excerpt section, with session-owned spacer offsets keeping the overall surface stable instead of rendering whole hunks at once.
- The visible Review workspace surface now paints through one viewport-level painted element over the session snapshot, with sparse overlays positioned by shared surface coordinates instead of one wrapper subtree per visible section.
- Once a Review workspace session loads, Diff mode no longer falls back to the legacy row-list scroller for multi-file rendering; the shared workspace surface is now the only multi-file Review surface, with a simple status surface reserved for loading/error states.
- Review loading, empty, and error states now ride on dedicated Review surface state instead of legacy `diff_rows` message rows, which removes another flat-row-only dependency from Diff mode.
- Review comment editing no longer forces Diff back onto the legacy flat list; the composer is now rendered as an overlay anchored from shared Review surface geometry, so the workspace surface stays active while editing comments.
- Sticky file headers, hunk navigation, and comment hunk lookup in Review now read from that shared session.
- Review rendering now also reads row content, row metadata, and syntax segment caches from the shared session.
- Review’s live row count now comes from the shared workspace layout rather than the legacy flat render vector length, so list sizing and visible-row sync track the workspace model directly.
- Once the Review session is loaded, the live Review surface rows no longer need to stay duplicated in the top-level legacy `diff_rows` caches; the shared session is now the source of truth for row data during Diff mode.
- Review compare apply no longer needs the generic flat-row load path when the shared session builds successfully; it now initializes the visible Review surface state directly from the workspace session and only falls back to the legacy row path if session construction fails.
- The remaining gap is the surface itself: Review still scrolls as a list over flattened rows instead of one editor-backed multi-file surface.

Zed analogue:

- `MultiDiffView::open`
- `register_entry`
- `MultiBuffer::set_excerpts_for_path`

### Phase 4: Move Diff Metadata Onto Workspace Coordinates

Status: Done

Targets:

- `crates/hunk-desktop/src/app/controller/comments.rs`
- `crates/hunk-desktop/src/app/controller/scroll.rs`
- `crates/hunk-desktop/src/app/controller/selection.rs`

Tasks:

- [ ] Move comment anchors from file-local assumptions to workspace row/file mappings.
- [x] Move comment anchors from file-local assumptions to workspace row/file mappings.
- [x] Move hunk navigation to workspace coordinates.
- [x] Move diff selection and reveal logic off `selected_path`-driven row lists.
- [x] Keep a stable file-path mapping for actions that still need file scope.

Current state:
- Review visible-row segment prefetch now upgrades the shared workspace session cache instead of the legacy `diff_row_segment_cache`.
- Review comment anchor building, row-context collection, and row selection/clamping now use active row/session accessors instead of directly indexing `diff_rows`.
- Review comment-anchor indexing and file-anchor reconcile state now come directly from `ReviewWorkspaceSession`, so comment matching no longer rebuilds those anchors by rescanning the legacy flat row list.
- Review’s current-file decisions for tab revisit, editor active-path sync, scroll-to-file, and next/previous-file navigation now prefer the current shared-surface row/session state before falling back to `selected_path`.
- Review file reveal, scroll-to-file, and comment fallback jumps now resolve through shared workspace file ranges before touching the legacy flat row ranges.
- Review row selection, visible-row sync, and diff-list sizing now derive their live row count and current file from the shared workspace session before falling back to the legacy flat row state.
- Diff-mode file collapse and diff-stream reload retention now preserve Review-owned selection through `current_review_path`/`review_last_selected_path` instead of mirroring back into the Files-mode `selected_path`.
- The remaining gap is no longer metadata ownership. It is the surface itself: selection UI and scrolling are still hosted by the flat diff list instead of one editor-native multi-file surface.

### Phase 5: Unify Syntax, Folding, Search, And Visible-Range Work

Status: In progress

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

Current state:
- Review’s viewport bookkeeping now tracks the shared workspace surface’s visible row range directly, and Review syntax-segment prefetch now reuses that explicit range instead of a legacy top-row fallback.
- Review workspace sessions now also own per-file line stats and compute their own visible segment-prefetch worklists from the shared viewport snapshot, so Review syntax scheduling no longer has to rebuild that plan by rescanning `active_diff_row*` in the controller.
- Review syntax/segment prefetch now derives its candidate rows from the shared workspace viewport snapshot instead of rebuilding a separate legacy row-range heuristic, so visible-range scheduling follows the same session-owned surface state the renderer consumes.
- Review syntax/segment prefetch dispatch now also has its own workspace-session controller path, so Diff no longer shares the old flat-row segment-cache scheduler with Files mode before the background cache build even starts.
- Review surface state now also caches one shared session-built surface snapshot, so render, sticky-header resolution, and visible-range consumers read the same viewport payload instead of rebuilding visible-state math and viewport sections independently on each path.
- The visible Review renderer now also consumes session-owned per-column workspace display rows from those viewport snapshots, so the display-model layer for Diff is no longer just implicit raw cell text glued directly into the GPUI surface.
- Review viewport snapshots now also carry render-ready syntax segments for each visible code row cell, so the painted Review surface no longer reaches back through global diff-row segment caches while painting visible rows.
- Review surface snapshots now also carry file-header line stats and comment-affordance visibility for visible rows, so sparse overlay planning no longer needs controller-side visible-row rescans during paint.
- Review workspace file-header, code-row, and meta-row text shaping now reuse native editor paint helpers, so Diff no longer maintains a fully separate single-line text/gutter shaping path for visible rows.
- Review viewport scroll-surface assembly and sparse overlay planning now enter through the dedicated Review surface module instead of the generic flat diff renderer, which narrows the remaining custom surface boundary around the shared session snapshot.
- Review row hit-testing inside the painted workspace viewport now resolves through shared viewport/session geometry instead of a local painted-row scan, which moves interaction targeting one step closer to the same display-model ownership Zed keeps inside the editor.
- Review painted rows no longer duplicate visible row bounds; the painted workspace viewport now reads row positions and heights back from the shared viewport snapshot during paint, so visible Review geometry has one owner for both targeting and painting.
- Review sparse overlay anchors for file-header controls and comment affordances now ride on the shared surface snapshot instead of a render-time rescan over visible rows, which moves another piece of viewport composition onto one snapshot model.
- Review workspace sessions now also build the sparse overlay plan for visible file-header controls, comment affordances, and the active comment editor anchor directly into the surface snapshot from one surface-options input, so controller code no longer decorates visible rows after the snapshot is built.
- Review sticky file-banner resolution now also rides on that shared surface snapshot, and the Review render path reuses one snapshot instance for both the scroller and sticky banner instead of refreshing viewport/header state twice in one render.
- Review’s active comment-editor card anchor now also comes from the shared surface snapshot instead of recomputing row/scroll geometry in the render helper, so the remaining Review viewport overlays share one snapshot-owned positioning model.
- Review’s current visible top row/file/header state is now also derived from a single session-built visible-state snapshot instead of separate ad hoc row-range/start caches, which keeps selection sync and sticky-header decisions tied to the shared viewport model.
- Review line-number gutter sizing and sticky file-header resolution in Diff mode now resolve through `ReviewWorkspaceSession` helpers instead of controller-side rescans over `active_diff_row*`, which keeps more layout derivation owned by the shared session.
- Diff-mode top-row, visible-range, and sticky-file banner resolution now consume that cached visible-state snapshot first instead of recomputing viewport math independently in each controller/render path.
- Diff-mode sticky-header, file-anchor, and visible-file resolution paths now prefer the shared workspace session and active row accessors instead of reading `diff_row_metadata` directly.
- Review file selection now also prefers the persistent workspace editor session’s active path before falling back to duplicated top-level Diff selection fields, which moves active-file ownership closer to the editor/session itself.
- Diff-mode compare rebuilds, file-to-file navigation, visible-row selection sync, and no-session review path selection now preserve the Review workspace/editor session’s active path or `review_last_selected_path` before falling back to the mirrored top-level `selected_path`.
- Diff mode no longer persists duplicate `file_row_ranges` or visible header lookup vectors when a Review workspace session exists; those file-range and header queries are now expected to resolve from the shared session instead of cached flat-row state.

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
- Workspace project persistence no longer stores duplicate flat diff-row vectors when a Review workspace session exists; the shared session now remains the authoritative Diff row/syntax source across project or tab state restores.
- Diff selection no longer needs to mirror into the Files-mode `selected_path`; Review tree highlighting and Review workspace path resolution now prefer the Review session/editor selection state directly.
- Background snapshot application no longer rewrites Files-mode `selected_path`/`selected_status` while Diff mode is active, so Files selection stays stable across Review refreshes.

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
