# AI Chat Timeline V2 TODO

## Status
- Owner: Hunk
- Start date: 2026-03-05
- Goal: Replace the current turn-card timeline UI with a chat-style conversation timeline while preserving high performance.

## Non-Negotiable Quality Gates (every phase)
- [ ] Implement only scoped phase changes.
- [ ] Run targeted tests/checks for changed crates.
- [ ] Conduct a thorough code review before phase close.
- [ ] Fix correctness, architecture, and performance issues found in review.
- [ ] Ensure no new file exceeds 1000 lines.

## Performance SLOs
- [ ] Keep virtualized timeline rendering (`ListState`) for all conversation rows.
- [ ] Avoid per-frame string cloning for message/tool content.
- [ ] Keep follow-output behavior stable during streaming turns.
- [ ] No regression in AI tab responsiveness with large thread histories.

## Phase 0: Baseline + Design Contract
- [x] Document visual target from desktop screenshots:
  - back-and-forth chat bubbles
  - inline tool-call activity rows
  - hover chevron to expand tool details
  - consistent light/dark styling
- [x] Lock implementation direction:
  - flatten rows by sequence
  - keep turn pagination behavior
  - preserve current diff actions ("View Diff")
- [x] Add this execution plan to docs.

### Phase 0 Code Review
- [x] Reviewed plan for scope clarity and phase ordering.
- [x] Verified no dependency on transport/protocol changes outside current architecture.

## Phase 1: Data Foundation for Expandable Tool Rows (Completed)
- [x] Extend `hunk-codex` item state with optional structured display metadata for timeline rendering.
- [x] Capture metadata from `ThreadItem` snapshots in notification ingestion.
- [x] Cap metadata payload size to avoid memory blowups.
- [x] Add reducer tests for metadata lifecycle (start/delta/completed/out-of-order safety).

### Planned Files (Phase 1)
- `crates/hunk-codex/src/state.rs`
- `crates/hunk-codex/src/threads/helpers.rs`
- `crates/hunk-codex/src/threads/notifications.rs`
- `crates/hunk-codex/tests/*` (as needed)

### Phase 1 Code Review Checklist
- [x] Verify stale events cannot overwrite newer metadata.
- [x] Verify metadata updates never mutate thread/turn associations.
- [x] Verify metadata truncation is UTF-8 safe.
- [x] Verify no duplicate large payload copies are introduced.

### Phase 1 Validation Notes
- [x] `cargo test -p hunk-codex`
- [x] `cargo test -p hunk-desktop --tests`
- [x] `cargo clippy -p hunk-codex -p hunk-desktop --all-targets -- -D warnings`
- [x] `cargo check --workspace`
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`

## Phase 2: Chat Row Index in Desktop Controller (Completed)
- [x] Build flattened conversation row index keyed by thread.
- [x] Keep current turn-limit pagination by filtering rows from visible turns.
- [x] Preserve scroll-follow and scroll-to-bottom correctness.
- [x] Add unit tests for ordering and pagination.

### Phase 2 Code Review Checklist
- [x] Verified row ordering remains stable with sequence + deterministic tie-breakers.
- [x] Removed obsolete per-item expansion/index state to prevent stale state drift.
- [x] Confirmed pagination applies by visible turns first, then row filtering.
- [x] Confirmed expansion state is pruned to existing row IDs on snapshot updates.

### Phase 2 Validation Notes
- [x] `cargo test -p hunk-desktop --tests`
- [x] `cargo clippy -p hunk-desktop -p hunk-codex --all-targets -- -D warnings`

## Phase 3: Chat-Style Timeline Rendering (Completed)
- [x] Replace turn-card rendering with row-based chat rendering.
- [x] Role-specific row UI:
  - user bubble (right)
  - assistant bubble (left)
  - tool/system inline status rows
- [x] Keep diff row affordance and existing actions.
- [x] Add hover chevron + expandable tool detail panel.

### Phase 3 Code Review Checklist
- [x] Reviewed row render path for empty/missing item safety.
- [x] Reviewed payload rendering to avoid unbounded metadata growth (metadata already capped in codex state).
- [x] Eliminated duplicate/obsolete preview rendering behavior in expanded tool rows.
- [x] Reduced per-frame cloning by switching timeline row/turn accessors to borrowed references.

### Phase 3 Validation Notes
- [x] `cargo test -p hunk-desktop --tests`
- [x] `cargo clippy -p hunk-desktop -p hunk-codex --all-targets -- -D warnings`

## Phase 4: Interaction + Visual Polish (Completed)
- [x] Fine-tune typography, spacing, and light/dark palette parity.
- [x] Add minimal intentional motion for row appearance.
- [x] Validate composer alignment and thread switch transitions.

### Phase 4 Code Review Checklist
- [x] Verified bubble/tool row contrast remains readable in light and dark themes.
- [x] Verified row entrance animation respects reduced-motion preference.
- [x] Verified thread switches clear expansion state and avoid stale expanded panels.

### Phase 4 Validation Notes
- [x] `cargo test -p hunk-desktop --tests`
- [x] `cargo clippy -p hunk-desktop -p hunk-codex --all-targets -- -D warnings`

## Phase 5: Perf + QA Hardening
- [x] Add large-thread performance harness for AI timeline.
- [ ] Validate memory profile with expanded/collapsed tool payloads.
- [x] Run workspace gates:
  - `cargo check --workspace`
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`

### Phase 5 Code Review Checklist
- [x] Added ignored perf harness test for row-index filtering with configurable thresholds.
- [x] Confirmed row pagination/filtering helpers remain linear with deterministic output.
- [ ] Capture and review explicit memory profile for expanded/collapsed large metadata payloads.

### Phase 5 Validation Notes
- [x] `cargo test -p hunk-desktop ai_timeline_visible_row_index_perf_harness -- --ignored --nocapture`
- [x] `cargo check --workspace`
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`

## Rollout
- [ ] Guard new timeline behind `ai_chat_timeline_v2` flag.
- [ ] Dogfood and compare against legacy timeline.
- [ ] Remove legacy path after parity and perf sign-off.
