# Diff Performance TODO

## Context

Large-diff benchmarks show a major language-dependent performance gap:

- Plain text (`--lang txt`) can sustain near `120 FPS` on very large diffs.
- TypeScript (`--lang ts`) drops to roughly `~70 FPS` on equivalent load.

Repro scripts:

```bash
./scripts/create_large_diff_repo.sh --lines 10000 --files 50 --lang ts --force
./scripts/create_large_diff_repo.sh --lines 10000 --files 50 --force
```

## Current Issues

1. Scrolling/render FPS on large TypeScript diffs does not meet the `120 FPS` target.
2. Initial diff load is blocked on full stream build; first visible diffs appear too late.
3. Loading path repeats expensive patch/materialization work across files.
4. Syntax + intra-line highlighting for TS is eager and heavy for all rows.
5. Render path creates many per-segment UI elements and string clones per frame.

## Goals

1. Sustain `120 FPS` on high-refresh displays (`>=115 FPS p95`) for large TypeScript diffs.
2. Show first meaningful diff content quickly (target: `<=300 ms` to first diff rows).
3. Keep app responsive while remaining files continue loading in background.
4. Establish regression-proof perf measurement for future changes.

## Success Metrics

- Time to first diff content (`TTFD`) <= `300 ms`.
- Time to selected file visible content <= `800 ms`.
- P95 scroll FPS >= `115` on `50 x 10k` TypeScript fixture.
- No input jank while background diff processing is active.

## Implementation Plan

### Phase 1: Fast First Paint + Progressive Loading

- [x] Render immediate lightweight diff skeleton (loading placeholder row on cold load).
- [x] Load selected file (or first file) first and display it as soon as ready.
- [x] Continue loading remaining files in background and progressively replace placeholders.
- [x] Add perf logs for stage timings (selected-file stage vs full-stream stage).

### Phase 2: Remove Backend Duplication

- [x] Avoid per-file re-materialization scans when loading patches.
- [x] Build a single per-refresh patch map for expanded files (`path -> patch`).
- [x] Avoid patch-string generation in snapshot line-stats computation (stats-only path).
- [x] Reuse one shared diff-entry cache across staged diff-stream loads in the same refresh.

### Phase 3: Syntax/Highlight Budgeting

- [x] Compute syntax + intra-line highlight lazily for visible/prefetched rows only.
- [x] Add complexity guards (token/line thresholds) for changed-pair LCS work.
- [x] Add fallback highlight mode under load (coarse token classes / changed-line only).

### Phase 4: Render Path Optimizations

- [x] Reduce per-row/per-segment allocations and `String` cloning in hot render path.
- [x] Replace map-based row segment lookup with index-oriented structures.
- [x] Reduce element count for highly segmented lines when under pressure.

### Phase 5: Perf Regression Safety Net

- [x] Add repeatable perf harness for large-diff fixtures.
- [x] Track and gate metrics for `TTFD`, selected-file latency, and scroll FPS.
- [x] Document benchmark protocol and acceptable thresholds.

## Work Log

- [x] Created this performance plan and aligned goals to measured issues.
- [x] Started implementation work.
- [x] Implemented two-stage diff loading (selected file first, full stream second).
- [x] Added stage timing logs for initial/full diff stream load.
- [x] Removed expensive full row-text hashing from stable row IDs.
- [x] Added adaptive coarse segment mode for very large per-file diffs.
- [x] Added immediate loading placeholder row to avoid blank initial diff pane.
- [x] Refactored diff loading to a single-pass patch-map backend path (no per-file JJ scan).
- [x] Switched snapshot line-stats to stats-only diff processing (no patch text assembly).
- [x] Switched cached segment text storage to `SharedString` to reduce render-path clones.
- [x] Replaced per-frame sticky header reverse scans with precomputed row lookups.
- [x] Switched diff row segment cache from stable-id map lookup to row-indexed lookup.
- [x] Added visible-row lazy segment prefetch with coarse render fallback on cache miss.
- [x] Added segment compaction budget to cap per-cell render element count.
- [x] Added release-mode large-diff perf harness test + script with threshold gating.
- [x] Documented benchmark protocol, metric definitions, and default thresholds.
- [x] Hardened perf harness gating (`scroll_fps_p95`, strict threshold parsing, invalid input fallback).
- [x] Made snapshot-loaded diff refresh unconditional to avoid stale diff rows.
- [x] Made batch patch loading resilient to per-entry render failures and duplicate-path overwrites.
- [x] Added progressive per-file loading placeholders and staged background diff replacement.
- [x] Switched patch entry collection to path-filtered materialized diff streams for staged loads.
- [ ] Add side-pane first-content metric for Git workspace cold-start instrumentation.
