# AI Inline Diff In AI Workspace Todo

## Status

- Owner: Hunk
- Last Updated: 2026-04-07
- Scope: show agent-produced diffs directly inside the AI thread with a unified up-down presentation, while keeping the full Review tab as a separate deep-review path

## Product Goal

The AI timeline should be able to show diffs inline in the same tab instead of forcing an immediate jump to Review.

Target behavior:

- diff rows in the AI thread expand inline into a unified diff preview
- the inline preview is lightweight and fast enough to live inside the painted AI surface
- a separate explicit action still opens the full Review experience
- the existing right-side inline Review pane remains available for deep inspection, but it is not the only way to inspect edits

## Architecture Decision

We will not embed `ReviewWorkspaceSession` directly into the AI thread body.

Reason:

- Review is a full side-by-side workspace compare surface
- it builds workspace documents, excerpts, display rows, and two-column viewport state
- that is the wrong cost model for many inline turn diffs inside a chat thread

Instead we will:

- reuse `hunk_domain::diff::parse_patch_document(...)` and related diff primitives
- build a lightweight AI-specific unified-diff projection
- cache that projection in the AI workspace session
- paint the visible inline diff rows directly inside the AI surface
- keep `Open in Review` as an explicit handoff to the existing Review surface

## Phases

### Phase 1: Projection Foundation

- [x] Todo 1: Create a lightweight AI inline-diff projection module and tests.
  Plan:
  - add a dedicated `ai_workspace_inline_diff.rs` module in `hunk-desktop`
  - split turn diffs into file-scoped patch sections
  - reuse `parse_patch_document(...)` per file section
  - emit structured file, hunk, and line data for unified rendering
  - include truncation options for files, hunks, and lines so the future paint path can stay bounded

- [x] Todo 2: Define the initial truncation and summary policy for inline diffs.
  Plan:
  - choose default limits for max files, hunks per file, and lines per hunk
  - define how truncated content is surfaced in the UI
  - define when a diff should stay collapsed by default
  - define when very large diffs should immediately recommend `Open in Review`
  Decisions:
  - default preview limits are `4` files, `6` hunks per file, and `80` lines per hunk
  - inline diffs stay collapsed by default in v1, regardless of size
  - truncated previews surface a thread-safe notice instead of trying to silently approximate the full patch
  - `Open in Review` should be recommended whenever truncation occurs or when the diff exceeds `160` changed lines even without truncation

### Phase 2: AI Session Integration

- [x] Todo 3: Extend the AI workspace session model to cache inline diff projection by row and width bucket.
  Plan:
  - add structured inline-diff payloads to AI diff blocks or adjacent session caches
  - key invalidation off `row_id` and `last_sequence`
  - keep projection cached separately from text layout so streaming updates only rebuild affected diff rows

- [x] Todo 4: Add inline diff expansion state to AI timeline rows.
  Plan:
  - support collapsed summary vs expanded inline diff state per AI diff row
  - preserve expansion state per thread as timeline rows update
  - keep diff selection and text selection behavior stable while expanding/collapsing

### Phase 3: Painted Unified Diff Rendering

- [x] Todo 5: Paint unified inline diff rows inside the AI surface.
  Plan:
  - add file header, hunk header, context, removed, added, and meta row paint helpers
  - use theme colors only
  - keep rendering monospace and visible-range only
  - avoid introducing nested GPUI row trees for diff lines

- [x] Todo 6: Add row geometry and hit-testing for inline diff content.
  Plan:
  - compute diff row heights as part of the AI surface layout cache
  - support line-local hit targets without breaking block-local hover and selection behavior
  - preserve the existing 8ms frame-budget mindset

### Phase 4: Interaction Changes

- [x] Todo 7: Change AI diff click behavior from immediate Review-tab navigation to inline expansion.
  Plan:
  - primary click should expand/collapse the inline diff preview
  - remove the current default jump-to-Review behavior from summary-block click
  - keep the behavior thread-local and stable across refreshes

- [ ] Todo 8: Add explicit diff actions.
  Plan:
  - add `Open in Review` to jump to the full Review tab
  - add `Open in Side Pane` to select the existing right-side inline Review pane
  - ensure these actions do not conflict with expand/collapse hit zones

- [ ] Todo 9: Add selection and copy behavior for inline diff content.
  Plan:
  - support text selection across inline diff rows
  - support copying hunks or selected diff text
  - preserve the thread-wide selection semantics already added to the AI surface

### Phase 5: Performance and Validation

- [ ] Todo 10: Add instrumentation and cache invalidation safeguards.
  Plan:
  - record projection/build timings for inline diff payloads
  - ensure scrolling reuses cached projection and layout
  - ensure unrelated thread updates do not rebuild expanded diffs outside the affected rows

- [ ] Todo 11: Add coverage and manual QA for representative diff shapes.
  Plan:
  - multi-file patch
  - rename / delete / new file diff
  - large diff truncation
  - binary/meta-only diff
  - Windows perf check on long AI threads

## Current Order Of Work

1. Todo 1 is complete in this slice.
2. Todo 2 is complete: the projection policy now has locked defaults and a review recommendation threshold.
3. Todo 3 is complete: inline diff projection is now cached in the AI workspace session by row/version and width bucket.
4. Todo 4 is complete: turn-diff rows now participate in the existing AI row-expansion state and preserve that state across timeline/session refreshes.
5. Todo 5 is complete: expanded diff rows now render unified file, hunk, context, added, removed, and meta lines directly inside the AI painted surface.
6. Todo 6 is complete: inline diff preview lines now carry diff-aware hit targets through the session layout cache, and the AI surface hit-test path can distinguish file headers and diff lines without regressing block-level hover or selection behavior.
7. Todo 7 is complete: expandable diff summaries now toggle inline inside the AI thread on primary click instead of navigating away to the Review tab.
8. Todo 8 is next: add explicit diff actions for `Open in Review` and `Open in Side Pane`.
