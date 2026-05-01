# AI Android Emulator Control TODO

## Status

- Planned
- Owner: Hunk
- Last Updated: 2026-05-01

## Summary

This document tracks the Android-first implementation plan for agentic mobile emulator control in Hunk.

The goal is to let the AI agent control an Android Emulator in the same spirit as the current embedded browser tools:

- inspect the current screen
- receive an indexed map of visible interactive elements
- tap, type, swipe, and press system buttons
- install and launch app builds
- read screenshots and logs
- keep actions auditable in the AI timeline

Android comes first because it can work on macOS, Linux, and Windows through the Android SDK toolchain. iOS Simulator support is intentionally out of scope for this document and should be planned separately after the Android control loop is working.

## Product Decisions

- Android emulator control ships before iOS simulator control.
- Hunk does not bundle Android Studio, the Android SDK, or emulator system images for v1.
- Hunk discovers user-installed Android SDK tools from `ANDROID_HOME`, `ANDROID_SDK_ROOT`, common Android Studio SDK paths, and `PATH`.
- V1 targets Android Emulator instances, not physical Android devices.
- Physical devices can be added later behind explicit user confirmation because they carry more privacy and destructive-action risk.
- V1 uses direct Android SDK tools instead of requiring Appium, Maestro, Detox, or Node.
- V1 uses `adb`, `emulator`, `uiautomator`, `screencap`, `input`, `am`, and `logcat`.
- Use Rust `Command` with explicit argv. Do not invoke commands through shell strings.
- The AI-facing tool contract should mirror `hunk_browser`: snapshot first, then actions by `snapshotEpoch` and indexed element.
- The first visual surface can be screenshot-driven. A live embedded Android surface can follow after the tool loop is useful.
- Android Emulator gRPC is a later performance path for live frame streaming and input, not a dependency for v1.
- React Native support should fall out naturally through native accessibility metadata, `testID`, and standard Android view hierarchy exposure.

## Goals

- Let the agent discover available AVDs and running emulator devices.
- Let the agent start an AVD when the Android SDK emulator is installed.
- Let the agent select the active emulator for the current AI thread.
- Let the agent install an APK onto the active emulator.
- Let the agent launch an app by package name, with optional activity override.
- Let the agent inspect the visible UI through a parsed UI Automator hierarchy.
- Let the agent receive screenshots as image-capable dynamic tool output.
- Let the agent tap/type/swipe by indexed elements or raw coordinates.
- Let the agent press Android system/navigation keys such as Back, Home, Enter, and Recent Apps.
- Let the agent read bounded `logcat` output for the active emulator.
- Keep emulator sessions scoped by AI thread where feasible.
- Keep action results structured and concise.

## Non-Goals For V1

- iOS Simulator support.
- Physical Android device control.
- Cloud device farms.
- Bundling Android SDK components.
- Replacing Detox, Appium, Maestro, or project test suites.
- High-fps emulator rendering inside GPUI.
- Pixel-only computer-vision automation as the primary control mechanism.
- Full WebView DOM inspection.
- Reliable arbitrary Unicode text entry.
- Emulator creation or system image download flows.

## Architecture Boundary

### New Crate

- `crates/hunk-mobile`

Responsibilities:

- shared mobile session, device, snapshot, frame, and action types
- Android backend module for SDK tool discovery and command execution
- UI Automator XML parsing into Hunk snapshot elements
- screenshot frame metadata and PNG bytes
- action preflight and stale snapshot validation
- safety classification for mobile actions
- testable command construction without requiring an installed SDK

Non-responsibilities:

- GPUI rendering
- AI timeline projection
- dynamic tool schema registration
- app-level settings UI
- downloading SDKs or creating AVDs

### Existing Crates

- `crates/hunk-codex`
  - owns Android dynamic tool specs, developer instructions, argument parsing, and protocol-facing response shapes
- `crates/hunk-desktop`
  - owns the dynamic tool executor bridge, pending confirmations, GPUI mobile pane, and visible session routing
- `crates/hunk-domain`
  - may own persisted preferences such as last selected AVD or Android SDK path override if needed

## Runtime Model

V1 should use a process-backed Android runtime similar in shape to `BrowserRuntime`, but without requiring a live backend for unit tests.

Suggested core types:

- `MobileRuntime`
- `AndroidRuntime`
- `MobileSession`
- `MobileDeviceId`
- `MobileDeviceSummary`
- `MobileSnapshot`
- `MobileElement`
- `MobileFrame`
- `MobileAction`
- `SensitiveMobileAction`

Session state should include:

- selected device serial
- selected AVD name, if known
- current app package, if known
- latest UI snapshot
- latest screenshot frame
- snapshot epoch
- last logcat cursor or timestamp, if supported

## Android Backend Strategy

### Tool Discovery

Resolve these executables:

- `adb`
- `emulator`

Search order:

1. explicit Hunk setting or environment override, if added
2. `ANDROID_HOME`
3. `ANDROID_SDK_ROOT`
4. common SDK roots
   - macOS: `$HOME/Library/Android/sdk`
   - Linux: `$HOME/Android/Sdk`
   - Windows: `%LOCALAPPDATA%/Android/Sdk`
5. `PATH`

Validation:

- `adb version`
- `emulator -version`
- `emulator -list-avds`

### Device Discovery

Use:

- `adb devices -l`
- `emulator -list-avds`

Return:

- running emulator serials, for example `emulator-5554`
- connection state
- model/device metadata from `adb devices -l`
- known AVD names
- selected device for the current AI thread

### Starting Emulators

Use:

- `emulator -avd <name>`

V1 starts a normal emulator window. Later phases can add:

- headless mode
- gRPC port allocation
- Hunk-owned emulator process lifecycle
- quick-boot snapshot policy

The runtime should treat emulator startup as asynchronous:

- spawn process
- poll `adb devices`
- wait for `sys.boot_completed=1`
- surface timeout and boot errors as structured tool output

### App Install And Launch

Install:

- `adb -s <serial> install -r <apk-path>`

Launch:

- prefer `adb -s <serial> shell monkey -p <package> 1` when only a package is known
- support `adb -s <serial> shell am start -n <package>/<activity>` when the activity is provided
- support optional deep link launch later with `am start -a android.intent.action.VIEW -d <url>`

### Snapshot

Use UI Automator for the semantic snapshot:

1. `adb -s <serial> shell uiautomator dump /sdcard/hunk-window.xml`
2. `adb -s <serial> exec-out cat /sdcard/hunk-window.xml`
3. parse XML with a structured XML parser
4. optionally delete the temporary file

Parse node attributes:

- `index`
- `text`
- `resource-id`
- `class`
- `package`
- `content-desc`
- `bounds`
- `clickable`
- `enabled`
- `focusable`
- `focused`
- `scrollable`
- `selected`
- `checked`

Return Hunk elements with:

- stable per-snapshot index
- role inferred from class and booleans
- label from `content-desc`, `text`, or `resource-id`
- text
- bounds
- enabled/selected/checked state
- package/resource id metadata

Use screenshot data for visual context:

- `adb -s <serial> exec-out screencap -p`

The `snapshot` tool should return both:

- structured JSON with visible text, viewport, and elements
- attached screenshot image when the protocol supports image output

### Actions

Element actions should validate the `snapshotEpoch` before executing.

Tap:

- indexed element center through `adb shell input tap <x> <y>`
- optional raw coordinate tap for fallback

Type:

- tap target first when an element index is provided
- V1: use `adb shell input text <escaped-text>` for simple text
- Later: add a helper IME or clipboard-based path for reliable Unicode and multiline input

Swipe:

- `adb shell input swipe <x1> <y1> <x2> <y2> <duration-ms>`
- support named directions relative to viewport or an element bounds

Press:

- `adb shell input keyevent <keycode>`
- support names such as `Back`, `Home`, `Enter`, `Tab`, `Escape`, `Delete`, `RecentApps`, `Power`, and `VolumeUp`

Logs:

- bounded `adb -s <serial> logcat -d`
- optional filters by package, priority, tag, and since timestamp
- redact likely secrets before returning tool output

## Dynamic Tool Namespace

Use `hunk_android` for the Android-first tool namespace. Keep the crate model generic enough that `hunk_ios` or a future `hunk_mobile` namespace can share internals later.

Initial tools:

- `hunk_android.devices`
- `hunk_android.start`
- `hunk_android.select_device`
- `hunk_android.install_apk`
- `hunk_android.launch`
- `hunk_android.snapshot`
- `hunk_android.screenshot`
- `hunk_android.tap`
- `hunk_android.type`
- `hunk_android.press`
- `hunk_android.swipe`
- `hunk_android.logs`

Possible later tools:

- `hunk_android.rotate`
- `hunk_android.set_location`
- `hunk_android.set_network`
- `hunk_android.clear_app_data`
- `hunk_android.uninstall`
- `hunk_android.open_deep_link`
- `hunk_android.record_video`

Developer instructions should tell the agent:

- use `hunk_android.devices` before assuming a device exists
- use `hunk_android.snapshot` before tap/type/swipe actions that target elements
- pass the latest `snapshotEpoch` and element index to element actions
- use Android tools instead of external Appium, Maestro, Detox, or raw shell commands when the user asks to control the Android Emulator
- stop and wait for user confirmation when a mobile action reports confirmation required

## GPUI Surface

V1 can be tool-first, but the user experience should quickly add a visible mobile pane.

### Screenshot Pane

Render:

- active device name and serial
- current package, if known
- latest screenshot
- loading/error state
- agent-control indicator
- compact controls for refresh screenshot, snapshot, Back, Home, and Recent Apps

Interaction:

- clicking the screenshot can send raw coordinate taps when the pane is active
- resize should not affect device coordinates; translate from painted image rect to screenshot pixel coordinates
- keep manual input auditable if it mutates the emulator through Hunk

### Later Live Pane

Use Android Emulator gRPC after the ADB tool loop is stable:

- launch emulator with a private gRPC port and token/JWT protection where available
- stream screenshots or display frames
- send input events through gRPC
- keep ADB/UI Automator as the semantic snapshot source unless gRPC exposes a better tree

## Safety Policy

Prompt before:

- installing APKs outside the current workspace
- controlling a physical Android device
- entering likely secrets
- payment or purchase flows
- Play Store account actions
- adding/removing accounts
- changing app permissions
- sending SMS or placing calls
- setting location
- wiping emulator data or clearing app data
- uninstalling apps
- file push/pull outside an approved workspace boundary

Always redact likely secrets from:

- typed text echoed in tool output
- UI text
- logcat output
- resource IDs or command output containing tokens

## Testing Plan

Crate-level tests should live under `crates/hunk-mobile/tests`.

Unit tests:

- Android SDK path discovery with fake filesystem fixtures
- command argv construction without shell escaping
- `adb devices -l` parsing
- `emulator -list-avds` parsing
- UI Automator XML parsing
- bounds parsing
- role and label inference
- visible text extraction
- stale snapshot rejection
- text escaping for `adb shell input text`
- safety classification
- log redaction

Codex tests:

- dynamic tool schema generation
- argument parsing
- unsupported tool errors
- missing SDK errors
- missing device errors
- confirmation-required responses
- snapshot response shape
- screenshot image output shape

Desktop tests:

- dynamic executor routes Android tools without breaking workspace or browser tools
- pending confirmations reuse the existing approval path
- timeline projection renders compact Android tool rows

Optional integration tests:

- gated behind an environment variable such as `HUNK_ANDROID_EMULATOR_TEST_SERIAL`
- never required for normal workspace CI
- verify snapshot, screenshot, tap, type, press Back, and log capture against a running emulator

Final implementation validation:

- run workspace build once
- run workspace clippy once
- run workspace tests once

## Phased TODOs

### Phase 0: Android Control Spike

- [ ] Verify `adb devices -l` parsing on macOS, Linux, and Windows.
- [ ] Verify `emulator -list-avds` parsing on macOS, Linux, and Windows.
- [ ] Verify `uiautomator dump` output on a current emulator image.
- [ ] Verify `adb exec-out screencap -p` returns usable PNG bytes on all target platforms.
- [ ] Verify `input tap`, `input swipe`, `input keyevent`, and `input text` behavior on a standard emulator.
- [ ] Document text-entry limitations found during the spike.

Exit criteria:

- [ ] A local script or scratch tool can snapshot, screenshot, tap, type, and press Back on a running emulator.

### Phase 1: `hunk-mobile` Core Types

- [ ] Add `crates/hunk-mobile`.
- [ ] Add mobile device/session/snapshot/action/frame types.
- [ ] Add Android UI Automator XML parser.
- [ ] Add bounds parser for `[x1,y1][x2,y2]`.
- [ ] Add snapshot element indexing.
- [ ] Add stale snapshot validation.
- [ ] Add screenshot frame metadata type.
- [ ] Add safety classification and redaction helpers.
- [ ] Add crate-level parser and safety tests.

Exit criteria:

- [ ] Android snapshots are represented in Hunk-owned types without needing a live emulator.

### Phase 2: Android SDK Backend

- [ ] Add Android SDK tool discovery.
- [ ] Add structured command runner using explicit argv.
- [ ] Add AVD listing.
- [ ] Add running emulator listing.
- [ ] Add emulator startup and boot polling.
- [ ] Add active device selection.
- [ ] Add APK install.
- [ ] Add package/activity launch.
- [ ] Add UI snapshot capture.
- [ ] Add screenshot capture.
- [ ] Add tap/type/swipe/keyevent actions.
- [ ] Add bounded logcat capture.
- [ ] Add structured backend errors.
- [ ] Add backend tests using fake command output.

Exit criteria:

- [ ] The backend can control a running emulator from Rust with structured results and no shell-string execution.

### Phase 3: Dynamic Android Tools

- [ ] Add `hunk-codex/src/android_tools.rs`.
- [ ] Register `hunk_android` tool specs in Android-enabled AI threads.
- [ ] Add Android developer instructions.
- [ ] Add typed dynamic tool request parsing.
- [ ] Add dynamic tool response helpers.
- [ ] Add missing-SDK and missing-device responses.
- [ ] Add screenshot image output support.
- [ ] Add tests for schemas, parsing, and response shapes.

Exit criteria:

- [ ] The model can call Android dynamic tools through the Codex protocol without desktop UI coupling.

### Phase 4: Desktop Executor Bridge

- [ ] Extend `AiDynamicToolExecutor` to include Android tools alongside workspace and browser tools.
- [ ] Keep browser tool routing unchanged.
- [ ] Add Android runtime ownership to the AI worker or visible workspace bridge.
- [ ] Decide whether v1 Android calls must route through visible GPUI state or can run directly in the worker.
- [ ] Add pending confirmation support for sensitive Android actions.
- [ ] Add compact Android tool rows to AI timeline projection.
- [ ] Add tests for routing and confirmation-required behavior.

Exit criteria:

- [ ] AI Android tool calls execute from an AI thread and return structured results to the model.

### Phase 5: GPUI Mobile Screenshot Pane

- [ ] Add AI workspace mobile pane state.
- [ ] Add Android pane mode beside existing companion pane modes.
- [ ] Render latest screenshot frame.
- [ ] Render active device metadata.
- [ ] Add refresh/snapshot, Back, Home, and Recent Apps controls.
- [ ] Add click-to-tap coordinate translation.
- [ ] Use colors from `crates/hunk-desktop/src/app/theme.rs`.
- [ ] Keep screenshot decode and scaling work outside the GPUI render hot path.
- [ ] Keep frame work within the 8ms frame budget.

Exit criteria:

- [ ] The user can see and manually interact with the active emulator through Hunk while the agent can also control it.

### Phase 6: Android Emulator gRPC Live Surface

- [ ] Research current emulator gRPC auth/token behavior in installed SDK versions.
- [ ] Add optional gRPC port allocation when Hunk starts the emulator.
- [ ] Add gRPC screenshot/frame streaming prototype.
- [ ] Add gRPC input event prototype.
- [ ] Compare latency against `adb screencap` plus `adb input`.
- [ ] Decide whether gRPC becomes the live-pane backend while ADB remains the semantic/action fallback.

Exit criteria:

- [ ] Hunk can render a low-latency live emulator surface without relying on repeated `adb screencap`.

### Phase 7: Documentation And Settings

- [ ] Add user-facing setup docs for Android SDK requirements.
- [ ] Add troubleshooting for missing `adb`, missing `emulator`, no AVDs, offline devices, and boot timeouts.
- [ ] Add optional setting for Android SDK path override.
- [ ] Add optional setting for default AVD.
- [ ] Add notes for React Native teams:
  - [ ] prefer `testID` on important controls
  - [ ] set `accessibilityLabel` for icon-only controls
  - [ ] set `accessibilityRole` for buttons, inputs, tabs, switches, and lists
  - [ ] avoid hiding important children from accessibility unless intentional

Exit criteria:

- [ ] A developer with Android Studio installed can enable the feature and understand how to make their app automation-friendly.

## Recommended First Implementation Order

1. Add `crates/hunk-mobile` with Android snapshot types and XML parsing.
2. Add Android SDK discovery and fake-command-output tests.
3. Implement snapshot and screenshot capture against a selected running emulator.
4. Add `hunk_android.devices`, `hunk_android.select_device`, `hunk_android.snapshot`, and `hunk_android.screenshot`.
5. Add tap, press, swipe, and simple type tools.
6. Add install and launch tools.
7. Add logcat tool.
8. Add desktop confirmation routing and timeline rows.
9. Add the GPUI screenshot pane.
10. Revisit gRPC for a live embedded surface.

## Open Questions

- Should V1 expose Android tools only when an Android SDK is detected, or always expose them with clear missing-SDK errors?
- Should Android runtime state live in the AI worker first, or route immediately through visible desktop state like the CEF browser backend?
- Should Hunk support existing already-running emulators before supporting Hunk-started emulator lifecycle?
- Should text entry use only `adb shell input text` in v1, or should a tiny helper IME be part of the first production version?
- Should physical Android devices be a separate explicit mode rather than a later option on the same tools?
- Should React Native setup docs recommend `testID` only, or both `testID` and accessibility labels for better user-facing accessibility?

## References

- Android Debug Bridge: https://developer.android.com/tools/adb
- Android Emulator command line: https://developer.android.com/studio/run/emulator-commandline
- Android Emulator console: https://developer.android.com/studio/run/emulator-console
- Android UI Automator: https://developer.android.com/training/testing/other-components/ui-automator
- Android `UiDevice` API: https://developer.android.com/reference/androidx/test/uiautomator/UiDevice
- Android Emulator gRPC notes: https://developer.android.com/studio/releases/emulator
- React Native accessibility: https://reactnative.dev/docs/accessibility.html
- React Native `testID`: https://reactnative.dev/docs/view.html#testid
