# AI Coding in Hunk: Codex App Server Spec

## Status
- In progress
- Owner: Hunk
- Last Updated: 2026-04-05

## Product Decisions (Locked)
1. Transport is WebSocket-only. Hunk will not implement a stdio integration path.
2. Codex integration is bundled with Hunk; users should not install Codex separately.
3. Hunk and Codex CLI share `~/.codex` for auth and thread continuity.
4. Thread discovery and default navigation are `cwd`-scoped.
5. Approval UI must support explicit `accept` and `decline`.
6. "Mad Max" mode exists in v1 and is a per-workspace setting.
7. v1 includes ChatGPT login via system browser flow.
8. Tool call handling is required in v1.

## Pinned Upstream Baseline
- Codex repo: `https://github.com/openai/codex`
- Pinned tag: `rust-v0.119.0`
- Pinned commit SHA: `4a3466efbf84cfb7469eca94bbf6307166c9f48e`
- Pin captured on: 2026-04-10

## Architecture Boundary
- New crate: `crates/hunk-codex`
- Responsibilities:
  - bundled app-server host lifecycle (WebSocket mode)
  - JSON-RPC request/response correlation
  - typed protocol envelope handling
  - AI domain state reducer for threads, turns, and items
  - bridge APIs consumed by `hunk-desktop`
- Non-responsibilities (v1):
  - non-Codex providers
  - stdio transport

## v1 Method Coverage

### Required Core Lifecycle
- `initialize`
- `initialized`

### Required Thread/Turn Flow
- `thread/start`
- `thread/resume`
- `thread/fork`
- `thread/read`
- `thread/list`
- `thread/loaded/list`
- `thread/archive`
- `thread/unarchive`
- `thread/unsubscribe`
- `thread/compact/start`
- `thread/rollback`
- `turn/start`
- `turn/steer`
- `turn/interrupt`
- `review/start`

### Required Coding Interaction
- `item/commandExecution/requestApproval`
- `item/fileChange/requestApproval`
- `item/tool/requestUserInput`
- `item/tool/call`
- `command/exec`

### Required Account/Auth
- `account/read`
- `account/login/start`
- `account/login/cancel`
- `account/logout`
- `account/rateLimits/read`

### Required Discovery and Metadata
- `model/list`
- `experimentalFeature/list`
- `collaborationMode/list`
- `skills/list`
- `skills/config/write`
- `app/list`

### Required Notifications (minimum)
- `thread/started`
- `thread/status/changed`
- `thread/archived`
- `thread/unarchived`
- `thread/closed`
- `turn/started`
- `turn/completed`
- `turn/diff/updated`
- `item/started`
- `item/completed`
- `item/agentMessage/delta`
- `item/commandExecution/outputDelta`
- `item/fileChange/outputDelta`
- `serverRequest/resolved`
- `account/updated`
- `account/rateLimits/updated`

## Deferred Methods (Post-v1)
- MCP auth/config admin:
  - `mcpServer/oauth/login`
  - `mcpServerStatus/list`
  - `config/mcpServer/reload`
- Config authoring:
  - `config/read`
  - `config/value/write`
  - `config/batchWrite`
  - `configRequirements/read`
- External agent migration:
  - `externalAgentConfig/detect`
  - `externalAgentConfig/import`
- `feedback/upload`
- Realtime audio/text thread APIs
- Fuzzy file search session APIs
- `windowsSandbox/setupStart` unless Windows packaging validation proves it is mandatory

## Guardrails + Mad Max
- Default behavior:
  - show explicit approval cards for command and file-change server requests
  - do not auto-approve by default
- Mad Max per-workspace behavior:
  - set approval policy to `never`
  - set sandbox mode to `danger-full-access`
  - visually label workspace as unsafe mode
  - require explicit user opt-in and explicit user exit

## Quality Gates
A phase closes only when:
1. Unit tests and integration tests pass for that phase.
2. `cargo check --workspace` passes.
3. `cargo test --workspace` passes.
4. `cargo clippy --workspace --all-targets -- -D warnings` passes.
5. Deep code review findings are fixed before advancing.

## References
- Docs: `https://developers.openai.com/codex/app-server`
- API overview: `https://developers.openai.com/codex/app-server#api-overview`
- Source: `https://github.com/openai/codex/tree/main/codex-rs/app-server`
