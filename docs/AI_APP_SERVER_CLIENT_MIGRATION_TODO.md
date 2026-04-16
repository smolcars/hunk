# AI App Server Client Migration TODO

## Status
- Owner: Hunk
- Start date: 2026-04-16
- Goal: Replace Hunk's custom Codex host + WebSocket worker path with an upstream-style app-server client architecture that is more stable, easier to reason about, and able to support both remote and embedded transports.
- Current internal transport flag: `HUNK_CODEX_APP_SERVER_TRANSPORT=auto|embedded|remote`
- Latest automated validation (2026-04-16): `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`

## Current Implementation Snapshot
- [x] Hunk now has a transport-agnostic app-server client boundary in `hunk-codex`.
- [x] `RemoteBundled` now runs on the new upstream-style client boundary.
- [x] `Embedded` transport selection is wired through the same boundary and fails fast with a clear unsupported error in the current desktop workspace build.
- [x] `auto` transport selection now skips embedded when the current build cannot link it and falls back directly to `RemoteBundled`.
- [x] Desktop AI runtime now consumes app-server events instead of polling raw WebSocket session queues.
- [x] Stall detection and automatic recovery are implemented.
- [x] Completed thread snapshots no longer reopen finished turns during item hydration, which removes a major source of false-positive stall recovery.
- [x] Reconnect-time `thread/resume` / `thread/read` snapshots now replace stale same-item agent content, so restarted hosts do not keep showing old streamed text.
- [x] Automated coverage now exercises reconnect-time snapshot replacement, background workspace disconnect/fatal cleanup, and thread-selection behavior while another thread is streaming.
- [ ] A true in-process `Embedded` client is still blocked by upstream `sqlx/sqlite` linkage conflicting with Hunk's `rusqlite` linkage inside the desktop binary.
- [ ] Default transport is still pending a longer soak period and measurement.
- [ ] Manual soak validation, perf comparisons, and legacy transport cleanup are still pending.

## Why This Exists
- [ ] Hunk currently launches a bundled `codex app-server`, connects over loopback WebSocket, polls for notifications, and emits full AI snapshots into the UI worker path.
- [ ] Codex CLI/TUI already use a more robust app-server client model with bounded queues, explicit event semantics, and an embedded transport by default.
- [ ] Hunk should move toward the upstream model instead of continuing to grow custom transport/runtime logic.
- [ ] The migration must preserve current Hunk AI workspace behavior while reducing streaming stalls and disconnect sensitivity.

## Scope Lock (Non-Negotiable)
- [x] Hunk adopts an upstream-style app-server client boundary before attempting a full embedded-only rewrite.
- [x] Phase 1 keeps the current Hunk AI UI/state model intact; no timeline/composer rewrite is allowed in that phase.
- [x] Remote bundled-runtime support remains available until embedded mode is proven stable in Hunk.
- [ ] Upstream MIT code may be copied or vendored, but Hunk-specific changes must stay in Hunk-owned adapter layers whenever possible.
- [ ] Keep vendored/upstream diffs as small as possible.
- [ ] Do not introduce a third transport model beyond `RemoteBundled` and `Embedded`.
- [ ] Production Git behavior remains in `crates/hunk-git`; do not shell out from app code.
- [ ] Tests are required in every phase.

## Target Architecture

### Core Direction
- [x] Introduce a transport-agnostic Hunk app-server client interface used by the AI worker.
- [ ] Support two concrete transports in the shipped desktop build:
  - [x] `RemoteBundled`: bundled Codex runtime started by Hunk, connected through upstream-style remote app-server client plumbing.
  - [ ] `Embedded`: true in-process app server client path modeled after upstream `InProcessAppServerClient` is still blocked by sqlite linkage.
- [x] Make Hunk's AI worker consume app-server events rather than directly polling `JsonRpcSession`.
- [x] Preserve Hunk's `ThreadService` and `AiState` reducer initially so desktop UI integration stays stable during migration.

### Desired End State
- [x] Hunk AI workspace controller depends on a Hunk-owned app-server client adapter, not on `ws_client.rs`.
- [x] Remote and embedded transports share the same worker/event handling code.
- [x] Critical stream events use explicit lossless handling semantics.
- [x] Hunk can choose transport by config/flag during rollout and dogfooding.
- [ ] Embedded becomes the default only after soak testing proves parity or better stability.

## Design Constraints
- [ ] Keep files under 1000 lines; split early when adding adapter/client modules.
- [ ] Prefer direct reuse of upstream client code over reimplementing equivalent queue/event logic.
- [ ] Do not couple desktop GPUI render code to transport details.
- [ ] Preserve current approval, user-input, review-mode, and workspace-thread behaviors during the transport migration.
- [ ] Preserve shared `~/.codex` continuity where the upstream transport supports it.

## Non-Negotiable Quality Gates (Every Phase)
- [ ] Implement only scoped phase changes.
- [ ] Run targeted tests/checks for changed crates.
- [ ] Conduct a deep code review before phase close.
- [ ] Fix correctness, architecture, and performance issues found in review.
- [ ] Ensure no new file exceeds 1000 lines.

## Performance and Stability SLOs
- [ ] Reduce transport-related failure surfaces relative to the current host + WebSocket worker path.
- [ ] Eliminate idle polling loops where the upstream event-driven client already provides direct event delivery.
- [ ] Avoid additional full-state cloning beyond the current baseline during Phase 1.
- [ ] Maintain smooth streaming behavior for long-running turns.
- [ ] Preserve or improve desktop responsiveness during streaming.
- [ ] Keep frame work below Hunk's 8ms target in normal AI workspace usage.

## Phase 0: Architecture Contract and Baseline
- [x] Add this migration TODO to `docs/`.
- [ ] Freeze the target architecture:
  - [x] Hunk-owned app-server client interface.
  - [x] `RemoteBundled` transport first.
  - [x] `Embedded` transport second.
  - [x] same worker path for both.
- [ ] Record which current modules are expected to be replaced, adapted, or removed:
  - [x] `crates/hunk-codex/src/ws_client.rs`
  - [x] parts of `crates/hunk-codex/src/host.rs`
  - [x] `crates/hunk-desktop/src/app/ai_runtime/*` polling/reconnect paths
- [ ] Record which current modules are expected to remain initially:
  - [x] `ThreadService`
  - [x] `AiState`
  - [x] desktop AI snapshot application path
- [ ] Define transport selection strategy for rollout:
  - [x] internal config flag
  - [x] fallback behavior
  - [ ] logging/telemetry tags
- [x] Capture a baseline of current failure symptoms and expected recovery behavior.

### Phase 0 Code Review
- [ ] Verify phase ordering avoids a desktop UI rewrite in the transport migration.
- [ ] Verify the migration boundary is transport-focused rather than protocol-surface-focused.
- [ ] Verify the plan allows rollback to current behavior until parity is proven.

## Phase 1: Hunk App-Server Client Boundary
- [x] Create a Hunk-owned client adapter module or crate.
- [ ] Define the minimal interface needed by the AI worker:
  - [x] `request_typed`
  - [x] `notify`
  - [x] `next_event`
  - [x] `resolve_server_request`
  - [x] `reject_server_request`
  - [x] `shutdown`
- [x] Add Hunk-owned event/request wrapper types only where required.
- [x] Refactor the AI worker to depend on this interface instead of directly depending on `JsonRpcSession`.
- [x] Keep `ThreadService` as the reducer/application layer for app-server notifications and requests.
- [x] Keep desktop-facing snapshot emission unchanged in this phase.

### Planned Files (Phase 1)
- `crates/hunk-codex/src/app_server_client/*`
- `crates/hunk-desktop/src/app/ai_runtime/core.rs`
- `crates/hunk-desktop/src/app/ai_runtime/sync.rs`
- `crates/hunk-desktop/src/app/ai_runtime/reconnect.rs`
- crate-level tests in `crates/hunk-codex/tests` and `crates/hunk-desktop/tests`

### Phase 1 Code Review Checklist
- [x] Verify desktop AI runtime no longer depends on raw WebSocket session details.
- [x] Verify the client boundary is transport-agnostic.
- [x] Verify no behavior regressions in approvals, user-input, or review mode.
- [x] Verify the adapter API is small enough to swap implementations cleanly.

## Phase 2: Remote Bundled Transport on Upstream Client Model
- [x] Vendor or copy the upstream remote app-server client implementation patterns into Hunk.
- [x] Replace Hunk's custom WebSocket request/event plumbing with the new remote client path.
- [x] Preserve current bundled runtime startup and shutdown logic as needed.
- [x] Replace polling-driven worker/event bridging where upstream remote event delivery already provides a cleaner path.
- [x] Preserve current thread hydration behavior using `thread/resume` and `thread/read` where needed.
- [ ] Add explicit handling for:
  - [x] disconnect events
  - [x] lag/backpressure signals
  - [x] dropped server-request safety
- [ ] Remove or deprecate now-redundant custom transport code once the new path is stable.

### Phase 2 Code Review Checklist
- [x] Verify no duplicate event streams are active at once.
- [x] Verify disconnect behavior is explicit and testable.
- [x] Verify queue/backpressure behavior cannot silently corrupt transcript state.
- [ ] Verify the worker does not regress into hidden polling loops around an event-driven client.

## Phase 3: Recovery, Stall Detection, and Health Checks
- [x] Add stall detection for `InProgress` turns that stop receiving meaningful stream updates.
- [ ] Define a recovery policy:
  - [x] re-read thread state
  - [x] attempt `thread/resume`
  - [x] reconnect transport if needed
  - [x] surface clear UI status
- [ ] Add stronger structured logging around:
  - [x] reconnect attempts
  - [x] resume attempts
  - [x] listener reattachment
  - [x] stall durations
- [x] Ensure recovery is automatic before asking the user to manually type another prompt.
- [x] Keep current Hunk snapshot/UI model but improve self-healing behavior.

### Phase 3 Code Review Checklist
- [x] Verify no infinite reconnect/resume loops.
- [ ] Verify recovery does not duplicate user turns or replay unsafe actions.
- [x] Verify silent stalls become observable in logs and test fixtures.
- [x] Verify background/hidden AI workspaces recover correctly.

## Phase 4: Embedded Transport
- [x] Add embedded transport selection and a shared app-server client surface for future in-process integration.
- [x] Reuse the same Hunk app-server client interface from Phase 1.
- [x] Add transport selection by internal config/flag.
- [x] Make `auto` skip embedded when the current build cannot link it.
- [x] Make explicit `embedded` preference fail fast with a clear unsupported error.
- [ ] Isolate upstream sqlite/sqlx linkage so a true in-process `Embedded` client can be compiled into the desktop workspace.
- [ ] Support side-by-side validation:
  - [x] `RemoteBundled`
  - [ ] `Embedded`
- [ ] Ensure embedded mode preserves:
  - [ ] approvals
  - [ ] user inputs
  - [ ] thread resume/fork/read behavior
  - [ ] review mode
  - [ ] shared AI workspace UX
- [ ] Document packaging/runtime implications for desktop.

### Phase 4 Code Review Checklist
- [ ] Verify a future embedded mode does not couple GPUI UI code directly to upstream internals.
- [ ] Verify desktop shutdown is clean and bounded.
- [ ] Verify no duplicate background runtimes remain alive after workspace switches.
- [x] Verify feature-flagged transport switching is deterministic and debuggable.

## Phase 5: Make Embedded Default and Remove Old Path
- [ ] Soak test embedded mode with long-running agent sessions.
- [ ] Compare stability against `RemoteBundled`.
- [ ] Compare CPU wakeups, memory churn, and startup latency.
- [ ] Make embedded the default only after parity or better is proven.
- [ ] Keep remote fallback until at least one full release cycle of dogfooding succeeds.
- [ ] Remove obsolete custom transport/runtime code:
  - [ ] custom WebSocket session
  - [ ] transport-specific polling paths
  - [ ] unused host orchestration code
- [ ] Update docs that still describe WebSocket-only architecture as the long-term direction.

### Phase 5 Code Review Checklist
- [ ] Verify the old path is fully dead or intentionally retained as fallback.
- [ ] Verify code deletion does not remove needed bundled-runtime fallback behavior.
- [ ] Verify transport default selection is documented and test covered.
- [ ] Verify no stale TODO/spec docs contradict the new architecture.

## Validation Matrix

### Required Automated Coverage
- [x] Unit tests for the Hunk app-server client adapter.
- [x] Unit tests for event ordering and backpressure handling.
- [x] Unit tests for disconnect and lag event handling.
- [ ] Integration tests for:
  - [x] active streaming turn survives transport reconnect
  - [x] `thread/resume` reattaches and streaming continues
  - [x] approval requests still resolve correctly
  - [x] user-input requests still resolve correctly
  - [x] background workspace runtime remains correct
  - [x] thread switching during streaming remains correct

### Required Manual/Soak Validation
- [ ] Long-running AI thread soak test in Hunk.
- [ ] Repeated workspace switching during active streams.
- [ ] Simulated network/transport interruption.
- [ ] Simulated host restart in `RemoteBundled` mode.
- [ ] Embedded transport startup/shutdown stress test.
- [ ] Compare behavior against current manual "type continue to wake it up" failure mode.

### Workspace Gates Before Closing Major Phases
- [x] `cargo check --workspace`
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo build --workspace`

## Open Questions
- [ ] Should Hunk vendor upstream app-server client code directly, or copy selected modules into `hunk-codex` and maintain them locally?
- [ ] Do we want to keep `RemoteBundled` as a permanent fallback for debugging and packaging edge cases?
- [ ] Is shared `~/.codex` continuity fully preserved in embedded mode for Hunk's expected workflows?
- [ ] Do we need a visible developer toggle in the desktop UI for transport selection, or is config/env enough?
- [ ] Which metrics/log fields should be added to compare remote vs embedded failure rates?

## Rollout
- [x] Add a hidden/internal transport selection flag for dogfooding.
- [ ] Dogfood `RemoteBundled` on the new upstream-style client path first.
- [ ] Dogfood `Embedded` second.
- [ ] Collect stability and performance observations before changing defaults.
- [ ] Remove or archive the older WebSocket-only transport plan after this migration becomes authoritative.

## Authoritative Note
- [x] This document is intended to supersede the older long-term direction in `docs/AI_CODEX_WEBSOCKET_TODO.md` once implementation starts.
