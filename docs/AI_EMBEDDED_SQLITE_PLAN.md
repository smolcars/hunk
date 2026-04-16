# Embedded Codex SQLite Plan

## Goal

Unblock an upstream-style embedded Codex path for Hunk without destabilizing Hunk's own SQLite-backed features.

## Current Blocker

Hunk desktop and upstream embedded Codex currently want different native SQLite linkages in the same final binary.

Hunk side:
- `crates/hunk-domain/Cargo.toml` depends on `rusqlite = { version = "0.37", features = ["bundled"] }`
- `cargo tree -p hunk-desktop -i libsqlite3-sys@0.35.0` resolves:
  - `hunk-desktop -> hunk-domain -> rusqlite 0.37.0 -> libsqlite3-sys 0.35.0`

Upstream embedded side:
- `codex-rs/app-server-client/Cargo.toml` depends directly on `codex-app-server`
- `codex-rs/app-server/Cargo.toml` depends directly on `codex-state`
- `codex-rs/state/Cargo.toml` depends directly on `sqlx`
- `codex-rs/Cargo.lock` pins `libsqlite3-sys 0.30.1`

Cargo will not link two different crates that both declare `links = "sqlite3"` into the same binary. That is why the current desktop build can ship `RemoteBundled` but not true in-process embedded Codex.

## Constraints

- Do not destabilize Hunk comment storage, migrations, or other `hunk-domain` SQLite usage just to chase embedded Codex.
- Keep the existing `AppServerClient` boundary in `hunk-codex`; transport-specific changes should stay behind that seam.
- Avoid a broad dependency rewrite in `hunk-desktop`.
- Prefer an approach that preserves upstream code with minimal local forking.

## Option Summary

### Option A: Align Hunk and upstream on one SQLite linkage

Ways this could happen:
- downgrade Hunk's `rusqlite` stack to match upstream's `libsqlite3-sys`
- upgrade/fork upstream Codex SQL stack to match Hunk's `libsqlite3-sys`
- remove `bundled` and rely on a shared system SQLite crate path

Problems:
- this touches unrelated Hunk persistence code
- the conflict is the Rust crate-level `links = "sqlite3"` identity, not only the native library on disk
- it creates upgrade pressure every time either Hunk or Codex changes SQLite dependencies

Recommendation:
- do not use this as the primary path

### Option B: Isolate embedded Codex in a separate helper binary

Shape:
- add a new crate/binary, for example `crates/hunk-codex-embedded-runner`
- this binary links upstream embedded crates:
  - `codex-app-server-client`
  - `codex-app-server`
  - their transitive `codex-state/sqlx/sqlite` stack
- the desktop app does not link those crates directly
- desktop talks to the helper over a thin local IPC transport owned by Hunk

Pros:
- cleanly breaks the SQLite linkage conflict
- preserves upstream embedded code with minimal changes
- keeps Hunk desktop binary independent of upstream SQL choices
- gives us a practical path to evaluate embedded semantics without rewriting Hunk persistence

Cons:
- not true same-process embedding inside `hunk-desktop`
- introduces a new helper transport to maintain
- performance/stability will improve less than a true in-process integration

Recommendation:
- this is the best engineering path if we want to unblock embedded behavior in the near term

### Option C: Upstream feature split to remove `codex-state/sqlx` from the embedded client path

Shape:
- work with upstream or carry a fork so `codex-app-server-client` can build an in-process client without pulling the full `codex-app-server -> codex-state` stack into embedders
- likely requires optionalizing state/persistence concerns in upstream crates

Pros:
- enables true in-process desktop embedding later
- avoids the extra helper process

Cons:
- high uncertainty
- likely invasive upstream work
- difficult to schedule as a Hunk-only dependency

Recommendation:
- treat this as a longer-term track, not the first unblocker

### Option D: Dynamic loading or plugin-style isolation

Shape:
- compile the upstream embedded stack into a separate shared library and load it dynamically

Problems:
- platform-specific packaging complexity
- harder crash isolation than a helper process
- worse operational simplicity than a separate binary

Recommendation:
- not recommended

## Recommended Path

Adopt Option B now, keep Option C as the long-term true-embedded path.

That means:
- short term: isolate upstream embedded Codex in a helper binary
- medium term: validate whether helper-based embedded semantics are materially better than `RemoteBundled`
- long term: only pursue true same-process embedding if the helper still leaves meaningful reliability or latency gaps

## Proposed Architecture

### New crate

Add:
- `crates/hunk-codex-embedded-runner`

Responsibilities:
- own the upstream embedded dependency graph
- start upstream `InProcessAppServerClient`
- expose a Hunk-controlled IPC surface to the desktop app

Non-responsibilities:
- no GPUI/UI code
- no Hunk persistence
- no direct desktop state management

### IPC boundary

Use a narrow request/event protocol that mirrors the existing Hunk `AppServerClient` boundary:
- request
- notify
- next_event
- respond_typed
- reject_server_request
- shutdown

Transport choices, in order:
1. Unix domain socket / Windows named pipe
2. stdio framed JSON
3. loopback websocket only if reusing existing remote machinery is materially cheaper

Recommendation:
- prefer UDS / named pipe so the helper is still local-only and avoids another HTTP/websocket layer

### Desktop integration

Add a new transport implementation behind the current Hunk seam:
- `EmbeddedHelper` or similar transport kind

Flow:
1. desktop launches helper binary
2. helper starts upstream `InProcessAppServerClient`
3. desktop uses the same worker/reducer path already used by `RemoteBundled`
4. transport-specific code stays isolated in `hunk-codex`

## Work Breakdown

### Phase 1: Prove the isolation path

- create `hunk-codex-embedded-runner` as a standalone binary crate
- link upstream embedded crates there
- confirm the desktop workspace no longer sees the SQLite conflict
- implement a tiny handshake:
  - start
  - initialize
  - shutdown

Success criteria:
- helper binary builds independently
- desktop workspace still builds cleanly
- helper can boot and shut down on demand

### Phase 2: Minimal embedded-helper transport

- define a framed IPC protocol aligned to Hunk's `AppServerClient`
- implement:
  - typed request/response
  - server event forwarding
  - server-request response/rejection
- add a new Hunk transport adapter for the helper

Success criteria:
- thread/list
- thread/read
- thread/resume
- approvals
- requestUserInput
- shutdown

### Phase 3: Recovery and parity

- add reconnect/restart behavior for the helper
- make sure helper crashes surface as transport errors, not hung streams
- compare behavior with `RemoteBundled` on:
  - active-turn reconnect
  - long-running streams
  - approvals and user input
  - workspace switching

Success criteria:
- helper transport passes the same reducer/controller coverage currently used for `RemoteBundled`

### Phase 4: Decision gate

Decide whether to:
- keep helper-based embedded as the long-term “embedded” implementation
- or invest in upstream feature-splitting for true in-process desktop embedding

Decision inputs:
- stability delta versus `RemoteBundled`
- startup latency
- CPU wakeups / memory churn
- operational complexity

## Validation Plan

Automated:
- helper startup/shutdown tests
- request/event round-trip tests
- parity tests against existing `ThreadService` scenarios
- crash/restart tests for the helper transport

Manual:
- long-running agent sessions
- repeated workspace switching during active turns
- approval and user-input flows
- compare “stream went quiet until I typed continue” incidence versus `RemoteBundled`

## Risks

- A helper binary is still another process, so it does not remove every transport failure surface.
- IPC ownership and lifecycle need to stay simpler than the current external websocket host, or the effort will not pay off.
- Upstream embedded crates may still pull in additional assumptions beyond SQLite; the first spike should verify startup viability early.

## Explicit Non-Recommendations

- Do not rewrite Hunk persistence to `sqlx` just to match upstream.
- Do not downgrade `rusqlite` as the first move.
- Do not maintain a long-lived fork of upstream Codex just to patch SQLite versions together.

## First Spike

If we want the fastest path to reduce uncertainty, the first concrete spike should be:

1. Create `crates/hunk-codex-embedded-runner`.
2. Link upstream `codex-app-server-client` there.
3. Start `InProcessAppServerClient` from a tiny `main`.
4. Expose one round-trip command such as `thread/list`.
5. Verify the desktop workspace still builds without SQLite linkage conflicts.

If that spike fails, stop and reassess. If it succeeds, continue with the helper transport instead of trying to force true same-process embedding immediately.
