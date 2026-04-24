# AI Browser CEF TODO

Status: Hunk-owned CEF macOS OSR smoke passed; production CEF adapter not connected yet.

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

- [x] Pick the exact CEF build and architecture for the first spike: macOS arm64.
- [x] Pick the Rust CEF binding version that matches the selected CEF build.
- [x] Verify that the binding supports the required CEF APIs:
  - [x] app/browser-process startup
  - [x] subprocess path configuration
  - [x] offscreen rendering callbacks
  - [x] mouse, wheel, keyboard, focus, resize, and scale input
  - [x] JavaScript execution or DevTools/CDP access
  - [x] screenshot or frame capture
- [ ] If the Rust binding is missing a required callback, add a small local native shim only for that missing API.
- [x] Add a local-only CEF asset layout under `assets/browser-runtime/cef/macos`.
- [x] Add a README in the runtime asset folder with the pinned CEF version, download source, expected files, and checksum process.
- [x] Build a minimal offscreen CEF smoke path that loads `https://example.com` and produces a nonblank frame buffer.
  - [x] Download/export the pinned macOS arm64 CEF runtime with cef-rs.
  - [x] Build and launch the cef-rs OSR app bundle from the exported runtime.
  - [x] Adapt the smoke to a Hunk-owned binary that loads `https://example.com`.
  - [x] Add an automated nonblank pixel assertion.

Exit criteria:

- [x] Hunk can initialize and shut down CEF on macOS arm64.
- [x] A helper subprocess launches correctly.
- [x] An offscreen page produces pixels without opening the system browser.

Implementation notes:

- Candidate binding: `tauri-apps/cef-rs` version `146.7.0+146.0.12`, backed by CEF `146.0.12+g6214c8e+chromium-146.0.7680.179`.
- First spike target: `aarch64-apple-darwin`, using cef-rs' prebuilt CEF download/export flow.
- The cef-rs OSR example uses `cef::execute_process` for subprocess dispatch, `cef::initialize` with `Settings { windowless_rendering_enabled: true, external_message_pump: true, .. }`, `WindowInfo { windowless_rendering_enabled: true, .. }`, and `browser_host_create_browser_sync`.
- The example provides the exact callback shape we need: `wrap_render_handler!` implements `view_rect`, `screen_info`, `on_paint`, and optional `on_accelerated_paint`. For Hunk v1, start with CPU BGRA `on_paint`; accelerated OSR can follow after the basic GPUI texture path is stable.
- Required browser controls exist in the generated bindings: `load_url`, `go_back`, `go_forward`, `reload`, `stop_load`, `was_resized`, `set_focus`, `send_key_event`, `send_mouse_click_event`, `send_mouse_move_event`, `send_mouse_wheel_event`, `execute_dev_tools_method`, and `add_dev_tools_message_observer`.
- cef-rs includes `bundle-cef-app`, helper naming metadata, and platform bundle support. We should adapt the packaging behavior rather than inventing a parallel CEF layout.
- Smoke command run successfully on macOS arm64: `CARGO_HOME=/Volumes/hulk/dev/cache/cargo HUNK_CEF_SKIP_EXPORT=1 HUNK_CEF_SMOKE_RUN_SECONDS=8 nix develop -c ./scripts/smoke_browser_cef_macos.sh`.
- Hunk-owned smoke command run successfully on macOS arm64: `CARGO_HOME=/Volumes/hulk/dev/cache/cargo HUNK_CEF_SKIP_EXPORT=1 HUNK_CEF_SMOKE_RUN_SECONDS=0 nix develop -c ./scripts/smoke_browser_cef_macos.sh`.
- Hunk-owned smoke result: `CEF smoke produced nonblank frame: Some((1024, 768)), frames=1, load_done=true`.
- The Hunk-owned smoke is intentionally isolated under `tools/browser-cef-smoke` instead of the normal workspace so regular Hunk builds do not resolve, build, or link CEF until the production adapter is ready.
- macOS CEF subprocesses require helper app bundles under `Contents/Frameworks`. The smoke script creates the general helper plus GPU, Renderer, Plugin, and Alerts variants, and the smoke binary points `Settings.browser_subprocess_path` at the general helper.
- The smoke uses cef-rs as a dependency for generated bindings and runtime/build utilities. Do not copy generated bindings into Hunk unless cef-rs becomes unmaintained or blocks a required API; copying them would make Hunk own a large version-sensitive FFI surface.
- Exported runtime size: about `325M`; bundled cef-rs OSR app size: about `342M`.
- cef-rs `cef-dll-sys` hardcodes the Ninja CMake generator, so Hunk's nix dev shell now includes `ninja`.

## Phase 1: Browser Runtime Crates

- [x] Add `crates/hunk-browser` for browser runtime logic.
- [x] Add `crates/hunk-browser-helper` for the CEF subprocess entrypoint.
  - [x] Replace the placeholder main with `cef::execute_process` once the cef-rs dependency is pinned in-tree.
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
  - [x] Add `BrowserStoragePaths` with isolated CEF root cache, profile, and downloads directories under `<app-data>/browser`.
  - [x] Add `BrowserRuntimeConfig` with CEF runtime path, helper executable path, and storage paths.
  - [x] Resolve the real app data directory during desktop startup and call `BrowserStoragePaths::ensure_directories`.
  - [ ] Pass `root_cache_path` and `profile_path` into the production CEF settings.
- [ ] Keep the browser profile isolated from the user's system browser profile.
  - [x] Model the profile path as a child of the Hunk-owned CEF root cache path, which CEF requires.
- [x] Key browser sessions by AI thread ID.
- [x] Create a browser session lazily when a thread first opens or uses the browser.
- [x] Keep each thread's URL, latest frame, latest snapshot index map, and navigation state separate.
- [ ] Implement navigation, reload, stop, back, and forward.
  - [x] Add state-only navigation helpers that invalidate stale snapshots.
  - [ ] Wire helpers to the CEF browser host.
- [ ] Implement resize and device-scale handling.
- [ ] Implement mouse, wheel, keyboard, and focus forwarding.
- [x] Convert CEF BGRA frame buffers into a GPUI-paintable frame representation.
  - [x] Add a validated `BrowserFrame` BGRA representation with metadata and nonblank checks.
  - [x] Add a desktop GPUI adapter that paints `BrowserFrame` through `RenderImage`.
- [ ] Keep frame conversion off the GPUI render path.
  - [x] Store validated frame bytes in `BrowserSession`; UI state reads frame metadata separately.
- [x] Add crate-level tests for snapshot indexing, stale index rejection, and safety classification.
- [x] Add crate-level tests for input coordinate scaling.

Exit criteria:

- [ ] `hunk-browser` can drive a single CEF browser session independently from the UI.
- [x] Multiple AI threads can have separate browser session state without sharing the active page.
- [x] Browser state is testable without requiring GPUI.
- [ ] Runtime failures return structured errors instead of panicking.

Implementation notes:

- `hunk-browser` now has a production configuration contract that does not link CEF yet: `BrowserRuntimeConfig` stores the CEF runtime directory, helper executable path, and `BrowserStoragePaths`.
- `BrowserRuntime::new_configured(config)` reports `BrowserRuntimeStatus::Configured` without starting CEF. Reserve `BrowserRuntimeStatus::Ready` for a production adapter that has actually initialized CEF.
- Desktop startup now resolves the shared Hunk app-data directory through `hunk_domain::state::app_data_dir`, creates the browser storage directories, and initializes `BrowserRuntime` as `Configured` when storage setup succeeds.
- Keeping this contract CEF-free lets normal workspace builds stay fast while packaging/runtime validation matures behind opt-in smoke scripts.
- `hunk-browser-helper` now exposes a feature-gated `cef-subprocess` entrypoint that calls `cef::execute_process` through the pinned cef-rs dependency. The default build keeps the placeholder path so normal workspace builds do not compile or link CEF unless the feature is requested.
- macOS release packaging builds `hunk-browser-helper` with `--features hunk-browser-helper/cef-subprocess` before copying it into the CEF helper app bundles.

## Phase 2: GPUI Browser Panel

- [x] Add browser state to the AI workspace state model.
- [x] Extend the AI workspace layout so the main timeline/composer column can open a right-side companion pane that switches between inline review/diff preview and browser modes.
- [x] Reuse the existing AI workspace split-pane pattern used for inline review.
- [x] If inline review and browser are both available, show them as switchable right-pane modes instead of creating three crowded columns.
- [x] Keep the browser outside the scrollable timeline so page rendering, focus, and input are independent from timeline scrolling.
- [x] Add browser controls:
  - [x] address bar
  - [x] back
  - [x] forward
  - [x] reload
  - [x] stop
  - [x] page loading/error status
  - [x] agent-control indicator
- [x] Use colors from `crates/hunk-desktop/src/app/theme.rs`.
- [x] Paint the latest browser frame into the GPUI surface.
- [ ] Forward panel mouse, wheel, keyboard, focus, resize, and scale changes to `hunk-browser`.
- [ ] Throttle frame notifications to 60fps for v1.
  - [x] Add a tested `BrowserFrameRateLimiter` primitive with a 60fps v1 target interval.
  - [ ] Wire the limiter into the production CEF `on_paint` adapter before notifying GPUI.
- [ ] Keep browser rendering work within the 8ms frame budget.
- [ ] Add compact AI timeline rows for browser activity such as navigation, click, type, scroll, screenshot, and confirmation-required events.

Exit criteria:

- [ ] The AI workspace displays a live in-app browser.
- [ ] Manual navigation works through the Hunk UI.
- [ ] The browser does not launch the user's default browser for normal web navigation.
- [ ] Browser activity is visible in the timeline without embedding the browser viewport inside timeline rows.

Implementation notes:

- The browser pane now converts the selected thread's latest `BrowserFrame` into a GPUI `RenderImage` and paints it in the right-side browser surface.
- The desktop adapter caches the `RenderImage` by thread ID, frame epoch, and dimensions so normal re-renders reuse the current frame instead of rebuilding the GPUI image object.
- This is still a UI paint path only; the production CEF adapter still needs to feed frames into `BrowserSession`.

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
  - [x] Route browser tool calls through a persistent state-only `hunk-browser::BrowserRuntime` in the AI worker.
  - [ ] Replace the state-only route with a UI/CEF bridge so calls operate on the visible embedded browser session.
- [x] Add browser tools:
  - [x] `hunk.browser_navigate`
  - [x] `hunk.browser_snapshot`
  - [x] `hunk.browser_click`
  - [x] `hunk.browser_type`
  - [x] `hunk.browser_press`
  - [x] `hunk.browser_scroll`
  - [x] `hunk.browser_screenshot`
- [x] Make `hunk.browser_snapshot` return URL, title, viewport, scroll position, visible text, and indexed interactive elements.
- [x] Store the latest element index map in the browser session.
- [x] Reject click/type actions when the index map is stale after navigation or page mutation.
- [x] Return concise action results to the model.
- [x] Return screenshots through the richest image-capable result format available in the current Codex protocol.
- [x] Add tests for tool schema generation, tool routing, and missing-browser errors.
- [x] Add tests for stale snapshot behavior in routed browser tool calls.

Implementation notes:

- Browser dynamic tools now validate arguments, classify sensitive actions, and route allowed calls into a persistent worker-owned `BrowserRuntime` when browser tools are enabled for the AI worker.
- `hunk.browser_navigate` updates the thread session state, `hunk.browser_snapshot` returns the latest state-layer snapshot shape, and click/type calls reject stale or unknown snapshot elements before any backend dispatch.
- This is intentionally not the final bridge: the worker-owned state runtime must be replaced by a UI/CEF-backed bridge before tool calls can control the live GPUI browser pane.

Exit criteria:

- [ ] The AI agent can navigate, inspect, click, type, press keys, scroll, and capture screenshots in the in-app browser.
- [ ] Tool calls operate on the embedded CEF browser, not an external browser.

## Phase 4: Sensitive Action Policy

- [x] Add a browser safety policy module.
- [ ] Prompt before credential submission, purchases, payments, file upload/download, permission prompts, external protocol launches, and high-risk form submissions.
  - [x] Return confirmation-required tool responses for sensitive browser actions detected by the state-layer executor.
- [ ] Route browser confirmations through the existing AI user-input or approval UI.
- [x] Redact secrets from tool results.
- [x] Block external protocol launches unless the user confirms.
- [x] Add tests for sensitive-action classification.
- [x] Add tests for confirmation-required tool responses.

Exit criteria:

- [ ] Normal browsing is smooth.
- [x] Sensitive actions pause for user confirmation.
- [x] Secret values are not echoed back to the model.

## Phase 5: Packaging

- [x] Add `assets/browser-runtime` to desktop package resources.
- [x] Add CEF runtime validation scripts for macOS first.
  - [x] Add `scripts/smoke_browser_cef_macos.sh` to clone/pin cef-rs, export CEF, build the cef-rs OSR bundle, and optionally launch it.
  - [x] Extend the smoke script to build and run a Hunk-owned OSR app bundle with helper apps and a nonblank pixel assertion.
  - [x] Add Hunk-specific runtime layout validation for release packaging.
- [x] Update macOS packaging to include:
  - [x] CEF framework/runtime files
  - [x] locales
  - [x] resources
  - [x] helper app/binary
  - [x] codesigning and notarization coverage
- [ ] Update Linux packaging after the macOS path is stable.
- [ ] Update Windows packaging after the Linux path is stable.
- [x] Extend release bundle validation to check browser runtime files.
- [x] Document how to refresh the pinned CEF runtime.

Exit criteria:

- [ ] A packaged macOS build can open the in-app browser without a development checkout.
- [x] Bundle validation fails clearly when required CEF files are missing.

Implementation notes:

- `scripts/validate_browser_cef_macos.sh` validates the staged runtime and, when given an app bundle path, validates the copied framework plus macOS CEF helper app layout.
- The smoke script runs the validator before launching the Hunk-owned CEF smoke app, so helper/framework packaging regressions fail before runtime startup.
- `scripts/package_browser_cef_macos.sh` copies the staged CEF framework into `Contents/Frameworks`, creates the macOS helper app variants from `hunk-browser-helper`, and runs the CEF bundle validator. `scripts/package_macos_release.sh` invokes it before signing.
- `scripts/package_macos_release.sh` signs nested CEF helper `.app` bundles and the CEF `.framework` before signing the top-level Hunk app and submitting the DMG to the existing notarization flow.
- `scripts/validate_release_bundle_layout.sh macos-app` now invokes the CEF app-bundle validator with the production `Hunk Browser` helper prefix.
- Package layout smoke passed against `target/browser-cef-smoke/package-validation.app` after building `hunk-browser-helper` with `--features hunk-browser-helper/cef-subprocess`.

## Phase 6: Final Verification

- [x] Add ignored integration tests that run only when the CEF runtime is installed.
- [x] Integration smoke test: load `https://example.com`.
- [x] Integration smoke test: verify a nonblank painted frame.
- [ ] Integration smoke test: forward click and key input.
- [ ] Integration smoke test: call `hunk.browser_snapshot`.
- [ ] Integration smoke test: capture a screenshot.
- [x] Run `nix develop -c cargo build --workspace`.
- [x] Run `nix develop -c cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Run targeted browser, Codex, and desktop tests.
- [x] Run macOS package smoke once the CEF runtime is staged.

Exit criteria:

- [ ] The in-app browser works in development.
- [ ] The AI agent can control the embedded browser.
- [x] The macOS packaged app includes everything needed to run the browser runtime.
- [x] Build, clippy, and targeted tests pass.
