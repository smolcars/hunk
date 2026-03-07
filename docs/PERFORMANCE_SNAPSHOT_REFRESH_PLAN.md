# Performance Snapshot Refresh Plan

Date: 2026-03-07
Owner: Codex
Status: Phases 1-3 Complete, Phase 4 Not Required
Scope: Reduce unnecessary snapshot and diff-refresh cost in the Git backend and desktop refresh controller without changing product behavior.

## Problem

The current Git refresh architecture is sound, but one important backend distinction is still missing:

- the desktop controller distinguishes between read-only background refreshes and full working-copy refreshes
- `hunk-git` currently does not honor that distinction in the hot workflow snapshot loaders
- as a result, the `*_without_refresh` workflow loaders still pay essentially the same cost as the regular loaders

This means the app is already fast, but it is leaving performance on the table in the exact path used by:

- polling / auto-refresh
- watcher-driven metadata refreshes
- cold-start stale-first snapshot loads

## Goals

1. Make read-only workflow snapshot loads materially cheaper than full working-copy refresh loads.
2. Preserve Review-tab correctness and fast first paint.
3. Preserve the existing desktop refresh policy and progressive diff architecture.
4. Avoid introducing persistent caches or a second backend architecture unless profiling proves they are needed.

## Non-Goals

1. No rewrite of the desktop refresh controller.
2. No persistent patch cache in this pass.
3. No change to user-visible Git tab behavior.
4. No speculative optimization of patch rendering before snapshot hot spots are addressed.

## Current Architecture

The desktop already has the right control surface:

- `SnapshotRefreshRequest::background()` for read-only refreshes
- `SnapshotRefreshRequest::background_refresh_working_copy()` for background working-copy refreshes
- `SnapshotRefreshRequest::user(force)` for user-initiated refreshes

Relevant code:

- refresh policy: [refresh_policy.rs](/Volumes/hulk/dev/projects/hunk/crates/hunk-desktop/src/app/refresh_policy.rs)
- snapshot controller: [core.rs](/Volumes/hulk/dev/projects/hunk/crates/hunk-desktop/src/app/controller/core.rs)
- watcher / polling: [core_runtime.rs](/Volumes/hulk/dev/projects/hunk/crates/hunk-desktop/src/app/controller/core_runtime.rs)
- Git snapshot backend: [git.rs](/Volumes/hulk/dev/projects/hunk/crates/hunk-git/src/git.rs)

The gap is backend-side: the workflow `*_without_refresh` loaders still route through the same snapshot pipeline and still read full worktree content to build content-sensitive signatures.

## Plan

### Phase 1: Real Lightweight Workflow Snapshot Path

Goal:

- make workflow `*_without_refresh` calls cheaper without changing desktop controller APIs

Changes:

1. Introduce an explicit snapshot load mode in `hunk-git`
   - `ReadOnlyLight`
   - `RefreshWorkingCopy`

2. Split workflow snapshot construction from patch rendering
   - workflow snapshot path should not require full worktree byte loading
   - patch rendering stays on the existing heavy path

3. For `ReadOnlyLight` workflow snapshots:
   - keep branch/upstream state
   - keep changed-file enumeration
   - keep rename detection
   - compute a lightweight per-file signature from:
     - file status
     - rename source
     - `HEAD` object identity for the tracked side
     - worktree metadata signature for the current side
   - do not read full worktree bytes just to build the workflow fingerprint

4. Apply the light path to:
   - `load_workflow_snapshot_without_refresh`
   - `load_workflow_snapshot_with_fingerprint_without_refresh`
   - `load_workflow_snapshot_if_changed_without_refresh`
   - `load_snapshot_fingerprint_without_refresh`

5. Keep the full path for:
   - `load_workflow_snapshot`
   - `load_workflow_snapshot_if_changed`
   - patch loading
   - line-stat loading

Expected outcome:

- background polling and metadata-only watcher refreshes stop hashing full changed-file contents
- diff reload correctness is preserved because changed workflow signatures still invalidate when changed-file content or metadata moves

Validation:

- `cargo test -p hunk-git`
- `cargo build --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `./scripts/run_perf_harness.sh --no-gate`

Deep review gate:

- verify no false negative on same-status modified files
- verify rename snapshots still invalidate correctly
- verify no Review-tab reload regression

### Phase 2: Tighten Forced Refresh And Line-Stat Scope

Goal:

- stop user-forced refreshes from automatically implying full line-stat work when the diff state did not actually change

Changes:

1. Refine the controller decision in [core.rs](/Volumes/hulk/dev/projects/hunk/crates/hunk-desktop/src/app/controller/core.rs)
   - if file list and working-copy identity are unchanged, preserve existing line stats
   - only run full line-stat refresh when file-level diff state actually changed
   - prefer path-scoped line-stat refresh whenever `pending_dirty_paths` is available

2. Keep the top-of-app line counter stable during no-op refreshes

Expected outcome:

- fewer expensive full-stat passes on manual refreshes
- less post-refresh background work

Validation:

- workspace tests
- perf harness
- manual edit / save / refresh spot checks

Deep review gate:

- verify line counter stays correct after:
  - edit
  - undo
  - branch switch
  - publish / push / sync

### Phase 3: Add Bigger Perf Fixtures

Goal:

- find the next scaling bottleneck before making deeper architectural changes

Changes:

1. Extend the perf harness and/or fixture scripts with:
   - many-files-small-patches
   - rename-heavy diffs
   - binary-heavy diffs
   - ignored-tree pressure

2. Record benchmark baselines in [PERFORMANCE_BENCHMARK.md](/Volumes/hulk/dev/projects/hunk/docs/PERFORMANCE_BENCHMARK.md)

Expected outcome:

- clearer signal about whether the next bottleneck is:
  - snapshot enumeration
  - patch rendering
  - line stats
  - syntax segmentation

### Phase 4: Targeted Follow-Up Optimization

Only do this after Phase 3 profiling.

Possible candidates:

1. cache or reuse more `HEAD` tree lookup state during snapshot loads
2. split large-repo diff reload into smaller batches when changed-file count is extreme
3. add optional memoization for per-file line stats keyed by file signature

This phase is conditional. Do not start it unless profiling proves the need.

## Complexity Assessment

This is not a giant rewrite.

Expected size by phase:

- Phase 1: medium
- Phase 2: small to medium
- Phase 3: small
- Phase 4: medium, only if needed

The difficult part is correctness, not raw code volume. The controller already has the right refresh model. The missing work is mostly making the backend honor it.

## Current Status

- Plan written
- Phase 1 complete
- Phase 2 complete
- Phase 3 complete
- Phase 4 reviewed and closed with no additional product-path optimization required right now

## Phase 1 Result

Implemented in `crates/hunk-git`:

- workflow `*_without_refresh` loaders now use a real lightweight snapshot path instead of routing to the full resolver
- read-only light loads keep branch/upstream state, changed-file enumeration, and rename detection
- light loads avoid full worktree byte loading for ordinary unstaged changes and instead derive workflow signatures from:
  - file status
  - rename source
  - `HEAD` entry identity
  - worktree metadata signature
- staged/index-sensitive candidates stay on the full resolver so the app keeps hiding unsupported index-only state correctly
- full and light workflow snapshots now intentionally share the same fingerprint/working-copy identity model so the desktop controller can safely compare them across refresh modes without causing false reloads

Validation completed:

- `cargo test -p hunk-git`
- `cargo build --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `./scripts/run_perf_harness.sh --no-gate`

Current large-diff harness snapshot:

- `ttfd_ms=6.25`
- `selected_file_latency_ms=20.97`
- `full_stream_ms=194.07`
- `scroll_fps_p95=317.41`

## Phase 2 Result

Implemented in `crates/hunk-desktop`:

- no-op and forced refreshes now preserve cached per-file line stats when the diff state did not actually change
- background read-only refreshes still skip line-stat work entirely
- background dirty-path refreshes continue using path-scoped line-stat reloads
- missing cached line stats are filled in path-scoped form instead of forcing a full recompute

Validation completed:

- `cargo test -p hunk-desktop --test refresh_policy`
- `cargo build -p hunk-desktop`

Deep review findings and fixes:

- preserved the existing read-only background refresh behavior so polling stays cheap
- avoided a false optimization where unchanged diff state would have skipped filling missing cached line stats
- kept the top-level line counter derived from file-level stats rather than recomputing from scratch on no-op refreshes

## Phase 3 Result

Implemented in the benchmark tooling:

- scenario-aware fixture generation for:
  - `default`
  - `many-files-small-patches`
  - `rename-heavy`
  - `binary-heavy`
  - `ignored-tree-pressure`
- scenario-aware wrapper defaults in [run_perf_harness.sh](/Volumes/hulk/dev/projects/hunk/scripts/run_perf_harness.sh)
- harness support for scenario parsing, binary-aware threshold behavior, and safer selected-file selection in [performance_harness.rs](/Volumes/hulk/dev/projects/hunk/crates/hunk-desktop/tests/performance_harness.rs)
- standalone harness fixture creation now resolves the workspace `scripts/` path correctly instead of depending on the wrapper

Validation completed:

- `bash -n scripts/create_large_diff_repo.sh`
- `bash -n scripts/run_perf_harness.sh`
- `cargo test -p hunk-desktop --test performance_harness`
- multi-scenario perf sweep with `--no-gate`

Deep review findings and fixes:

- normalized `default` as the canonical baseline scenario name while still accepting `large-diff` as an alias
- fixed the standalone harness script lookup path
- made selected-file latency prefer non-deleted text entries so `binary-heavy` and `rename-heavy` runs are less misleading
- documented two remaining harness caveats:
  - the synthetic scroll-FPS proxy can saturate on very small fixtures and should be treated as a coarse regression signal, not a literal UI FPS measurement
  - the `rename-heavy` fixture currently behaves as path churn in the unstaged worktree benchmark rather than a fully collapsed rename-only view at large scale

See the benchmark baseline table in [PERFORMANCE_BENCHMARK.md](/Volumes/hulk/dev/projects/hunk/docs/PERFORMANCE_BENCHMARK.md).

## Phase 4 Decision

Phase 4 was reviewed after the broader benchmark sweep.

Decision:

- no additional product-path optimization is required right now

Reasoning:

- the default large-diff path remains comfortably inside the current thresholds
- Phase 1 removed the unnecessary heavy backend work on read-only refreshes
- Phase 2 stopped unnecessary line-stat recomputation after no-op refreshes
- the broader fixtures did not reveal a new backend bottleneck severe enough to justify deeper caching or a second snapshot architecture

Remaining watch items:

- keep an eye on very large changed-file counts beyond the current fixture set
- revisit line-stat memoization or deeper diff batching only if a future profile shows a concrete regression
