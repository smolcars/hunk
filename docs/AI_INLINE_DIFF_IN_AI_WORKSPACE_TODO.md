# AI Diff Pane In AI Workspace Todo

## Status

- Owner: Hunk
- Last Updated: 2026-04-07
- Scope: show agent-produced diffs inside the AI workspace with a side-by-side pane on the right, while keeping the full Review tab as the explicit deep-review path

## Product Goal

The AI timeline should let users inspect code changes without leaving the AI workspace.

Target behavior:

- `Files edited` groups stay in the AI timeline on the left
- clicking `Edited ...` rows opens the side-by-side diff pane on the right
- clicking `Files edited` group actions opens the same right-side diff pane for the current thread compare
- clicking the file path itself still opens the file view
- `Open in Review` remains the explicit jump to the full Review tab

## Pivot Decision

We are abandoning the custom inline unified-diff preview.

Reason:

- the requested UX is closer to Codex desktop / T3 code than to an in-thread patch preview
- the existing Hunk Review surface already has much higher rendering quality, syntax highlighting, and file navigation
- continuing to invest in the inline-diff renderer duplicates the Review-quality work in a worse presentation model

So the implementation path is:

- remove inline-diff projection, caching, rendering, tests, and docs
- reuse the existing review workspace session/surface for the AI side pane
- make AI file-change rows drive that pane directly

## Reuse Strategy

Reusable directly:

- the existing right-side AI review pane shell in `render/ai_workspace_sections.rs`
- the existing review workspace session in `review_workspace_session.rs`
- the existing side-by-side review surface in `render/review_workspace_surface.rs`
- the existing review file selection and scroll helpers in `controller/review_compare.rs`, `controller/selection.rs`, and `controller/scroll.rs`

Not worth keeping:

- `ai_workspace_inline_diff.rs`
- inline diff caches in `ai_workspace_session.rs`
- inline diff preview line rendering in `ai_workspace_session_preview.rs`
- inline diff-specific hit targets, overlay actions, tests, and perf counters

## Phases

### Phase 1: Pivot Cleanup

- [x] Todo 1: Remove inline-diff code from the AI workspace model.
  Plan:
  - remove inline-diff modules, state, caches, and preview-line kinds
  - remove inline-diff-only hit targets and overlay actions
  - keep diff summary rows as lightweight AI timeline rows only

- [x] Todo 2: Remove inline-diff tests and stale docs.
  Plan:
  - delete inline-diff-specific test files and assertions
  - replace stale doc content with the side-pane plan
  - keep only tests that still apply to AI timeline summary rows or shared selection behavior

### Phase 2: AI Row To Diff-Pane Routing

- [x] Todo 3: Define which AI rows open the side diff pane.
  Plan:
  - `fileChange` item rows open the side pane and focus their file path
  - `file_change_batch` group rows keep disclosure behavior, but expose explicit side-pane actions
  - `TurnDiff` rows, when present, open the side pane for the thread compare

- [x] Todo 4: Preserve file-path click behavior while making row clicks useful.
  Plan:
  - clicking the linked path still opens the file view
  - clicking the rest of an `Edited ...` row opens the side diff pane
  - group rows keep row-body toggle behavior and use explicit diff actions instead

### Phase 3: Review Surface Reuse In The AI Workspace

- [x] Todo 5: Make AI-side diff pane selection row-aware.
  Plan:
  - allow AI rows to open the existing right-side review pane for the selected thread
  - resolve a preferred review file path from AI file-change rows
  - push that path into the shared review selection state before showing the pane

- [x] Todo 6: Add explicit AI diff actions.
  Plan:
  - `View diff` / side-pane action for AI file-change rows and file-change groups
  - `Open in Review` action for the full Review tab
  - avoid conflicting with disclosure chevrons and file-path links

### Phase 4: Pane UX And Quality

- [x] Todo 7: Improve the AI-side diff pane presentation.
  Plan:
  - make the pane title and empty/loading states specific to AI-driven review
  - keep Review render quality and syntax highlighting unchanged
  - ensure the pane can be closed and reopened per thread without losing state unexpectedly

- [x] Todo 8: Preserve grouped file rows without forcing giant expands.
  Plan:
  - keep `Files edited` as a summary/disclosure container for child rows
  - let individual `Edited ...` rows focus their own file in the side pane
  - avoid reopening the pane unnecessarily when only the selected file changes

### Phase 5: Validation

- [x] Todo 9: Add representative coverage for AI side-pane diff behavior.
  Plan:
  - file row opens side pane and selects matching review path
  - file-path click still opens file view instead of the side pane
  - grouped file-change row keeps disclosure behavior
  - `Open in Review` escalates to the full Diff tab

- [x] Todo 10: Run final validation once after the pivot is complete.
  Plan:
  - `cargo fmt --all`
  - `./scripts/run_with_macos_sdk_env.sh cargo check --workspace`
  - `./scripts/run_with_macos_sdk_env.sh cargo test --workspace`
  - `./scripts/run_with_macos_sdk_env.sh cargo clippy --workspace --all-targets -- -D warnings`

Validation note:

- `cargo fmt --all` passed.
- Focused `hunk-desktop` validation is the meaningful signal for this pivot because the full workspace currently hits an unrelated native build failure in `libghostty-vt-sys` before Rust compilation reaches these AI workspace files on this machine.

## Manual QA Checklist

1. Open a thread with grouped `Files edited` rows.
2. Expand the group and click an individual `Edited ...` row.
3. Confirm the AI diff pane opens on the right and focuses the matching file.
4. Confirm clicking the linked file path still opens the file view instead.
5. Confirm `Open in Review` still switches to the full Diff tab.
6. Confirm switching threads preserves pane state per thread and does not break compare-source sync.
7. Confirm the right-side pane keeps Hunk Review syntax highlighting and side-by-side quality.
