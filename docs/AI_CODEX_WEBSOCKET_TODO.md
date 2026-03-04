# AI Coding in Hunk (Codex WebSocket-Only) TODO

## Scope Lock (Non-Negotiable)
- [ ] Transport is WebSocket only for Codex App Server integration.
- [ ] No stdio transport path in Hunk runtime logic.
- [ ] AI integration lives in its own crate: `crates/hunk-codex`.
- [ ] Hunk and Codex CLI share `~/.codex` for auth/threads/state continuity.
- [ ] Thread lists are scoped to current repository `cwd`.
- [ ] v1 includes ChatGPT login + dynamic tool call support.
- [ ] v1 includes explicit approvals (`accept`, `decline`) and a visible per-workspace "mad-max" mode.
- [ ] Unit tests and integration tests are required in every phase.

## Product Decisions (Locked)
- [ ] Pin Codex app-server integration to the then-current upstream `main` commit at implementation kickoff.
- [ ] Store and enforce "mad-max" as a per-workspace setting (not global and not per-thread).
- [ ] Use system-browser ChatGPT login flow with local callback handled by app-server.

## Phase Gate Policy (Applies To Every Phase)
A phase is complete only when all items below are done.

### Mandatory Engineering Gate
- [ ] Feature code for the phase is implemented.
- [ ] Unit tests added/updated in crate-level `tests` directories.
- [ ] Integration tests added/updated for end-to-end behavior.
- [ ] Workspace checks pass:
  - [ ] `cargo check --workspace`
  - [ ] `cargo test --workspace`
  - [ ] `cargo clippy --workspace --all-targets -- -D warnings`

### Mandatory Deep Code Review Gate (Before Next Phase)
- [ ] Deep review for correctness bugs (state races, ordering bugs, reconnect edge cases).
- [ ] Deep review for unsafe/fragile error handling (timeouts, retries, partial failures).
- [ ] Deep review for architecture quality (module boundaries, public API hygiene).
- [ ] Deep review for refactor opportunities (duplication, oversized functions, naming clarity).
- [ ] Deep review for test quality (coverage of unhappy paths and protocol edge cases).
- [ ] Review findings are fixed before phase close (no deferred known-critical issues).

---

## App Server API Coverage (Current Plan vs API Overview)

### v1 In Scope
- [ ] Core lifecycle: `initialize`, `initialized`.
- [ ] Threads: `thread/start`, `thread/resume`, `thread/fork`, `thread/read`, `thread/list`, `thread/loaded/list`, `thread/archive`, `thread/unarchive`, `thread/unsubscribe`, `thread/compact/start`, `thread/rollback`, `thread/status/changed` (notify).
- [ ] Turns: `turn/start`, `turn/steer`, `turn/interrupt`, `review/start`.
- [ ] Streaming item notifications and server-request/response loops.
- [ ] Approvals: `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, `serverRequest/resolved`.
- [ ] Tools: dynamic `item/tool/call` + `tool/requestUserInput`.
- [ ] Models and runtime metadata: `model/list`, `experimentalFeature/list`, `collaborationMode/list`.
- [ ] Skills/apps discovery for tool surfaces: `skills/list`, `skills/config/write`, `app/list`.
- [ ] Account/auth: `account/read`, `account/login/start`, `account/login/cancel`, `account/logout`, `account/rateLimits/read`, `account/updated` (notify), `account/login/completed` (notify).
- [ ] Utility execution: `command/exec`.

### Explicitly Deferred (Post-v1 unless required by blocking UX)
- [ ] MCP auth/config admin: `mcpServer/oauth/login`, `mcpServerStatus/list`, `config/mcpServer/reload`.
- [ ] Configuration authoring APIs: `config/read`, `config/value/write`, `config/batchWrite`, `configRequirements/read`.
- [ ] External agent migration: `externalAgentConfig/detect`, `externalAgentConfig/import`.
- [ ] Feedback endpoint: `feedback/upload`.
- [ ] Windows-specific bootstrap API: `windowsSandbox/setupStart` (included only if required by Windows packaging validation).

---

## Phase 0: Foundation and Contract Freeze
- [x] Create `docs/AI_CODEX_SPEC.md` with exact supported v1 API surface.
- [x] Freeze supported app-server methods/notifications used by Hunk.
- [x] Record pinned upstream codex commit SHA (from `main`) in spec + Cargo metadata.
- [x] Document WebSocket lifecycle contract (connect/init/reconnect/shutdown).
- [x] Document per-workspace "mad-max" behavior and warnings.
- [x] Define failure modes and UX states (offline, auth-needed, reconnecting, degraded).

### Required Tests
- [x] Protocol schema compatibility smoke test against pinned `codex-app-server-protocol`.
- [x] Deep phase review gate complete.
- Phase 0 review note (2026-03-03): v1 surface, transport constraints, and pinned upstream baseline are explicitly frozen in `AI_CODEX_SPEC.md`.

---

## Phase 1: New AI Crate Scaffolding (`crates/hunk-codex`)
- [x] Add crate with modules:
  - [x] `host` (embedded WebSocket app-server host process manager)
  - [x] `ws_client` (WS transport + JSON framing)
  - [x] `rpc` (request/response correlation + notification routing)
  - [x] `api` (typed requests)
  - [x] `state` (AI domain reducer)
  - [x] `errors` (typed integration errors)
- [x] Add crate-level test harness utilities.
- [x] Keep files below 1000 lines; split modules early.

### Required Tests
- [x] Unit tests for request id generation and correlation map behavior.
- [x] Unit tests for JSON serialization/deserialization of key messages.
- [x] Deep phase review gate complete.
- Phase 1 review note (2026-03-03): `hunk-codex` crate boundaries and test harness were validated; request-id correlation was revised to support both integer and string JSON-RPC ids.

---

## Phase 2: Embedded WebSocket Host Runtime
- [x] Implement host startup for bundled app-server in WebSocket listen mode.
- [x] Bind to loopback only (`127.0.0.1` / platform equivalent).
- [x] Allocate/verify free port selection and startup readiness checks.
- [x] Add lifecycle manager (`Starting`, `Ready`, `Reconnecting`, `Stopped`, `Failed`).
- [x] Capture and surface host stderr diagnostics into structured logs.
- [x] Implement clean shutdown and orphan-process guard.

### Required Tests
- [x] Integration test: host boots and accepts WebSocket client.
- [x] Integration test: reconnect after forced host restart.
- [x] Integration test: graceful shutdown leaves no running child process.
- [x] Deep phase review gate complete.
- Phase 2 review note (2026-03-03): host lifecycle manager now enforces loopback startup readiness, captures stderr diagnostics, and protects against orphan child processes on shutdown/drop.

---

## Phase 3: JSON-RPC Session + Initialization Handshake
- [x] Implement WebSocket client session loop.
- [x] Implement `initialize` request + `initialized` notification handshake.
- [x] Set `experimentalApi: true` capability.
- [x] Add support for notification opt-out list wiring (if we choose to use it).
- [x] Handle response/error routing and timeout cancellation.
- [x] Handle overloaded errors (`-32001`) with bounded retry/backoff.

### Required Tests
- [x] Integration test: handshake success path.
- [x] Integration test: request before initialize gets rejected and is surfaced.
- [x] Integration test: duplicate initialize handling.
- [x] Integration test: overloaded error retry policy.
- [x] Deep phase review gate complete.
- Phase 3 review note (2026-03-03): blocking JSON-RPC session now supports initialize handshake, timeout/error routing, and bounded overload retries; protocol edge cases are covered by dedicated integration tests.

---

## Phase 4: Core AI Domain State Reducer
- [x] Implement normalized state for threads/turns/items keyed by ids.
- [x] Implement reducers for:
  - [x] `thread/*`
  - [x] `turn/*`
  - [x] `item/started`
  - [x] `item/*/delta`
  - [x] `item/completed`
  - [x] `serverRequest/resolved`
- [x] Enforce deterministic ordering and idempotency for late/duplicate messages.
- [x] Add persistence hooks for last active thread per `cwd`.

### Required Tests
- [x] Unit tests for ordered stream application.
- [x] Unit tests for out-of-order and duplicate event handling.
- [x] Integration test: turn stream reaches correct final state.
- [x] Deep phase review gate complete.
- Phase 4 review note (2026-03-03): reducer state normalization, dedupe ordering, and active-thread persistence hooks were validated with deterministic stream and out-of-order coverage.

---

## Phase 5: Desktop Integration + AI Tab Skeleton
- [x] Add `WorkspaceViewMode::Ai` and `Cmd/Ctrl+4` shortcut.
- [x] Add footer/workspace switch UI for AI tab.
- [x] Add AI screen shell in `hunk-desktop` with split panes:
  - [x] thread list
  - [x] timeline
  - [x] composer
  - [x] status/header controls
- [x] Wire focus/keyboard contexts for AI pane interactions.

### Required Tests
- [x] UI integration test: mode switching does not break existing tabs.
- [x] Unit tests for AI view mode controller actions.

---

## Phase 6: Thread APIs + `cwd` Scoping
- [x] Implement `thread/list` with strict current-`cwd` filtering.
- [x] Implement `thread/start`, `thread/resume`, `thread/fork`, `thread/read`, `thread/loaded/list`.
- [x] Implement thread lifecycle methods: `thread/archive`, `thread/unarchive`, `thread/compact/start`, `thread/rollback`.
- [x] Implement thread subscription/unsubscription behavior and `thread/status/changed` handling.
- [x] Load last active thread for current repo from Hunk state.
- [x] Ensure continuity with Codex CLI through shared `~/.codex` threads.

### Required Tests
- [x] Integration test: listing only current `cwd` threads.
- [x] Integration test: resume thread created externally (simulated shared `.codex`).
- [x] Integration test: unsubscribe semantics and closed-thread status handling.
- [x] Integration test: archive and unarchive round-trip.
- [x] Integration test: compact-start streams and completion semantics.
- [x] Integration test: rollback updates thread state correctly.
- [x] Deep phase review gate complete.
- Phase 6 review note (2026-03-03): thread API wiring now enforces workspace-cwd boundaries, captures queued lifecycle notifications during in-flight requests, and syncs rollback snapshots by pruning removed turns/items to prevent stale timeline state.

---

## Phase 7: Turn APIs + Streaming Conversation UI
- [x] Implement `turn/start`, `turn/steer`, `turn/interrupt`.
- [x] Implement `review/start` and render review-mode lifecycle items.
- [x] Render streaming deltas for agent message and tool output.
- [x] Render `turn/diff/updated` and link to Hunk diff workflows.
- [x] Implement robust in-flight turn state transitions and cancellation UI.
- [x] Implement `command/exec` UI action for one-off command runs outside thread turns.

### Required Tests
- [x] Integration test: full turn stream from start to completion.
- [x] Integration test: interrupt mid-turn and final state correctness.
- [x] Integration test: review-start emits expected mode-entry/mode-exit items.
- [x] Integration test: command-exec success/error mapping.
- [x] Unit tests for delta accumulation correctness.
- [x] Deep phase review gate complete.
- Phase 7 review note (2026-03-03): desktop AI tab now runs against an embedded `hunk-codex` worker, renders live thread/turn/item state (including review-mode lifecycle markers and turn diff updates), supports turn interrupt + steer behavior, and exposes one-off `command/exec`; deep review tightened idle-notification polling and added explicit tests for review mode entry/exit and notification polling semantics.

---

## Phase 8: Approvals + Mad-Max Mode
- [x] Handle server requests:
  - [x] `item/commandExecution/requestApproval`
  - [x] `item/fileChange/requestApproval`
- [x] Build approval UI with explicit `accept`/`decline` actions.
- [x] Implement and surface `serverRequest/resolved` handling before item finalization.
- [x] Add explicit per-workspace "Mad Max" toggle with clear destructive warning UI.
- [x] Mad Max behavior:
  - [x] set approval policy to never
  - [x] set sandbox to danger-full-access
  - [x] auto-approve residual prompts safely by protocol contract

### Required Tests
- [x] Integration test: command approval happy path.
- [x] Integration test: file-change approval decline path.
- [x] Integration test: mad-max path does not block on approvals.
- [x] Unit tests for approval request lifecycle bookkeeping.
- [x] Deep phase review gate complete.
- Phase 8 review note (2026-03-03): WebSocket server-request capture now feeds a desktop approval queue with explicit accept/decline actions, `serverRequest/resolved` reducer updates, and per-workspace Mad Max persistence; mad-max mode now enforces `approvalPolicy: never` with `dangerFullAccess`, auto-accepts newly queued and residual pending approvals, and is covered by new command/file approval integration tests plus desktop/runtime policy unit tests.

---

## Phase 9: Dynamic Tool Calls + request_user_input
- [ ] Handle `item/tool/call` server request end-to-end.
- [ ] Add dynamic tool registry in `hunk-codex`.
- [ ] Implement v1 built-in tools needed for desktop workflow.
- [ ] Handle `item/tool/requestUserInput` with structured UI forms.
- [ ] Implement metadata discovery endpoints: `skills/list`, `skills/config/write`, `app/list`.
- [ ] Ensure tool failures are returned as structured responses (no panics).

### Required Tests
- [ ] Integration test: dynamic tool call request/response round-trip.
- [ ] Integration test: request-user-input round-trip and continuation.
- [ ] Unit tests for tool argument validation + serialization.

---

## Phase 10: Auth + Account + Rate Limits (v1)
- [ ] Implement `account/read`, `account/login/start`, `account/login/cancel`, `account/logout`.
- [ ] Implement ChatGPT login flow (open browser auth URL + callback completion handling).
- [ ] Render account state + auth-required banners in AI tab.
- [ ] Implement `account/rateLimits/read` + live update notifications.

### Required Tests
- [ ] Integration test: ChatGPT login lifecycle notifications.
- [ ] Integration test: logout and account-updated propagation.
- [ ] Unit tests for auth state transitions.

---

## Phase 11: Models + Session Controls
- [ ] Implement `model/list` and model picker in AI UI.
- [ ] Implement `experimentalFeature/list` support to gate client-visible toggles.
- [ ] Implement `collaborationMode/list` and add per-thread collaboration mode control.
- [ ] Add per-thread controls for model and effort usage.
- [ ] Validate unsupported/hidden model handling UX.

### Required Tests
- [ ] Integration test: model list pagination and hidden-model options.
- [ ] Unit tests for model picker state and persistence rules.

---

## Phase 12: Packaging and Cross-Platform Runtime
- [ ] Bundle Codex app-server runtime artifacts for macOS, Linux, Windows.
- [ ] Add startup path resolution and compatibility checks per platform.
- [ ] For Windows targets, validate whether `windowsSandbox/setupStart` is required and implement if needed.
- [ ] Add packaging validation in CI for all target OSes.

### Required Tests
- [ ] Integration test: startup on each target platform image.
- [ ] Integration test: reconnect/restart behavior cross-platform.

---

## Phase 13: Stabilization and Performance
- [ ] Load/perf profiling for long threads and heavy stream volume.
- [ ] Backpressure and bounded memory handling under burst notifications.
- [ ] Final UX polish for offline/reconnect/error states.
- [ ] Documentation pass for developer setup and user behavior.

### Required Tests
- [ ] Integration soak test with long multi-turn thread.
- [ ] Integration test for reconnect under active stream.
- [ ] Regression suite pass across entire workspace.

---

## Final Exit Criteria (Project Complete)
- [ ] All phase gates passed.
- [ ] All deep review findings closed.
- [ ] All required tests and workspace checks pass.
- [ ] AI feature works cross-platform with bundled Codex runtime.
- [ ] CLI <-> Hunk continuity verified through shared `~/.codex`.
- [ ] `docs/AI_CODEX_SPEC.md` method matrix reflects implemented vs deferred API methods.
