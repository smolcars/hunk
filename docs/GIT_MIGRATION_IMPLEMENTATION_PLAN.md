# Git Migration Implementation Plan

Date: 2026-03-07
Owner: Codex
Status: Proposed
Scope: Full app pivot from JJ-backed workflows to Git-backed workflows, with a hard breaking change and no backwards compatibility.

## Decision

The staged `hunk-git` plan makes sense.

It is the lowest-risk way to pivot the app without throwing away the performance work already built into snapshotting, background refreshes, patch sessions, line-stat loading, and progressive diff rendering.

The important constraint is this:

- Do not design a generic multi-backend abstraction first.
- Keep the current desktop refresh architecture.
- Add `hunk-git` as a parallel backend crate.
- Move the UI to `hunk-git` incrementally.
- Delete `hunk-jj` only after the UI no longer imports it.

This keeps the migration focused on replacing the repository backend, not rewriting the app.

## Goals

1. Replace JJ as the backing repository engine with Git.
2. Keep `Files` and `Review` fast, smooth, and background-driven.
3. Ship a simpler Git tab focused on branches, commits, push, pull/sync, and PR/MR actions.
4. Remove all revision-stack behavior from the Git tab for v1.
5. Preserve or improve current large-diff performance characteristics.

## Non-Goals

1. No backwards compatibility for JJ state, JJ cache layout, or JJ comment scoping.
2. No feature flag gating.
3. No attempt to preserve JJ-native concepts like bookmarks, working-copy revisions, or operation history.
4. No revision-stack UI or stack mutation flows in the new Git tab.
5. No generic VCS abstraction layer before the Git migration lands.

## Product Scope For Git v1

### Files tab

- Repository tree
- File editor
- Changed-file list
- Top-of-app line change counter

### Review tab

- Current workspace diff against `HEAD`
- Fast initial diff paint
- Background loading for remaining diff content
- Line stats and sticky headers
- Existing comments workflow, re-scoped to Git branch/ref terminology

### Git tab

- Current branch
- Local branches
- Create branch
- Switch branch
- Rename current branch
- Commit current changes
- Publish branch (`push --set-upstream`)
- Push branch
- Sync branch with a simple policy
- Open/copy PR/MR URL

### Explicitly removed from Git v1

- Revision stack panel
- Squash tip into parent
- Reorder tip older
- Abandon tip revision
- JJ undo/redo operation history
- JJ glossary
- Bookmark dirty-switch snapshot/recovery flow

## Architectural Direction

### New crate

Add a new crate:

- `crates/hunk-git`

This crate should become the only Git backend imported by `hunk-desktop` once the migration is complete.

### Keep the current desktop architecture

Preserve these existing desktop behaviors and patterns:

- fingerprint-driven refresh decisions
- background snapshot loading
- background patch loading
- background line-stat loading
- progressive diff stream construction
- existing refresh policy logic

The migration should replace the backend feeding those systems, not replace those systems themselves.

### Do not mirror JJ surface area blindly

`hunk-git` should expose only the Git v1 surface that the UI actually needs. It should not replicate revision-stack types or JJ-specific operations.

### Temporary coexistence

During migration:

- `hunk-jj` stays in the workspace
- `hunk-git` is added beside it
- `hunk-desktop` switches imports from `hunk-jj` to `hunk-git` in phases

Only delete `hunk-jj` after all production imports are gone.

## Backend Contract For `hunk-git`

`hunk-git` should expose a flat façade similar to the current backend entrypoints.

Required Git v1 data types:

- `ChangedFile`
- `FileStatus`
- `LocalBranch`
- `LineStats`
- `RepoTreeEntry`
- `RepoTreeEntryKind`
- `RepoSnapshotFingerprint`
- `WorkflowSnapshot`
- `PatchSession`

Required Git v1 snapshot fields:

- `root`
- `head_commit_id`
- `branch_name`
- `branch_has_upstream`
- `branch_ahead_count`
- `branch_behind_count`
- `branches`
- `files`
- `line_stats`
- `last_commit_subject`

Deliberately excluded from the contract:

- `bookmark_revisions`
- `working_copy_commit_id`
- `can_undo_operation`
- `can_redo_operation`
- stack mutation helpers
- JJ bookmark activation semantics

## Git Engine Strategy

Use `gitoxide`/`gix` as the default implementation target for:

- repo discovery
- worktree status
- branch/ref inspection
- ahead/behind calculation
- diff generation
- patch loading
- tree walking
- remote URL resolution for PR/MR links

Allowed fallback for v1 if needed:

- `git2` for a narrowly scoped operation that `gix` cannot support yet

Reason:

- this migration should not stall on porcelain completeness
- production code should stay Rust-native and library-driven
- `gitoxide` is the primary backend target
- if one required write-path operation is not ready in `gix`, prefer a narrow `git2` fallback over shelling out

Hard rule:

- do not use the Git CLI in production app code
- use `gix` first
- use `git2` only for isolated gaps that are proven necessary
- if a fallback is introduced, document the exact missing `gix` capability and keep the fallback surface as small as possible

## Performance Guardrails

This migration must preserve the current performance-oriented architecture.

Mandatory rules:

1. Do not make diff rendering wait on full repository processing before first content appears.
2. Do not recompute full patch maps in hot UI paths.
3. Do not move expensive Git work onto the UI thread.
4. Do not regress row virtualization, sticky header lookup strategy, or lazy segment generation.
5. Do not introduce a backend API that forces synchronous whole-repo recomputation for common refreshes.

Perf gates for each relevant phase:

- run the existing performance harness in [docs/PERFORMANCE_BENCHMARK.md](/Volumes/hulk/dev/projects/hunk/docs/PERFORMANCE_BENCHMARK.md)
- compare against current main-branch behavior before accepting the phase
- investigate any regression in:
  - `ttfd_ms`
  - `selected_file_latency_ms`
  - `scroll_fps_avg`
  - `scroll_fps_p95`

Migration-specific perf targets:

1. Match or beat current `Review` tab TTFD on the standard fixture.
2. Match or beat current selected-file first-paint latency on the standard fixture.
3. Do not introduce visible UI jank during background snapshot refreshes.
4. Keep top-of-app line counter updates off the hot render path.

## Review Gate Template

Every phase ends with a mandatory deep code review before the next phase begins.

That review must include:

1. Correctness review
   - edge cases
   - stale state risks
   - branch/ref correctness
   - remote/upstream correctness
   - detached-head handling

2. Performance review
   - hot-path allocations
   - repeated repository loads
   - repeated diff generation
   - blocking work on the UI thread
   - unnecessary string cloning

3. Code quality review
   - dead code
   - duplicated logic
   - naming drift from Git terminology
   - file size growth
   - APIs that are broader than needed

4. Refactor review
   - simplify before adding more code
   - consolidate duplicate backend helpers
   - tighten types if the phase exposed weak modeling

Phase completion rule:

- A phase is not done when the feature appears to work.
- A phase is done only after the deep review is complete and the resulting fixes/refactors are applied.

## Phase Plan

### Phase 0: Contract and Perf Baseline

- [x] Create this migration plan and lock the Git v1 scope.
- [ ] Record current performance numbers for the large-diff harness.
- [ ] Record current refresh timing logs for snapshot load, line stats, and patch loading.
- [x] Identify all production `hunk-jj` imports and classify them as:
  - read path
  - diff path
  - write path
  - JJ-only
- [x] Define the initial `hunk-git` façade API.
- [ ] Deep code review (phase gate):
  - [ ] Verify scope does not accidentally preserve revision-stack behavior.
  - [x] Verify planned API contains only v1 Git features.
  - [ ] Verify performance baseline is captured before backend work starts.

### Phase 1: Create `hunk-git` Crate Skeleton

- [x] Add `crates/hunk-git` to the workspace.
- [x] Create the Git-native core types needed by desktop.
- [x] Add crate-level `tests/` scaffolding for backend integration tests.
- [x] Implement repo discovery and repository-open helpers.
- [x] Implement a minimal snapshot fingerprint shape.
- [x] Keep `hunk-desktop` untouched in this phase.
- [x] Deep code review (phase gate):
  - [x] Check type names and field names use Git terminology.
  - [x] Remove any accidental JJ noun leakage.
  - [x] Ensure the crate API is smaller than the current `hunk-jj` API.

### Phase 2: Implement Git Read Path Parity

- [x] Implement workflow snapshot loading in `hunk-git`.
- [x] Implement changed-file status loading.
- [x] Implement repo line stats and per-file line stats.
- [x] Implement repo tree loading.
- [x] Implement patch session and patch-map loading for changed files.
- [x] Implement snapshot fingerprint comparison for refresh decisions.
- [x] Implement branch/upstream/ahead-behind calculation.
- [ ] Add backend tests for:
  - [x] repo discovery
  - [x] clean repo snapshot
  - [x] dirty repo snapshot
  - [ ] renamed/deleted/untracked file handling
  - [x] ahead/behind behavior
  - [x] repo tree listing
  - [x] patch loading
- [x] Deep code review (phase gate):
  - [x] Verify repeated refreshes do not reopen/recompute more than necessary.
  - [x] Verify patch generation is not duplicated per file when one pass will do.
  - [x] Verify the line-counter path stays lightweight.
  - [x] Refactor any duplicated snapshot/diff helpers before UI adoption.

### Phase 3: Switch `Files` and `Review` To `hunk-git`

- [x] Change `hunk-desktop` imports so read-path snapshot, patch, tree, and line-stat loading come from `hunk-git`.
- [x] Keep the current refresh policy and background task scheduling.
- [x] Keep the current progressive diff-loading architecture.
- [x] Make the toolbar line-change counter use `hunk-git` line stats.
- [x] Remove JJ-specific user-facing copy from Files and Review surfaces.
- [ ] Do not switch Git-tab actions yet.
- [ ] Run the perf harness and compare to the Phase 0 baseline.
- [x] Deep code review (phase gate):
  - [x] Review all touched refresh paths for stale-state bugs.
  - [x] Review visible-row, prefetch, and sticky-header interactions for regressions.
  - [x] Review render hot paths for new allocations.
  - [x] Refactor any desktop/backend glue that became repetitive during the switch.

### Phase 4: Replace Git Tab With A Simpler Git Workflow

- [x] Rename `JjWorkspace` mode to `GitWorkspace`.
- [x] Remove JJ glossary and JJ-branded labels/tooltips from the live Git tab.
- [x] Remove the revision stack panel entirely.
- [x] Remove stack mutation actions from controllers and UI.
- [x] Replace bookmark wording with branch/ref wording across the tab.
- [ ] Keep the Git tab limited to:
  - [x] branch list
  - [x] branch switch
  - [x] branch create
  - [x] branch rename
  - [x] commit
  - [x] publish
  - [x] push
  - [x] sync
  - [x] PR/MR URL open/copy
- [ ] Simplify branch switching behavior:
  - [x] if working tree is dirty, block switch with explicit guidance
  - [x] do not replicate JJ snapshot/recovery flow in v1
- [x] Deep code review (phase gate):
  - [x] Verify all removed JJ flows are actually dead, not half-connected.
  - [x] Verify branch switching cannot silently discard user changes.
  - [x] Verify UI labels/tooltips consistently use Git terminology.
  - [x] Refactor controller logic to remove legacy JJ conditional paths.

### Phase 5: Implement Git Write Actions

- [x] Implement branch create/switch/rename in `hunk-git`.
- [x] Implement commit creation.
- [x] Implement publish as push with upstream configuration.
- [x] Implement push for the current branch.
- [x] Implement sync with a strict v1 policy:
  - [x] fetch
  - [x] fast-forward only update when safe
  - [x] explicit error for divergence
- [x] Implement review URL generation from Git remotes.
- [ ] Add backend tests for:
  - [x] create branch
  - [x] switch branch in clean repo
  - [x] rename branch
  - [x] commit changes
  - [x] publish branch
  - [x] push branch
  - [x] sync fast-forward only
  - [x] divergence error behavior
- [x] Deep code review (phase gate):
  - [x] Review ref update correctness.
  - [x] Review upstream selection correctness.
  - [x] Review push/sync failure handling and user messaging.
  - [x] Verify no Git CLI usage was introduced.
  - [x] Verify any non-`gix` fallback is isolated, justified, and documented.
  - [x] Refactor write-path helpers if any action logic duplicated transport/ref code.

### Phase 6: Migrate Desktop State and Comment Scoping

- [x] Remove JJ-specific cached workflow fields from `hunk-domain` state.
- [x] Replace cached revision-stack data with Git-native snapshot state only.
- [x] Rename comment scope fields from `bookmark_name` to `branch_name` or `ref_name`.
- [x] Update desktop comment scope logic to Git naming.
- [x] Hard-break old persisted state and old comment schema as needed.
- [ ] Add tests for:
  - [x] app-state load/save with new workflow cache shape
  - [x] comment scoping by branch/ref
  - [ ] detached-head comment scope behavior
- [x] Deep code review (phase gate):
  - [x] Review all remaining persisted-state names for JJ leakage.
  - [x] Review all comment queries for stale schema assumptions.
  - [x] Refactor migration-time compatibility code aggressively instead of carrying it forward.

### Phase 7: Delete `hunk-jj`

- [x] Remove all production imports of `hunk-jj`.
- [x] Remove `crates/hunk-jj` from the workspace.
- [x] Delete JJ-only tests, docs, and strings that no longer apply.
- [x] Remove JJ-specific controller and render code.
- [x] Rename any remaining legacy identifiers that still contain `jj` in live crates.
- [x] Deep code review (phase gate):
  - [x] Search the tree for remaining JJ terms and remove or isolate them to intentional migration-only docs.
  - [x] Review for dead helper methods and dead state fields left by the cutover.
  - [x] Refactor any large files that became awkward after deletion-heavy changes.

### Phase 8: Validation, Perf Signoff, and Release Readiness

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo build --workspace`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test`.
- [ ] Run the performance harness from [docs/PERFORMANCE_BENCHMARK.md](/Volumes/hulk/dev/projects/hunk/docs/PERFORMANCE_BENCHMARK.md).
- [ ] Perform manual QA for:
  - [ ] open project
  - [ ] refresh after file changes
  - [ ] diff first paint
  - [ ] large diff scrolling
  - [ ] branch switch/create/rename
  - [ ] commit
  - [ ] publish/push
  - [ ] sync success
  - [ ] sync divergence failure
  - [ ] PR/MR open/copy
  - [ ] comments in Review mode
- [ ] Deep code review (phase gate):
  - [ ] Re-read every touched file for correctness and maintainability.
  - [ ] Remove any temporary migration scaffolding that survived too long.
  - [ ] Confirm no file has grown beyond maintainable size.

## Acceptance Criteria

1. `Files`, `Review`, and `Git` tabs run entirely on Git-backed data and actions.
2. `Review` tab remains fast under large-diff load and passes the existing benchmark gates.
3. The top-of-app line-change counter remains accurate and responsive.
4. No revision-stack behavior remains in the Git tab.
5. No production code imports `hunk-jj`.
6. Workspace build, clippy, tests, and perf checks pass.

## Execution Notes

1. Prioritize the read path before the write path.
2. Prioritize performance before polish.
3. Keep Git v1 behavior narrow and explicit rather than clever.
4. Prefer blocking unsafe flows over shipping ambiguous state transitions.
5. Delete aggressively once a JJ path is no longer part of the plan.
6. Keep production Git integration Rust-native: `gix` first, `git2` only for narrow proven gaps, no CLI.
