# Diff Viewer Implementation Plan

## Status

Proposed follow-on plan for the next stage of diff-viewer work.

This document extends the work in `docs/DIFF_VIEW_QUALITY_TODO.md`. That roadmap improved the current custom diff renderer substantially, but it intentionally stopped short of editor-backed diffing and inline editing. This plan covers the work required to reach Zed-level quality.

## Goal

Build a diff viewer that matches the quality bar of Zed's diff experience for:

- stable scrolling and viewport preservation during live refresh
- correct default review semantics for current worktree changes
- editable inline diff content
- reliable first-paint syntax highlighting
- fast navigation and rendering on large diffs

## Product Requirements

- When the agent edits files while the Review tab is visible, the diff must update without jumping the user back to the top or causing visible stutter.
- When AI inline diff affordances open the Review tab, the default view must show the current worktree delta for that workspace, not the full branch diff against the default branch.
- The diff surface must support direct text editing in the modified content.
- Syntax highlighting must be present on initial paint and must not depend on the user scrolling first.
- The final architecture must scale to large repositories and large patches without breaking the 8 ms frame-budget goal for interactive paths.

## Non-Goals

- Three-way merge tooling in the Review tab.
- Blame or history panels inside the same diff surface.
- A full Zed clone. The goal is parity in quality and interaction model, not source-level parity in every subsystem.

## Current Gaps

- The current Review refresh path replaces the live diff with temporary loading rows. That destroys scroll state and causes visible jumps during agent-driven edits.
- The current AI-to-Review handoff intentionally chooses `base branch -> workspace target` compare sources, which is wrong for the "show me the current working tree only" flow after earlier branch commits were already pushed.
- The current diff renderer falls back to plain segments until async row-segment prefetch catches up, so syntax highlighting can appear late.
- The current diff surface is a custom row list, not an editor-backed view. That makes inline editing, durable viewport anchoring, and syntax correctness harder than they should be.

## Architectural Decision

The end state should be an editor-backed diff workspace, not an editable version of the current `SideBySideRow` renderer.

Reasons:

- Zed's diff quality comes from rendering real editor buffers with diff overlays and anchored hunks, not from painting a custom diff list.
- Hunk already has reusable pieces for this direction:
  - Git compare logic in `crates/hunk-git`
  - text state in `crates/hunk-text`
  - editor state in `crates/hunk-editor`
  - syntax/highlighting in `crates/hunk-language`
  - a native file editor surface in `crates/hunk-desktop/src/app/native_files_editor.rs`
- Bolting text editing onto the current Review row renderer would create a high-complexity dead-end and still leave scroll, mapping, and highlighting weaker than an editor-backed approach.

## Design Constraints

- Production Git behavior stays in `crates/hunk-git`.
- Compare-source semantics must be explicit and type-safe. Review behavior should not depend on hidden branch assumptions.
- Refreshes must preserve visible state by stable anchors, not by transient row indices.
- UI state must stay GPUI-correct:
  - no task dropping
  - no circular entity ownership
  - state changes must notify
  - background work must be detached or stored intentionally
- New tests go in crate-level `tests/` directories.

## Phase Gate Policy

A phase is only complete when all of the following are true:

- The phase checklist is fully addressed.
- The phase acceptance criteria are satisfied.
- The code has had a deep review for correctness, state drift, stale cache risks, performance regressions, and simplification opportunities.
- New test coverage for the phase exists in the relevant crate-level `tests/` directories.

## Target End State

- Review tab uses an editor-backed diff surface with:
  - read-only left side
  - editable right side
  - synchronized vertical navigation
  - diff-hunk overlays and controls
  - per-file excerpts and sticky file identity
- Compare sources can represent:
  - branch snapshot
  - workspace target working tree
  - workspace target HEAD snapshot
- Review comments stay enabled only on the default review pair for v1.
- The legacy custom row renderer can either be deleted or kept only as a fallback until the new surface has parity.

## Phase 0: Contract, Baseline, and Instrumentation

### TODO

- [ ] Freeze the product contract for the four target behaviors:
  - stable refresh
  - current-worktree default
  - editable diff
  - first-paint highlighting
- [ ] Capture baseline metrics for:
  - refresh latency
  - first diff paint
  - first syntax-highlighted paint
  - scroll smoothness on large diffs
- [ ] Add explicit traces/counters around:
  - review compare refresh start and completion
  - diff row rebuild timing
  - segment-prefetch timing
  - visible-row highlight timing
- [ ] Build a fixture list of representative diffs:
  - small text-only diff
  - multi-file Rust diff
  - large generated diff
  - moved/edited hunks
  - worktree-only changes after branch commits were already pushed

### Likely Files

- `crates/hunk-desktop/src/app/controller/review_compare.rs`
- `crates/hunk-desktop/src/app/controller/core_diff.rs`
- `crates/hunk-desktop/src/app/controller/fps.rs`
- `crates/hunk-desktop/src/app/controller/ai_perf.rs`
- `docs/PERFORMANCE_BENCHMARK.md`

### Acceptance Criteria

- The team has a fixed behavior contract and perf baseline before further refactors.

## Phase 1: Stabilize The Current Review Surface

### TODO

- [ ] Stop replacing the live diff with temporary loading rows for normal refreshes.
- [ ] Keep the existing diff visible while refresh is in flight and render loading state in chrome or overlay only.
- [ ] Introduce a `DiffViewportAnchor` that stores:
  - stable row id
  - side if needed
  - intra-row pixel offset
- [ ] Restore viewport from stable anchors after refresh instead of relying only on `ListState.logical_scroll_top()`.
- [ ] Replace unconditional `scroll_selected_after_reload = true` behavior with an explicit navigation intent model.
- [ ] Only auto-scroll when the user explicitly changed file/hunk focus or when there is no prior viewport to preserve.
- [ ] Preserve selected file and visible-file banner state across refresh.

### Likely Files

- `crates/hunk-desktop/src/app/controller/review_compare.rs`
- `crates/hunk-desktop/src/app/controller/core_diff.rs`
- `crates/hunk-desktop/src/app/controller/scroll.rs`
- `crates/hunk-desktop/src/app/controller/file_tree.rs`
- `crates/hunk-desktop/src/app/render/diff.rs`

### Acceptance Criteria

- Agent-driven edits no longer snap the Review tab to the top.
- Background refreshes do not visibly replace the diff with a one-row placeholder.
- File selection and visible position remain stable through repeated refreshes.

## Phase 2: Fix Review Compare Semantics And AI Handoff

### TODO

- [ ] Extend `crates/hunk-git` compare-source modeling to represent workspace HEAD snapshots separately from workspace working trees.
- [ ] Replace the current implicit "base branch vs workspace target" assumption for AI review entry.
- [ ] Make AI inline diff handoff open:
  - `left = workspace target HEAD snapshot`
  - `right = workspace target working tree`
- [ ] Keep an explicit user action for branch review:
  - `left = resolved base branch`
  - `right = workspace target`
- [ ] Update persisted compare selection behavior so manual branch-review choices do not override the AI worktree-review default for the next AI handoff.
- [ ] Audit Review comments gating to ensure the default comment-enabled pair still makes sense after the new source types are added.

### Likely Files

- `crates/hunk-git/src/compare.rs`
- `crates/hunk-desktop/src/app/controller/review_compare.rs`
- `crates/hunk-desktop/src/app/controller/ai/core_timeline.rs`
- `crates/hunk-desktop/src/app/review_compare_picker.rs`
- `crates/hunk-domain/src/state.rs`

### Acceptance Criteria

- Opening Review from AI inline diffs shows only the current uncommitted worktree changes for that workspace.
- Branch review still exists, but only when explicitly requested.

## Phase 3: Fix First-Paint Syntax Highlighting In The Current Renderer

### TODO

- [ ] Prime visible-row segment caches immediately after diff load.
- [ ] Compute at least `SyntaxOnly` segments for the initially visible window before the user scrolls.
- [ ] Keep async upgrade to `Detailed` segments for idle time or nearby rows.
- [ ] Remove any dependency on FPS idle sampling as the first opportunity for syntax paint.
- [ ] Add a bounded overscan strategy for visible highlight computation, similar to the file editor.
- [ ] Ensure highlight invalidation is tied to stable diff revisions, not just scroll movement.

### Likely Files

- `crates/hunk-desktop/src/app/controller/scroll.rs`
- `crates/hunk-desktop/src/app/controller/fps.rs`
- `crates/hunk-desktop/src/app/data.rs`
- `crates/hunk-desktop/src/app/data_segments.rs`
- `crates/hunk-desktop/src/app/render/diff_rows.rs`

### Acceptance Criteria

- Syntax colors are present on the first usable paint of the Review tab.
- Scrolling still upgrades nearby rows without blocking interaction.

## Phase 4: Editor-Backed Diff Foundation

### TODO

- [ ] Define the editor-backed Review surface abstraction in desktop code.
- [ ] Decide the smallest viable ownership boundary:
  - keep the first implementation in `crates/hunk-desktop`
  - extract shared logic later only if duplication appears
- [ ] Build a diff document model that maps compare snapshots into editor-friendly excerpts instead of only `SideBySideRow`s.
- [ ] Represent per-file diff state with:
  - left/base text
  - right/working text
  - hunk ranges
  - file identity and status
- [ ] Reuse `hunk_language` highlighting and `hunk_editor` display state rather than inventing a second editing pipeline.
- [ ] Define how Review comments anchor to editor-backed hunks and lines.
- [ ] Define migration strategy:
  - feature flag or parallel implementation path
  - safe fallback to legacy renderer during rollout

### Likely Files

- `crates/hunk-desktop/src/app/native_files_editor.rs`
- `crates/hunk-desktop/src/app/render/file_editor_surface.rs`
- `crates/hunk-editor/src/lib.rs`
- `crates/hunk-text/src/*`
- `crates/hunk-git/src/compare.rs`

### Acceptance Criteria

- The codebase has a clear editor-backed Review abstraction that can host diff excerpts without depending on the legacy row list.

## Phase 5: Editable Single-File Diff MVP

### TODO

- [ ] Implement a single-file editor-backed diff view with:
  - read-only left side
  - editable right side
  - synchronized vertical scroll
  - diff overlays
  - syntax highlighting on both sides
- [ ] Recalculate diff hunks after right-side edits with debouncing.
- [ ] Preserve cursor and viewport during diff recomputation.
- [ ] Ensure save/reload behavior respects the active workspace target.
- [ ] Add comment affordances to the editor-backed hunk surface.
- [ ] Keep diagnostics disabled unless explicitly reintroduced later.

### Likely Files

- `crates/hunk-desktop/src/app/native_files_editor.rs`
- `crates/hunk-desktop/src/app/render/file_editor_surface.rs`
- `crates/hunk-desktop/src/app/controller/editor.rs`
- `crates/hunk-desktop/src/app/controller/review_compare.rs`
- `crates/hunk-desktop/src/app/render/diff.rs`

### Acceptance Criteria

- A modified file in Review can be edited inline.
- Diff overlays and syntax highlighting stay correct after edits.
- Refreshing the diff does not lose cursor or viewport unnecessarily.

## Phase 6: Multi-File Review Workspace

### TODO

- [ ] Replace the custom multi-file Review list with an editor-backed multi-file surface.
- [ ] Add per-file excerpts and sticky file boundaries.
- [ ] Keep file-to-file navigation parity with the current Review tab:
  - next file
  - previous file
  - next hunk
  - previous hunk
  - jump to selected file
- [ ] Preserve sidebar and tree integration with the new Review surface.
- [ ] Ensure compare-source switching rebinds the editor-backed Review state safely.
- [ ] Keep commentability limited to the default review pair in v1.

### Likely Files

- `crates/hunk-desktop/src/app/render/diff.rs`
- `crates/hunk-desktop/src/app/render/root.rs`
- `crates/hunk-desktop/src/app/controller/selection.rs`
- `crates/hunk-desktop/src/app/controller/file_tree.rs`
- `crates/hunk-desktop/src/app/controller/review_compare.rs`

### Acceptance Criteria

- Review behaves like a multi-file diff editor instead of a painted row list.
- File and hunk navigation stay fast and predictable on large patches.

## Phase 7: Performance Hardening, Cleanup, And Rollout

### TODO

- [ ] Profile large-diff behavior after the editor-backed migration.
- [ ] Verify no interactive path exceeds the frame budget in ordinary scrolling and navigation.
- [ ] Remove legacy row-renderer code once parity is reached, or explicitly mark it as fallback-only.
- [ ] Add regression tests for:
  - refresh while visible
  - AI handoff semantics
  - initial highlight availability
  - editable diff recomputation
  - multi-file navigation stability
- [ ] Update docs that still describe the old Review model.
- [ ] Do final workspace validation once at the end:
  - `./scripts/run_with_macos_sdk_env.sh cargo build --workspace`
  - `./scripts/run_with_macos_sdk_env.sh cargo clippy --workspace --all-targets -- -D warnings`
  - `./scripts/run_with_macos_sdk_env.sh cargo test --workspace`

### Acceptance Criteria

- The editor-backed Review surface is the default path.
- The major failure modes from the old Review renderer are covered by regression tests.
- Build, clippy, and full tests pass at the end of the implementation track.

## Test Plan

Add new tests in crate-level `tests/` directories:

- `crates/hunk-git/tests`
  - compare-source resolution
  - workspace-head vs workspace-worktree diffs
  - base-branch fallback behavior
- `crates/hunk-desktop/tests`
  - Review viewport preservation during refresh
  - AI inline diff opens current-worktree review
  - editor-backed diff editing and diff recomputation
  - multi-file Review navigation
- `crates/hunk-language/tests`
  - initial highlight coverage for representative Review excerpts if any new highlight path is introduced

## Risks And Mitigations

- Risk: trying to edit inside the current custom row list will waste time and still not reach parity.
  - Mitigation: treat the current renderer as a stabilization target only, not the final architecture.
- Risk: compare-source modeling grows ad hoc and becomes hard to reason about.
  - Mitigation: make source identity explicit in `crates/hunk-git` before further Review UI work.
- Risk: editor-backed Review work regresses performance.
  - Mitigation: baseline first, instrument hot paths, and keep excerpt loading incremental.
- Risk: comment anchoring breaks during the renderer transition.
  - Mitigation: keep comment logic scoped to stable hunk/file identities and preserve a temporary fallback path until parity is verified.

## Recommended Implementation Order

1. Phase 0
2. Phase 1
3. Phase 2
4. Phase 3
5. Phase 4
6. Phase 5
7. Phase 6
8. Phase 7

## Completion Definition

This project is complete when:

- Review no longer jumps during live refresh.
- AI inline diff handoff opens the correct current-worktree comparison by default.
- The Review tab supports inline editing in the modified content.
- Syntax highlighting is visible on first paint.
- The editor-backed Review surface is fast enough to replace the legacy renderer as the default path.
