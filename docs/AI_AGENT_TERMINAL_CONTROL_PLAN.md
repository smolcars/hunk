# AI Agent Terminal Control Plan

## Status

- Planned
- Owner: Hunk
- Last Updated: 2026-04-29

## Summary

This document defines the engineering plan for exposing Hunk's AI terminal to the embedded Codex agent as a first-class controlled surface.

The target is parity with the agent-controlled browser model: the agent can open the terminal for the active AI thread, inspect current screen state, read recent output, manage terminal tabs, and send terminal input while coordinating with the embedded browser.

The current terminal implementation already provides most of the runtime foundation. The remaining work is a Codex dynamic-tool contract, a desktop UI bridge, terminal-specific response serialization, and a conservative safety policy for shell input.

## Product Goal

Enable full-stack AI workflows where the agent can coordinate terminal output and browser state without human intervention.

Examples:

- start a Next.js dev server in the AI terminal
- watch server logs while inspecting the app in the embedded browser
- react to build errors printed in the terminal
- open additional terminal tabs for tests, package installs, or background services
- keep the terminal and browser scoped to the same AI thread/worktree context

## Existing Foundation

Implemented pieces this plan should reuse:

- Browser dynamic tools already provide the model for tool registration, developer instructions, worker routing, UI execution, result serialization, and safety prompts.
- `crates/hunk-terminal` already owns PTY spawning, process lifecycle, resize, output streaming, VT screen snapshots, keyboard input, paste, focus, pointer input, wheel input, and scrollback.
- The AI terminal is already thread-scoped, has tab state, parks hidden runtimes, restores selected thread state, and keeps bounded transcript output.
- The GPUI terminal surface already renders from `TerminalScreenSnapshot`, so the agent does not need a screenshot path for V1.

Non-goals for this work:

- Do not expose the Files terminal in V1.
- Do not create a second terminal runtime in `hunk-codex`.
- Do not shell out from app code to control terminals.
- Do not persist live terminal sessions across full app relaunch.

## Engineering Plan

### Phase 1: Tool Contract

Add a new `hunk_terminal` dynamic-tool namespace in `crates/hunk-codex`.

Create `crates/hunk-codex/src/terminal_tools.rs` with:

- tool constants and namespace detection
- dynamic tool specs
- terminal-specific developer instructions
- typed argument parsing
- idempotent helper to inject terminal tool specs into `ThreadStartParams`
- crate-level tests in `crates/hunk-codex/tests`

Initial tool set:

- `hunk_terminal.open`: ensure the active thread terminal is visible and has a shell session
- `hunk_terminal.tabs`: list terminal tabs for the active AI thread
- `hunk_terminal.new_tab`: create a new terminal tab and optionally activate it
- `hunk_terminal.select_tab`: select a terminal tab
- `hunk_terminal.close_tab`: close a terminal tab
- `hunk_terminal.snapshot`: return current screen text, cursor, size, mode, status, cwd, active tab, and tab summaries
- `hunk_terminal.logs`: return bounded transcript/log output, with `sinceSequence` and `limit` support if a sequence cursor is added
- `hunk_terminal.run`: send a command plus newline to the selected shell
- `hunk_terminal.type`: type text without automatically submitting it
- `hunk_terminal.paste`: paste text using terminal bracketed paste behavior
- `hunk_terminal.press`: send a named key sequence such as `Enter`, `Ctrl+C`, `Up`, or `Shift+PageUp`
- `hunk_terminal.scroll`: scroll the terminal viewport
- `hunk_terminal.resize`: resize rows and columns
- `hunk_terminal.kill`: stop the selected terminal process

### Phase 2: Thread Start Integration

Add terminal tool injection alongside browser tool injection when starting AI threads.

The worker start config should carry a terminal-tools-enabled flag. Enable terminal tools when the AI workspace supports a project terminal. Keep browser and terminal enablement independent so one surface can be available without the other.

Developer instructions should tell the agent:

- use `hunk_terminal.snapshot` before relying on terminal screen state
- use `hunk_terminal.logs` for long-running server output
- use `hunk_terminal.tabs` before targeting non-active tabs
- use `hunk_terminal.run` for shell commands
- use `hunk_terminal.press` for interactive prompts and process control
- coordinate browser and terminal by inspecting both surfaces instead of starting external tools

### Phase 3: Worker To UI Bridge

Mirror the browser bridge rather than executing terminal control in the worker.

Add an event payload similar to `BrowserToolCall`, for example `TerminalToolCall { params, response_tx }`.

Routing rules:

- terminal dynamic tool calls from Codex go through the AI worker event stream
- the visible GPUI workspace receives the call
- the UI selects the requested thread if needed
- the UI opens or promotes the thread terminal runtime
- the UI executes the terminal action against the live `TerminalSessionHandle`
- the UI returns a structured dynamic-tool response to the worker

This keeps ownership correct: the desktop controller owns live terminal handles and hidden runtime parking.

### Phase 4: Desktop Terminal Executor

Add a new desktop executor module instead of growing the existing browser dynamic-tool file.

Suggested files:

- `crates/hunk-desktop/src/app/ai_terminal_dynamic_tools.rs`
- `crates/hunk-desktop/src/app/controller/ai/terminal_tools.rs`

Responsibilities:

- parse terminal dynamic tool requests from `hunk-codex`
- select/create/close terminal tabs using existing terminal controller helpers
- ensure a shell session for `open`, `run`, `type`, `paste`, and `press`
- write input through existing `TerminalSessionHandle` APIs
- read visible and hidden terminal state without moving process ownership unnecessarily
- return consistent JSON success/error responses

Keep the file split small. `controller/ai/terminal.rs` is already large, so new agent-control logic should live in focused modules.

### Phase 5: Snapshot And Log Responses

Create a terminal response serializer that produces model-friendly text-first output.

`snapshot` response should include:

- `ok`
- `threadId`
- `turnId`
- `activeTabId`
- `tabs`
- `status`
- `cwd`
- `rows`
- `cols`
- `displayOffset`
- `cursor`
- `mode`
- `visibleText`
- optional `cells` behind a small cap or explicit `includeCells` flag

`logs` response should include:

- recent transcript text
- truncation metadata
- selected tab ID
- terminal status and exit code

Do not return full unbounded transcripts. Use existing bounded transcript behavior and add a response cap so dynamic tool output stays small.

### Phase 6: Safety Policy

Terminal control is higher risk than browser control because shell input can mutate the workspace or machine.

Add a terminal safety classifier before executing actions. It should return either allow, confirmation required, or reject.

Confirmation should be required for:

- destructive commands such as `rm -rf`, `git clean`, `git reset --hard`, disk formatting, or recursive permission changes
- commands that appear to exfiltrate secrets or upload arbitrary files
- commands that install global packages or change system configuration
- multi-line paste or run actions that include multiple commands
- process kill actions for running terminals
- likely secret entry, unless the action is explicitly user-approved

The policy should align with the current Codex approval/sandbox posture. In mad-max mode, policy can be less restrictive, but secret redaction should still apply to tool responses.

### Phase 7: UI State And Timeline Feedback

Terminal tool calls should have visible feedback in the AI timeline, similar to browser dynamic tool rows.

Add compact terminal tool summaries:

- `Opened terminal`
- `Read terminal snapshot`
- `Started command: npm run dev`
- `Pressed Ctrl+C`
- `Selected terminal tab 2`

When a terminal action needs confirmation, reuse the existing pending approval UI pattern rather than inventing a new modal path.

### Phase 8: Validation

Validation should cover the tool contract, routing, state behavior, and terminal IO.

Run final validation once after implementation:

- `nix develop -c cargo test -p hunk-codex`
- `nix develop -c cargo test -p hunk-terminal`
- targeted desktop tests for terminal tool response helpers
- `nix develop -c cargo clippy --workspace --all-targets --all-features`
- `nix develop -c cargo build --workspace`

Do not repeatedly run the full workspace checks during iteration. Run them once at the end.

## To-Do Items

### Codex Tool Contract

- [x] Add `crates/hunk-codex/src/terminal_tools.rs`.
- [x] Export the terminal tools module from `crates/hunk-codex/src/lib.rs`.
- [x] Define `TERMINAL_TOOL_NAMESPACE` as `hunk_terminal`.
- [x] Define constants for all V1 terminal tools.
- [x] Add terminal developer instructions.
- [x] Add terminal dynamic tool specs.
- [x] Add typed request parsing for every terminal tool.
- [x] Add idempotent `apply_terminal_thread_start_context`.
- [x] Add tests for tool list coverage.
- [x] Add tests for schema serialization.
- [x] Add tests for namespace/tool detection.
- [x] Add tests for argument parsing and validation.
- [x] Add tests for idempotent thread-start context injection.

### Worker Integration

- [x] Add terminal-tools-enabled state to `AiWorkerStartConfig`.
- [x] Apply terminal tool context during `StartThread` when enabled.
- [x] Add trace logging for terminal dynamic tool registration.
- [x] Add `AiWorkerEventPayload::TerminalToolCall`.
- [x] Route terminal dynamic tool calls from `ServerRequest::DynamicToolCall` through the UI bridge.
- [x] Add timeout/error responses for disconnected terminal UI bridge.
- [x] Preserve existing workspace and browser dynamic tool behavior.

### Desktop Executor

- [x] Add `crates/hunk-desktop/src/app/ai_terminal_dynamic_tools.rs`.
- [x] Add terminal JSON success/error response helpers.
- [x] Add terminal unavailable response helper.
- [x] Add terminal confirmation-required response helper.
- [x] Add terminal confirmation-declined response helper.
- [x] Add terminal action summary helpers.
- [x] Add terminal safety mode enum for enforce and allow-once behavior.
- [x] Keep executor logic independent from browser dynamic tool code.

### Controller Bridge

- [x] Add `ai_handle_terminal_dynamic_tool_call`.
- [x] Add `ai_execute_terminal_dynamic_tool_with_safety`.
- [x] Add pending terminal approval state.
- [x] Add terminal approval accept/decline action.
- [x] Ensure terminal tool calls select the correct AI thread.
- [x] Ensure `open` opens the terminal drawer and starts a shell when needed.
- [x] Ensure `tabs`, `snapshot`, and `logs` can read hidden tab state without accidentally killing a runtime.
- [x] Ensure `new_tab`, `select_tab`, and `close_tab` reuse existing tab controller behavior.
- [x] Ensure terminal tool execution notifies GPUI only when visible state changes.

### Terminal Actions

- [x] Implement `open`.
- [x] Implement `tabs`.
- [x] Implement `new_tab`.
- [x] Implement `select_tab`.
- [x] Implement `close_tab`.
- [x] Implement `snapshot`.
- [x] Implement `logs`.
- [x] Implement `run`.
- [x] Implement `type`.
- [x] Implement `paste`.
- [x] Implement `press`.
- [x] Implement `scroll`.
- [x] Implement `resize`.
- [x] Implement `kill`.

### Response Serialization

- [x] Add helper to convert `TerminalScreenSnapshot` to visible text lines.
- [x] Add helper to serialize cursor state.
- [x] Add helper to serialize mode flags.
- [x] Add helper to serialize tab summaries.
- [x] Add transcript truncation metadata to log responses.
- [x] Add optional cell serialization with a strict cap.
- [x] Redact likely secret tokens from visible text and logs.
- [x] Add stable error codes for stale/missing tab, no active thread, no workspace, and no shell session.

### Safety

- [x] Add terminal safety classifier.
- [x] Detect destructive shell commands.
- [x] Detect multi-line command submissions.
- [x] Detect likely secret input.
- [x] Detect system-level install/configuration commands.
- [x] Require confirmation for `kill`.
- [x] Require confirmation for command chains that include multiple separators.
- [x] Add tests for safety classification.
- [x] Add tests for confirmation-required responses.
- [x] Add tests that sensitive input is redacted from tool responses.

### Timeline And UX

- [x] Render terminal dynamic tool calls as compact timeline rows.
- [x] Add terminal tool action summaries.
- [x] Show pending terminal approvals in the existing approvals UI.
- [x] Add clear status messages for bridge disconnects and execution failures.
- [x] Keep the terminal drawer visually unchanged for normal user operation.

### Tests And Validation

- [x] Add `crates/hunk-codex/tests/terminal_tools.rs`.
- [x] Add terminal response serializer tests.
- [x] Add terminal safety tests.
- [x] Add worker routing tests for terminal dynamic tools.
- [x] Add desktop bridge tests where helpers can be exercised without a live GPUI window.
- [x] Add `hunk-terminal` integration coverage for input actions that the dynamic tools depend on.
- [x] Run `nix develop -c cargo test -p hunk-codex`.
- [x] Run `nix develop -c cargo check -p hunk-desktop`.
- [x] Run `nix develop -c cargo test -p hunk-desktop ai_terminal_dynamic_tools`.
- [x] Run `nix develop -c cargo test -p hunk-desktop ai_workspace_timeline_projection::tests`.
- [x] Run `nix develop -c cargo test -p hunk-desktop --test ai_terminal_safety`.
- [x] Run `nix develop -c cargo test -p hunk-desktop dynamic_tool_route_classifies_terminal_tools_for_ui_bridge`.
- [x] Run `nix develop -c cargo test -p hunk-terminal`.
- [x] Run `nix develop -c cargo clippy --workspace --all-targets --all-features`.
- [x] Run `nix develop -c cargo build --workspace`.

## Next Phase: Terminal Auto-Review Safety

The next implementation phase should move terminal safety away from command allowlists and toward a Guardian-style auto-review flow, similar to upstream Codex. Deterministic checks should remain small and conservative: allow read-only inspection, reject clearly invalid or credential-leaking actions, and route consequential or ambiguous terminal writes through a dedicated reviewer that returns structured JSON.

The reviewer must assess the exact terminal action in context, not just the command string. Context should include the user request, recent agent transcript, terminal cwd, workspace root, shell/platform, target tab, recent terminal logs, and a current terminal snapshot when useful. Low- and medium-risk reviewer approvals may execute automatically. High- and critical-risk actions must require user confirmation even when the reviewer returns `allow`. The system must fail closed to user confirmation on timeout, malformed output, or reviewer errors.

Current implementation status: the first slices have replaced the heuristic command-safety classifier with a conservative prefilter, structured auto-review request/assessment types, policy decision primitives, reviewer-output parsing, a prompt/schema document, and an async read-only reviewer session. Completed checklist items below refer to implemented scaffolding and app-side execution; unchecked items remain follow-up hardening.

### Architecture

- [x] Add a `TerminalAutoReview` subsystem owned by `crates/hunk-codex` or `crates/hunk-desktop` at the app-server boundary.
- [x] Model terminal review requests as structured data, not prompt-only strings.
- [x] Define `TerminalAutoReviewRequest` with action kind, command/input/keys, cwd, workspace root, shell, platform, tab id, thread id, turn id, recent logs, visible snapshot, and originating user intent.
- [x] Define `TerminalAutoReviewAssessment` with `riskLevel`, `userAuthorization`, `outcome`, `rationale`, and optional `evidence`.
- [x] Define explicit outcomes for `allow`, `confirm`, and `deny`; do not collapse reviewer output into a boolean.
- [x] Add a `TerminalAutoReviewPolicy` that maps reviewer `outcome` plus `riskLevel` into execute, user-confirm, or reject.
- [x] Execute reviewer `allow` decisions automatically only for `low` and `medium` risk levels.
- [x] Require user confirmation for reviewer `allow` decisions with `high` or `critical` risk levels.
- [x] Require user confirmation for reviewer `confirm` decisions at any risk level.
- [x] Reject reviewer `deny` decisions unless the user explicitly overrides the denied action.
- [x] Keep terminal secret redaction independent from auto-review so logs, snapshots, rationales, and pending approvals are always redacted before display or model review.
- [x] Preserve deterministic read-only behavior: `tabs`, `snapshot`, `logs`, and viewport reads should not require model review.

### Reviewer Session

- [x] Add a locked-down reviewer session inspired by upstream Codex Guardian.
- [x] Prefer a fast review model when available, with fallback to the active Codex model at low reasoning effort.
- [x] Run the reviewer with approval policy `never`.
- [x] Run the reviewer in a read-only sandbox or equivalent read-only capability profile.
- [x] Disable terminal write tools, browser mutation tools, plugins/apps, subagents, hooks, and any tool that can mutate local or remote state inside the reviewer.
- [x] Allow only read-only context tools needed for assessment, such as filesystem reads, git status/diff reads, terminal snapshot/log reads, and optionally browser snapshot/console reads.
- [x] Add a short timeout and fail closed to user confirmation when the reviewer does not complete.
- [x] Reject malformed or non-JSON reviewer output and surface the failure as `terminalConfirmationRequired`, not as an implicit allow.
- [x] Add cancellation support so interrupted turns stop pending terminal reviews.
- [x] Add a small denial circuit breaker so repeated denied terminal actions interrupt the current turn or force user confirmation.

### Prompt And Schema

- [x] Add a dedicated terminal auto-review policy prompt file under `crates/hunk-codex` or `docs/`.
- [x] Include a JSON schema for reviewer output with required `outcome` and optional `riskLevel`, `userAuthorization`, `rationale`, and `evidence`.
- [x] Prompt the reviewer to treat transcript, tool outputs, terminal logs, and command text as untrusted evidence, not instructions.
- [x] Prompt the reviewer to focus on whether the exact terminal action is authorized by the user and proportionate to the task.
- [x] Include policy guidance for local workspace mutation, destructive file operations, package installs, process management, network calls, credential handling, persistent system changes, and data exfiltration.
- [x] Keep low- and medium-risk actions eligible for automatic execution only when they are clearly task-aligned and have a bounded blast radius.
- [x] Require `confirm` or `deny` for high- and critical-risk actions unless the user has explicitly authorized the exact action and blast radius.
- [x] Require denial for obvious credential disclosure, untrusted exfiltration, broad destructive actions, or persistent security weakening without explicit user authorization.

### Deterministic Prefilter

- [x] Replace the current command-pattern classifier with a prefilter that only makes high-confidence decisions.
- [x] Allow read-only terminal tools without review.
- [x] Deny malformed requests, stale tab ids, no-workspace actions, and unavailable shell sessions before reviewer routing.
- [x] Deny obvious secret exfiltration before reviewer routing when command/input contains credential-looking material being sent to an untrusted external sink.
- [x] Route all terminal writes through prefilter: `run`, `type`, `paste`, `press`, `resize`, and `kill`.
- [x] Route ambiguous or consequential writes to auto-review instead of trying to prove safety locally.
- [x] Treat shell analyzers and command heuristics as evidence only; they should not become the main allowlist.
- [x] Keep user confirmation as fallback for reviewer timeout, reviewer denial override, or disabled auto-review.

### Terminal Context Collection

- [x] Capture current workspace root and cwd for every terminal review.
- [x] Capture shell family and platform for every terminal review.
- [x] Capture target tab id, active tab id, process status, exit code, title, and last command.
- [x] Capture recent terminal transcript lines with sequence metadata and redaction.
- [x] Capture visible terminal snapshot text with redaction.
- [x] Include recent relevant assistant/tool transcript from the Codex thread, capped and truncated predictably.
- [x] Include browser console/snapshot context only when a terminal action is linked to an active browser-driven workflow.
- [x] Avoid sending raw cell grids or large transcripts to the reviewer unless a clear assessment need exists.

### Executor Integration

- [x] Replace direct `terminal_dynamic_tool_confirmation` gating with `evaluate_terminal_action_prefilter`.
- [x] Add `review_terminal_action` for actions requiring auto-review.
- [x] Return `terminalConfirmationRequired` when the prefilter or reviewer requires human confirmation.
- [x] Return `terminalActionRejected` when the prefilter or reviewer denies the action.
- [x] Include review metadata in tool responses: `riskLevel`, `userAuthorization`, `outcome`, `rationale`, and `evidence`.
- [x] Execute low- and medium-risk reviewer-approved actions directly without creating a pending user approval.
- [x] Ensure an auto-approved terminal action executes only the exact reviewed request once.
- [x] Ensure a user-approved terminal action bypasses only the exact pending request once.
- [x] Keep terminal write execution centralized in the desktop controller so the reviewer and worker never own live terminal handles.
- [x] Make auto-review configurable per approval mode so users can choose user-only approvals, auto-review, or strict confirmation.

### Approval UX

- [x] Update pending terminal approval UI to distinguish `Needs user confirmation`, `Auto-review denied`, and `Auto-review timed out`.
- [x] Distinguish normal terminal confirmations from auto-review failures that fail closed to confirmation.
- [x] Show the reviewed command/input when it is not secret-looking.
- [x] Show reviewer risk level, user authorization level, rationale, and concise evidence.
- [x] Redact secret-looking fragments in summaries, rationale, evidence, timeline rows, and response payloads.
- [x] Add user actions for allow once and deny.
- [x] Add a path for the user to manually approve a specific action that auto-review denied, with the denial rationale preserved in thread context.

### Tests

- [x] Add unit tests for the deterministic prefilter: read-only allow, malformed reject, secret exfiltration reject, and ambiguous write review-required.
- [x] Add unit tests for unavailable terminal and stale context rejection before reviewer routing.
- [x] Add schema tests for `TerminalAutoReviewRequest` and `TerminalAutoReviewAssessment`.
- [x] Add prompt snapshot tests for terminal action review requests.
- [x] Add parser tests that malformed reviewer output fails closed.
- [x] Add reviewer routing tests that `run`, multiline `paste`, `kill`, and process-control keypresses route to auto-review.
- [x] Add policy tests that reviewer `allow` with `low` or `medium` risk maps to single-request execution.
- [x] Add policy tests that reviewer `allow` with `high` or `critical` risk maps to pending user approval.
- [x] Add policy tests that reviewer `confirm` maps to pending user approval with review metadata.
- [x] Add policy coverage that reviewer `deny` preserves denial metadata for manual override and can reject after repeated denials.
- [x] Add timeout/cancellation coverage for reviewer failure paths.
- [x] Add redaction tests for command, logs, snapshot, rationale, and evidence.
- [x] Add compact browser-context coverage for terminal plus browser coordination.
- [x] Run `nix develop -c cargo test -p hunk-codex`.
- [x] Run `nix develop -c cargo test -p hunk-desktop --test ai_terminal_safety`.
- [x] Run `nix develop -c cargo check -p hunk-desktop`.
- [x] Run `nix develop -c cargo clippy --workspace --all-targets --all-features`.
- [x] Run `nix develop -c cargo build --workspace`.

## Open Questions

- Which low-risk command categories should be eligible for automatic execution after terminal auto-review lands?
- Should terminal tools be enabled only when browser tools are also enabled, or independently based on AI workspace/project availability?
- Should V1 expose raw cells, or keep the response text-only until a clear agent use case needs cell-level attributes?
- Should terminal logs use sequence cursors, transcript byte offsets, or line offsets?
- Should terminal tool calls be available in chat-only AI workspaces, or remain project-only like the visible terminal?
