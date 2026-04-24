# AI Browser CEF TODO

Status: first runtime/tooling slice started.

This tracks the implementation of a true in-app browser for Hunk that can be controlled by the AI agent. The v1 direction is CEF offscreen rendering, embedded inside the GPUI AI workspace, with a single browser surface tied to the active AI session.

## Decisions

- Use CEF as the v1 browser engine.
- Do not implement Servo, Wry, or Lightpanda for v1.
- Use offscreen/windowless rendering so GPUI owns the visible browser surface.
- Bundle a pinned CEF runtime with Hunk. App size increase is acceptable.
- Start with macOS arm64, then add Linux and Windows packaging.
- Support one AI-controlled browser tab per browser-enabled AI thread in v1.
- Use one global CEF runtime, with browser sessions keyed by AI thread ID.
- Render only the selected thread's browser session; other thread sessions may remain alive in the background or be restored by policy later.
- Allow normal browsing actions, but prompt before sensitive actions.
- Render the browser in the same resizable right-side companion pane slot used by AI inline review/diff preview, not as a row inside the scrollable AI timeline.
- Keep compact browser tool/activity summaries in the AI timeline so the conversation remains auditable.

## Phase 0: CEF Runtime Spike

- [ ] Pick the exact CEF build and architecture for the first spike: macOS arm64.
- [ ] Pick the Rust CEF binding version that matches the selected CEF build.
- [ ] Verify that the binding supports the required CEF APIs:
  - [ ] app/browser-process startup
  - [ ] subprocess path configuration
  - [ ] offscreen rendering callbacks
  - [ ] mouse, wheel, keyboard, focus, resize, and scale input
  - [ ] JavaScript execution or DevTools/CDP access
  - [ ] screenshot or frame capture
- [ ] If the Rust binding is missing a required callback, add a small local native shim only for that missing API.
- [ ] Add a local-only CEF asset layout under `assets/browser-runtime/cef/macos`.
- [ ] Add a README in the runtime asset folder with the pinned CEF version, download source, expected files, and checksum process.
- [ ] Build a minimal offscreen CEF smoke path that loads `https://example.com` and produces a nonblank frame buffer.

Exit criteria:

- [ ] Hunk can initialize and shut down CEF on macOS arm64.
- [ ] A helper subprocess launches correctly.
- [ ] An offscreen page produces pixels without opening the system browser.

## Phase 1: Browser Runtime Crates

- [x] Add `crates/hunk-browser` for browser runtime logic.
- [ ] Add `crates/hunk-browser-helper` for the CEF subprocess entrypoint.
- [x] Add browser runtime types:
  - [x] `BrowserRuntime`
  - [x] `BrowserSession`
  - [x] `BrowserFrameMetadata`
  - [x] `BrowserSnapshot`
  - [x] `BrowserAction`
  - [x] `SensitiveBrowserAction`
- [ ] Initialize CEF once during desktop app startup.
- [ ] Shut CEF down during desktop app exit.
- [ ] Store browser profile, cache, cookies, and local storage under a Hunk-owned app data directory.
- [ ] Keep the browser profile isolated from the user's system browser profile.
- [x] Key browser sessions by AI thread ID.
- [x] Create a browser session lazily when a thread first opens or uses the browser.
- [x] Keep each thread's URL, latest frame, latest snapshot index map, and navigation state separate.
- [ ] Implement navigation, reload, stop, back, and forward.
- [ ] Implement resize and device-scale handling.
- [ ] Implement mouse, wheel, keyboard, and focus forwarding.
- [ ] Convert CEF BGRA frame buffers into a GPUI-paintable frame representation.
- [ ] Keep frame conversion off the GPUI render path.
- [x] Add crate-level tests for snapshot indexing, stale index rejection, and safety classification.
- [ ] Add crate-level tests for input coordinate scaling.

Exit criteria:

- [ ] `hunk-browser` can drive a single CEF browser session independently from the UI.
- [ ] Multiple AI threads can have separate browser session state without sharing the active page.
- [ ] Browser state is testable without requiring GPUI.
- [ ] Runtime failures return structured errors instead of panicking.

## Phase 2: GPUI Browser Panel

- [x] Add browser state to the AI workspace state model.
- [x] Extend the AI workspace layout so the main timeline/composer column can open a right-side companion pane that switches between inline review/diff preview and browser modes.
- [x] Reuse the existing AI workspace split-pane pattern used for inline review.
- [x] If inline review and browser are both available, show them as switchable right-pane modes instead of creating three crowded columns.
- [x] Keep the browser outside the scrollable timeline so page rendering, focus, and input are independent from timeline scrolling.
- [ ] Add browser controls:
  - [ ] address bar
  - [ ] back
  - [ ] forward
  - [ ] reload
  - [ ] stop
  - [ ] page loading/error status
  - [ ] agent-control indicator
- [x] Use colors from `crates/hunk-desktop/src/app/theme.rs`.
- [ ] Paint the latest browser frame into the GPUI surface.
- [ ] Forward panel mouse, wheel, keyboard, focus, resize, and scale changes to `hunk-browser`.
- [ ] Throttle frame notifications to 60fps for v1.
- [ ] Keep browser rendering work within the 8ms frame budget.
- [ ] Add compact AI timeline rows for browser activity such as navigation, click, type, scroll, screenshot, and confirmation-required events.

Exit criteria:

- [ ] The AI workspace displays a live in-app browser.
- [ ] Manual navigation works through the Hunk UI.
- [ ] The browser does not launch the user's default browser for normal web navigation.
- [ ] Browser activity is visible in the timeline without embedding the browser viewport inside timeline rows.

## Phase 3: AI Dynamic Browser Tools

- [x] Register browser tools through `ThreadStartParams.dynamic_tools` for browser-enabled AI threads.
- [x] Add helper to apply browser tool specs to `ThreadStartParams.dynamic_tools`.
- [x] Add helper to inject browser-specific developer instructions for browser-enabled threads.
- [x] Tell the agent to use `hunk.browser_snapshot` before click/type actions and then act by `snapshotEpoch` plus element index.
- [x] Tell the agent to use embedded `hunk.browser_*` tools instead of launching an external browser.
- [x] Add typed parsing from Codex browser dynamic tool arguments into `hunk-browser` actions.
- [x] Add a desktop-side dynamic tool executor seam for browser tool calls.
- [x] Preserve the existing workspace dynamic tools.
- [ ] Route browser tool calls asynchronously to `hunk-browser`.
- [x] Add browser tools:
  - [x] `hunk.browser_navigate`
  - [x] `hunk.browser_snapshot`
  - [x] `hunk.browser_click`
  - [x] `hunk.browser_type`
  - [x] `hunk.browser_press`
  - [x] `hunk.browser_scroll`
  - [x] `hunk.browser_screenshot`
- [ ] Make `hunk.browser_snapshot` return URL, title, viewport, scroll position, visible text, and indexed interactive elements.
- [ ] Store the latest element index map in the browser session.
- [x] Reject click/type actions when the index map is stale after navigation or page mutation.
- [ ] Return concise action results to the model.
- [ ] Return screenshots through the richest image-capable result format available in the current Codex protocol.
- [x] Add tests for tool schema generation, tool routing, and missing-browser errors.
- [x] Add tests for stale snapshot behavior in routed browser tool calls.

Exit criteria:

- [ ] The AI agent can navigate, inspect, click, type, press keys, scroll, and capture screenshots in the in-app browser.
- [ ] Tool calls operate on the embedded CEF browser, not an external browser.

## Phase 4: Sensitive Action Policy

- [ ] Add a browser safety policy module.
- [ ] Prompt before credential submission, purchases, payments, file upload/download, permission prompts, external protocol launches, and high-risk form submissions.
- [ ] Route browser confirmations through the existing AI user-input or approval UI.
- [ ] Redact secrets from tool results.
- [ ] Block external protocol launches unless the user confirms.
- [ ] Add tests for sensitive-action classification and confirmation-required tool responses.

Exit criteria:

- [ ] Normal browsing is smooth.
- [ ] Sensitive actions pause for user confirmation.
- [ ] Secret values are not echoed back to the model.

## Phase 5: Packaging

- [ ] Add `assets/browser-runtime` to desktop package resources.
- [ ] Add CEF runtime validation scripts for macOS first.
- [ ] Update macOS packaging to include:
  - [ ] CEF framework/runtime files
  - [ ] locales
  - [ ] resources
  - [ ] helper app/binary
  - [ ] codesigning and notarization coverage
- [ ] Update Linux packaging after the macOS path is stable.
- [ ] Update Windows packaging after the Linux path is stable.
- [ ] Extend release bundle validation to check browser runtime files.
- [ ] Document how to refresh the pinned CEF runtime.

Exit criteria:

- [ ] A packaged macOS build can open the in-app browser without a development checkout.
- [ ] Bundle validation fails clearly when required CEF files are missing.

## Phase 6: Final Verification

- [ ] Add ignored integration tests that run only when the CEF runtime is installed.
- [ ] Integration smoke test: load `https://example.com`.
- [ ] Integration smoke test: verify a nonblank painted frame.
- [ ] Integration smoke test: forward click and key input.
- [ ] Integration smoke test: call `hunk.browser_snapshot`.
- [ ] Integration smoke test: capture a screenshot.
- [ ] Run `nix develop -c cargo build --workspace`.
- [ ] Run `nix develop -c cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run targeted browser, Codex, and desktop tests.
- [ ] Run macOS package smoke once the CEF runtime is staged.

Exit criteria:

- [ ] The in-app browser works in development.
- [ ] The AI agent can control the embedded browser.
- [ ] The macOS packaged app includes everything needed to run the browser.
- [ ] Build, clippy, and targeted tests pass.
