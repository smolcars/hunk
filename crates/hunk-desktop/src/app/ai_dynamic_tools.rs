use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use hunk_browser::{
    BrowserAction, BrowserRuntime, BrowserSafetyDecision, SensitiveBrowserAction,
    classify_browser_action, redact_browser_tool_text,
};
use hunk_codex::android_tools::{AndroidDynamicToolRequest, parse_android_dynamic_tool_request};
use hunk_codex::browser_tools::{BrowserDynamicToolRequest, parse_browser_dynamic_tool_request};
use hunk_codex::protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
};
use hunk_codex::tools::DynamicToolRegistry;
use hunk_mobile::{
    AndroidAction, AndroidRuntime, MobileSafetyDecision, classify_android_action,
    redact_mobile_tool_text,
};
use image::ImageEncoder as _;
use serde_json::json;

#[derive(Debug, Clone)]
pub(crate) struct AiDynamicToolExecutor {
    workspace_tools: DynamicToolRegistry,
    browser_tools: AiBrowserDynamicToolExecutor,
    android_tools: AiAndroidDynamicToolExecutor,
}

impl Default for AiDynamicToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl AiDynamicToolExecutor {
    pub(crate) fn new() -> Self {
        Self {
            workspace_tools: DynamicToolRegistry::new(),
            browser_tools: AiBrowserDynamicToolExecutor::disabled(),
            android_tools: AiAndroidDynamicToolExecutor::new(),
        }
    }

    pub(crate) fn with_state_only_browser() -> Self {
        Self {
            workspace_tools: DynamicToolRegistry::new(),
            browser_tools: AiBrowserDynamicToolExecutor::state_only(),
            android_tools: AiAndroidDynamicToolExecutor::new(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_browser_runtime(runtime: BrowserRuntime) -> Self {
        Self {
            workspace_tools: DynamicToolRegistry::new(),
            browser_tools: AiBrowserDynamicToolExecutor {
                runtime: Some(runtime),
            },
            android_tools: AiAndroidDynamicToolExecutor::new(),
        }
    }

    pub(crate) fn execute(
        &mut self,
        cwd: &Path,
        params: &DynamicToolCallParams,
    ) -> DynamicToolCallResponse {
        if hunk_codex::browser_tools::is_browser_dynamic_tool_call(
            params.namespace.as_deref(),
            params.tool.as_str(),
        ) {
            return self.browser_tools.execute(params);
        }

        if hunk_codex::android_tools::is_android_dynamic_tool_call(
            params.namespace.as_deref(),
            params.tool.as_str(),
        ) {
            return self.android_tools.execute(cwd, params);
        }

        self.workspace_tools.execute(cwd, params)
    }
}

#[derive(Debug, Clone)]
struct AiAndroidDynamicToolExecutor {
    runtime: AndroidRuntime,
}

impl AiAndroidDynamicToolExecutor {
    fn new() -> Self {
        Self {
            runtime: AndroidRuntime::new_auto(),
        }
    }

    fn execute(&mut self, cwd: &Path, params: &DynamicToolCallParams) -> DynamicToolCallResponse {
        execute_android_dynamic_tool_with_runtime(cwd, &mut self.runtime, params)
    }
}

fn execute_android_dynamic_tool_with_runtime(
    cwd: &Path,
    runtime: &mut AndroidRuntime,
    params: &DynamicToolCallParams,
) -> DynamicToolCallResponse {
    let request = match parse_android_dynamic_tool_request(params) {
        Ok(request) => request,
        Err(error) => {
            return json_error_response(json!({
                "error": "invalidAndroidToolArguments",
                "message": error,
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
            }));
        }
    };

    match request {
        AndroidDynamicToolRequest::Devices => match runtime.devices() {
            Ok(inventory) => json_success_response(json!({
                "ok": true,
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
                "inventory": inventory,
                "message": "Android emulator devices and AVDs were read.",
            })),
            Err(error) => android_tool_error_response(params, "androidDevicesFailed", error),
        },
        AndroidDynamicToolRequest::Start {
            avd_name,
            wait_for_boot,
            timeout_seconds,
        } => match runtime.start_avd(
            avd_name.as_str(),
            wait_for_boot,
            timeout_seconds.map(std::time::Duration::from_secs),
        ) {
            Ok(inventory) => json_success_response(json!({
                "ok": true,
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
                "avdName": avd_name,
                "waitForBoot": wait_for_boot,
                "inventory": inventory,
                "message": "Android emulator start was requested.",
            })),
            Err(error) => android_tool_error_response(params, "androidStartFailed", error),
        },
        AndroidDynamicToolRequest::SelectDevice { device_id } => {
            runtime.select_device(params.thread_id.as_str(), device_id.clone());
            json_success_response(json!({
                "ok": true,
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
                "deviceId": device_id,
                "message": "Android emulator device was selected for this AI thread.",
            }))
        }
        AndroidDynamicToolRequest::InstallApk {
            device_id,
            apk_path,
        } => {
            let apk_path = match resolve_workspace_apk_path(cwd, apk_path.as_str()) {
                Ok(path) => path,
                Err(message) => {
                    return json_error_response(json!({
                        "error": "androidInstallRejected",
                        "message": message,
                        "tool": params.tool,
                        "threadId": params.thread_id,
                        "turnId": params.turn_id,
                    }));
                }
            };
            match runtime.install_apk(params.thread_id.as_str(), device_id.as_ref(), &apk_path) {
                Ok(output) => json_success_response(json!({
                    "ok": true,
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                    "deviceId": device_id,
                    "apkPath": apk_path,
                    "output": redact_mobile_tool_text(output.as_str()),
                    "message": "APK install was sent to the Android emulator.",
                })),
                Err(error) => android_tool_error_response(params, "androidInstallFailed", error),
            }
        }
        AndroidDynamicToolRequest::Launch {
            device_id,
            package,
            activity,
        } => match runtime.launch_package(
            params.thread_id.as_str(),
            device_id.as_ref(),
            package.as_str(),
            activity.as_deref(),
        ) {
            Ok(output) => json_success_response(json!({
                "ok": true,
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
                "deviceId": device_id,
                "package": package,
                "activity": activity,
                "output": redact_mobile_tool_text(output.as_str()),
                "message": "Android app launch was sent to the emulator.",
            })),
            Err(error) => android_tool_error_response(params, "androidLaunchFailed", error),
        },
        AndroidDynamicToolRequest::Snapshot { device_id } => {
            let snapshot =
                match runtime.capture_snapshot(params.thread_id.as_str(), device_id.as_ref()) {
                    Ok(snapshot) => snapshot.clone(),
                    Err(error) => {
                        return android_tool_error_response(params, "androidSnapshotFailed", error);
                    }
                };
            let screenshot = runtime
                .capture_screenshot(params.thread_id.as_str(), device_id.as_ref())
                .ok()
                .cloned();
            let response = android_snapshot_response(params, &snapshot, screenshot.as_ref());
            if let Some(frame) = screenshot.as_ref()
                && let Some(image_url) = android_frame_png_data_url(frame)
            {
                return json_success_response_with_items(
                    response,
                    vec![DynamicToolCallOutputContentItem::InputImage { image_url }],
                );
            }
            json_success_response(response)
        }
        AndroidDynamicToolRequest::Screenshot { device_id } => {
            match runtime.capture_screenshot(params.thread_id.as_str(), device_id.as_ref()) {
                Ok(frame) => {
                    let Some(image_url) = android_frame_png_data_url(frame) else {
                        return json_error_response(json!({
                            "error": "androidScreenshotEncodingFailed",
                            "message": "The Android screenshot could not be encoded as an input image.",
                            "tool": params.tool,
                            "threadId": params.thread_id,
                            "turnId": params.turn_id,
                        }));
                    };
                    json_success_response_with_items(
                        json!({
                            "ok": true,
                            "tool": params.tool,
                            "threadId": params.thread_id,
                            "turnId": params.turn_id,
                            "deviceId": device_id,
                            "frame": frame.metadata(),
                            "message": "Android emulator screenshot is attached as an input image.",
                        }),
                        vec![DynamicToolCallOutputContentItem::InputImage { image_url }],
                    )
                }
                Err(error) => android_tool_error_response(params, "androidScreenshotFailed", error),
            }
        }
        AndroidDynamicToolRequest::Logs {
            device_id,
            max_lines,
        } => match runtime.logcat(params.thread_id.as_str(), device_id.as_ref(), max_lines) {
            Ok(lines) => json_success_response(json!({
                "ok": true,
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
                "deviceId": device_id,
                "lines": lines
                    .iter()
                    .map(|line| redact_mobile_tool_text(line.as_str()))
                    .collect::<Vec<_>>(),
                "message": "Recent Logcat output was read from the Android emulator.",
            })),
            Err(error) => android_tool_error_response(params, "androidLogsFailed", error),
        },
        AndroidDynamicToolRequest::Action { device_id, action } => {
            if let MobileSafetyDecision::Prompt(kind) = classify_android_action(&action) {
                return json_error_response(json!({
                    "error": "androidConfirmationRequired",
                    "message": "This Android action requires user confirmation before it can run.",
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                    "sensitiveAction": format!("{kind:?}"),
                }));
            }
            match runtime.apply_action(params.thread_id.as_str(), device_id.as_ref(), &action) {
                Ok(()) => json_success_response(json!({
                    "ok": true,
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                    "deviceId": device_id,
                    "action": android_action_label(&action),
                    "message": android_action_message(&action),
                })),
                Err(error) => android_tool_error_response(params, "androidActionFailed", error),
            }
        }
    }
}

fn android_tool_error_response(
    params: &DynamicToolCallParams,
    error: &str,
    source: hunk_mobile::MobileError,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": error,
        "message": source.to_string(),
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

fn android_snapshot_response(
    params: &DynamicToolCallParams,
    snapshot: &hunk_mobile::MobileSnapshot,
    frame: Option<&hunk_mobile::MobileFrame>,
) -> serde_json::Value {
    let visible_text = snapshot
        .elements
        .iter()
        .flat_map(|element| [element.label.as_str(), element.text.as_str()])
        .filter(|text| !text.trim().is_empty())
        .map(redact_mobile_tool_text)
        .collect::<Vec<_>>()
        .join("\n");
    let elements = snapshot
        .elements
        .iter()
        .map(|element| {
            json!({
                "index": element.index,
                "role": element.role,
                "label": redact_mobile_tool_text(element.label.as_str()),
                "text": redact_mobile_tool_text(element.text.as_str()),
                "rect": element.rect,
                "enabled": element.enabled,
                "clickable": element.clickable,
                "focusable": element.focusable,
                "focused": element.focused,
                "scrollable": element.scrollable,
                "selected": element.selected,
                "checked": element.checked,
                "resourceId": element.resource_id,
                "className": element.class_name,
                "packageName": element.package_name,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "deviceId": snapshot.device_id,
        "snapshotEpoch": snapshot.epoch,
        "viewport": snapshot.viewport,
        "visibleText": visible_text,
        "elements": elements,
        "latestFrame": frame.map(|frame| frame.metadata().clone()),
        "message": "Android emulator UI snapshot was read.",
    })
}

fn android_action_label(action: &AndroidAction) -> &'static str {
    match action {
        AndroidAction::Tap { .. } => "tap",
        AndroidAction::Type { .. } => "type",
        AndroidAction::Press { .. } => "press",
        AndroidAction::Swipe { .. } => "swipe",
    }
}

fn android_action_message(action: &AndroidAction) -> &'static str {
    match action {
        AndroidAction::Tap { .. } => "Tap was sent to the Android emulator.",
        AndroidAction::Type { .. } => "Text input was sent to the Android emulator.",
        AndroidAction::Press { .. } => "Key press was sent to the Android emulator.",
        AndroidAction::Swipe { .. } => "Swipe was sent to the Android emulator.",
    }
}

fn resolve_workspace_apk_path(cwd: &Path, apk_path: &str) -> Result<std::path::PathBuf, String> {
    let candidate = Path::new(apk_path);
    let target = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        cwd.join(candidate)
    };
    let workspace_root = std::fs::canonicalize(cwd).map_err(|error| {
        format!(
            "failed to resolve workspace root '{}': {error}",
            cwd.display()
        )
    })?;
    let resolved = std::fs::canonicalize(&target)
        .map_err(|error| format!("failed to resolve APK path '{}': {error}", target.display()))?;
    if !resolved.starts_with(&workspace_root) {
        return Err(format!(
            "APK path '{}' is outside the workspace; v1 only installs workspace APKs",
            target.display()
        ));
    }
    if resolved
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("apk"))
    {
        return Err(format!(
            "APK path '{}' does not end in .apk",
            target.display()
        ));
    }
    Ok(resolved)
}

fn android_frame_png_data_url(frame: &hunk_mobile::MobileFrame) -> Option<String> {
    Some(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(frame.png())
    ))
}

#[derive(Debug, Clone)]
struct AiBrowserDynamicToolExecutor {
    runtime: Option<BrowserRuntime>,
}

impl AiBrowserDynamicToolExecutor {
    fn disabled() -> Self {
        Self { runtime: None }
    }

    fn state_only() -> Self {
        Self {
            runtime: Some(BrowserRuntime::new_disabled()),
        }
    }

    fn execute(&mut self, params: &DynamicToolCallParams) -> DynamicToolCallResponse {
        let Some(runtime) = self.runtime.as_mut() else {
            return browser_unavailable_response(
                params,
                "The embedded browser executor is not connected yet.",
            );
        };
        execute_browser_dynamic_tool_with_runtime(runtime, params, false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserToolSafetyMode {
    Enforce,
    AllowSensitiveOnce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserToolConfirmation {
    pub kind: SensitiveBrowserAction,
    pub summary: String,
}

pub(crate) fn execute_browser_dynamic_tool_with_runtime(
    runtime: &mut BrowserRuntime,
    params: &DynamicToolCallParams,
    use_backend: bool,
) -> DynamicToolCallResponse {
    execute_browser_dynamic_tool_with_runtime_and_safety(
        runtime,
        params,
        use_backend,
        BrowserToolSafetyMode::Enforce,
    )
}

pub(crate) fn execute_browser_dynamic_tool_with_runtime_and_safety(
    runtime: &mut BrowserRuntime,
    params: &DynamicToolCallParams,
    use_backend: bool,
    safety_mode: BrowserToolSafetyMode,
) -> DynamicToolCallResponse {
    let request = match parse_browser_dynamic_tool_request(params) {
        Ok(request) => request,
        Err(error) => {
            return json_error_response(json!({
            "error": "invalidBrowserToolArguments",
            "message": error,
            "tool": params.tool,
            "threadId": params.thread_id,
            "turnId": params.turn_id,
            }));
        }
    };

    match request {
        BrowserDynamicToolRequest::Snapshot { tab_id } => {
            if let Some(error) = select_browser_tool_tab(runtime, params, tab_id.as_ref()) {
                return error;
            }
            if use_backend {
                if let Err(error) = runtime.capture_backend_snapshot(params.thread_id.as_str()) {
                    return json_error_response(json!({
                        "error": "browserSnapshotFailed",
                        "message": error.to_string(),
                        "tool": params.tool,
                        "threadId": params.thread_id,
                        "turnId": params.turn_id,
                    }));
                }
                let _ = runtime.pump_backend();
            }
            let session = runtime.ensure_session(params.thread_id.clone());
            json_success_response(snapshot_response(params, session))
        }
        BrowserDynamicToolRequest::Screenshot { tab_id } => {
            if let Some(error) = select_browser_tool_tab(runtime, params, tab_id.as_ref()) {
                return error;
            }
            let session = runtime.ensure_session(params.thread_id.clone());
            let Some(frame) = session.latest_frame() else {
                return json_error_response(json!({
                    "error": "browserScreenshotUnavailable",
                    "message": "No browser frame has been captured yet.",
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                }));
            };
            let Some(image_url) = browser_frame_png_data_url(frame) else {
                return json_error_response(json!({
                    "error": "browserScreenshotEncodingFailed",
                    "message": "The latest browser frame could not be encoded as a PNG image.",
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                }));
            };
            json_success_response_with_items(
                json!({
                    "ok": true,
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                    "frame": frame.metadata(),
                    "message": "Screenshot frame is attached as an input image.",
                }),
                vec![DynamicToolCallOutputContentItem::InputImage { image_url }],
            )
        }
        BrowserDynamicToolRequest::Tabs => {
            let session = runtime.ensure_session(params.thread_id.clone());
            json_success_response(tabs_response(params, session, "Browser tabs were read."))
        }
        BrowserDynamicToolRequest::NewTab { url, activate } => {
            let tab_id = runtime.create_tab(params.thread_id.as_str(), url.clone(), activate);
            if let Some(url) = url {
                if use_backend && runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready {
                    let _ = runtime.navigate_backend_tab(params.thread_id.as_str(), &tab_id, url);
                    let _ = runtime.pump_backend();
                } else {
                    let _ =
                        runtime.navigate_tab_state_only(params.thread_id.as_str(), &tab_id, url);
                }
            } else if use_backend
                && activate
                && runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready
            {
                let _ = runtime.ensure_backend_session(params.thread_id.clone());
                let _ = runtime.pump_backend();
            }
            let session = runtime.ensure_session(params.thread_id.clone());
            json_success_response(tabs_response(
                params,
                session,
                format!("Created browser tab {}.", tab_id.as_str()).as_str(),
            ))
        }
        BrowserDynamicToolRequest::SelectTab { tab_id } => {
            if let Err(error) = runtime.select_tab(params.thread_id.as_str(), &tab_id) {
                return json_error_response(json!({
                    "error": "browserActionRejected",
                    "message": error.to_string(),
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                }));
            }
            if use_backend && runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready {
                let _ = runtime.ensure_backend_session(params.thread_id.clone());
                let _ = runtime.pump_backend();
            }
            let session = runtime.ensure_session(params.thread_id.clone());
            json_success_response(tabs_response(
                params,
                session,
                format!("Selected browser tab {}.", tab_id.as_str()).as_str(),
            ))
        }
        BrowserDynamicToolRequest::CloseTab { tab_id } => {
            if let Err(error) = runtime.close_tab(params.thread_id.as_str(), &tab_id) {
                return json_error_response(json!({
                    "error": "browserActionRejected",
                    "message": error.to_string(),
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                }));
            }
            if use_backend && runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready {
                let _ = runtime.ensure_backend_session(params.thread_id.clone());
                let _ = runtime.pump_backend();
            }
            let session = runtime.ensure_session(params.thread_id.clone());
            json_success_response(tabs_response(
                params,
                session,
                format!("Closed browser tab {}.", tab_id.as_str()).as_str(),
            ))
        }
        BrowserDynamicToolRequest::Console {
            tab_id,
            level,
            since_sequence,
            limit,
        } => {
            if let Some(error) = select_browser_tool_tab(runtime, params, tab_id.as_ref()) {
                return error;
            }
            if use_backend {
                let _ = runtime.pump_backend();
            }
            let session = runtime.ensure_session(params.thread_id.clone());
            let tab_id = session.active_tab_id().clone();
            json_success_response(console_response(
                params,
                session,
                &tab_id,
                level,
                since_sequence,
                limit,
            ))
        }
        BrowserDynamicToolRequest::Action { tab_id, action } => {
            if safety_mode == BrowserToolSafetyMode::Enforce
                && let BrowserSafetyDecision::Prompt(kind) = classify_browser_action(&action)
            {
                return json_error_response(json!({
                    "error": "browserConfirmationRequired",
                    "message": "This browser action requires user confirmation before it can run.",
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                    "sensitiveAction": format!("{kind:?}"),
                }));
            }
            if let Some(error) = select_browser_tool_tab(runtime, params, tab_id.as_ref()) {
                return error;
            }

            let action_result =
                if use_backend && runtime.status() == hunk_browser::BrowserRuntimeStatus::Ready {
                    runtime.apply_backend_action(params.thread_id.as_str(), &action)
                } else {
                    runtime.apply_state_only_action(params.thread_id.as_str(), &action)
                };

            if use_backend {
                let _ = runtime.pump_backend();
            }

            match action_result {
                Ok(()) => {
                    let session = runtime.ensure_session(params.thread_id.clone());
                    json_success_response(action_response(
                        params,
                        &action,
                        session.state(),
                        use_backend,
                    ))
                }
                Err(error) => json_error_response(json!({
                    "error": "browserActionRejected",
                    "message": error.to_string(),
                    "tool": params.tool,
                    "threadId": params.thread_id,
                    "turnId": params.turn_id,
                })),
            }
        }
    }
}

pub(crate) fn browser_dynamic_tool_confirmation(
    params: &DynamicToolCallParams,
) -> Option<BrowserToolConfirmation> {
    let Ok(BrowserDynamicToolRequest::Action { action, .. }) =
        parse_browser_dynamic_tool_request(params)
    else {
        return None;
    };
    let BrowserSafetyDecision::Prompt(kind) = classify_browser_action(&action) else {
        return None;
    };

    Some(BrowserToolConfirmation {
        kind,
        summary: browser_action_summary(&action),
    })
}

pub(crate) fn browser_unavailable_response(
    params: &DynamicToolCallParams,
    message: &str,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "browserUnavailable",
        "message": message,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

pub(crate) fn browser_confirmation_declined_response(
    params: &DynamicToolCallParams,
) -> DynamicToolCallResponse {
    json_error_response(json!({
        "error": "browserConfirmationDeclined",
        "message": "The user declined this browser action.",
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
    }))
}

fn select_browser_tool_tab(
    runtime: &mut BrowserRuntime,
    params: &DynamicToolCallParams,
    tab_id: Option<&hunk_browser::BrowserTabId>,
) -> Option<DynamicToolCallResponse> {
    let tab_id = tab_id?;
    runtime
        .select_tab(params.thread_id.as_str(), tab_id)
        .err()
        .map(|error| {
            json_error_response(json!({
                "error": "browserActionRejected",
                "message": error.to_string(),
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
            }))
        })
}

fn action_response(
    params: &DynamicToolCallParams,
    action: &BrowserAction,
    state: &hunk_browser::BrowserSessionState,
    use_backend: bool,
) -> serde_json::Value {
    json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "action": browser_action_label(action),
        "url": state.url,
        "title": state.title,
        "loading": state.loading,
        "snapshotEpoch": state.snapshot_epoch,
        "activeTabId": state.active_tab_id,
        "tabs": browser_tabs_value(state),
        "message": browser_action_message(action, use_backend),
    })
}

fn snapshot_response(
    params: &DynamicToolCallParams,
    session: &hunk_browser::BrowserSession,
) -> serde_json::Value {
    let state = session.state();
    let snapshot = session.latest_snapshot();
    let visible_text = snapshot
        .elements
        .iter()
        .flat_map(|element| [element.label.as_str(), element.text.as_str()])
        .filter(|text| !text.trim().is_empty())
        .map(redact_browser_tool_text)
        .collect::<Vec<_>>()
        .join("\n");
    let elements = snapshot
        .elements
        .iter()
        .map(|element| {
            json!({
                "index": element.index,
                "role": element.role,
                "label": redact_browser_tool_text(element.label.as_str()),
                "text": redact_browser_tool_text(element.text.as_str()),
                "rect": element.rect,
                "selector": element.selector,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "snapshotEpoch": snapshot.epoch,
        "url": snapshot.url.as_ref().or(state.url.as_ref()),
        "title": snapshot.title.as_ref().or(state.title.as_ref()),
        "loading": state.loading,
        "activeTabId": state.active_tab_id,
        "tabs": browser_tabs_value(state),
        "viewport": snapshot.viewport,
        "scrollPosition": {
            "x": snapshot.viewport.scroll_x,
            "y": snapshot.viewport.scroll_y,
        },
        "visibleText": visible_text,
        "elements": elements,
    })
}

fn tabs_response(
    params: &DynamicToolCallParams,
    session: &hunk_browser::BrowserSession,
    message: &str,
) -> serde_json::Value {
    let state = session.state();
    json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "activeTabId": state.active_tab_id,
        "tabs": browser_tabs_value(state),
        "message": message,
    })
}

fn browser_tabs_value(state: &hunk_browser::BrowserSessionState) -> serde_json::Value {
    json!(
        state
            .tabs
            .iter()
            .map(|tab| {
                json!({
                    "tabId": tab.tab_id,
                    "url": tab.url,
                    "title": tab.title,
                    "loading": tab.loading,
                    "loadError": tab.load_error,
                    "canGoBack": tab.can_go_back,
                    "canGoForward": tab.can_go_forward,
                    "snapshotEpoch": tab.snapshot_epoch,
                    "latestFrame": tab.latest_frame,
                })
            })
            .collect::<Vec<_>>()
    )
}

fn console_response(
    params: &DynamicToolCallParams,
    session: &hunk_browser::BrowserSession,
    tab_id: &hunk_browser::BrowserTabId,
    level: Option<hunk_browser::BrowserConsoleLevel>,
    since_sequence: Option<u64>,
    limit: usize,
) -> serde_json::Value {
    let entries = session
        .recent_console_entries_for_tab(tab_id, level, since_sequence, limit)
        .into_iter()
        .map(|entry| {
            json!({
                "sequence": entry.sequence,
                "tabId": entry.tab_id,
                "level": entry.level,
                "message": redact_browser_tool_text(entry.message.as_str()),
                "source": entry.source.as_deref().map(redact_browser_tool_text),
                "line": entry.line,
                "timestampMs": entry.timestamp_ms,
            })
        })
        .collect::<Vec<_>>();
    let latest_sequence = session.latest_console_sequence_for_tab(tab_id);

    json!({
        "ok": true,
        "tool": params.tool,
        "threadId": params.thread_id,
        "turnId": params.turn_id,
        "tabId": tab_id,
        "entries": entries,
        "latestSequence": latest_sequence,
        "message": "Console messages were read from the embedded browser.",
    })
}

fn browser_action_summary(action: &BrowserAction) -> String {
    match action {
        BrowserAction::Navigate { url } => format!("Navigate to {url}"),
        BrowserAction::Reload => "Reload the page".to_string(),
        BrowserAction::Stop => "Stop loading the page".to_string(),
        BrowserAction::Back => "Go back".to_string(),
        BrowserAction::Forward => "Go forward".to_string(),
        BrowserAction::Click { index, .. } => format!("Click element {index}"),
        BrowserAction::Type { index, text, .. } => {
            format!(
                "Type {} characters into element {index}",
                text.chars().count()
            )
        }
        BrowserAction::Press { keys } => format!("Press {keys}"),
        BrowserAction::Scroll { down, pages, index } => {
            let direction = if *down { "down" } else { "up" };
            if let Some(index) = index {
                format!("Scroll {direction} {pages} page(s) near element {index}")
            } else {
                format!("Scroll {direction} {pages} page(s)")
            }
        }
        BrowserAction::Screenshot => "Capture a screenshot".to_string(),
    }
}

fn browser_action_label(action: &BrowserAction) -> &'static str {
    match action {
        BrowserAction::Navigate { .. } => "navigate",
        BrowserAction::Reload => "reload",
        BrowserAction::Stop => "stop",
        BrowserAction::Back => "back",
        BrowserAction::Forward => "forward",
        BrowserAction::Click { .. } => "click",
        BrowserAction::Type { .. } => "type",
        BrowserAction::Press { .. } => "press",
        BrowserAction::Scroll { .. } => "scroll",
        BrowserAction::Screenshot => "screenshot",
    }
}

fn browser_action_message(action: &BrowserAction, use_backend: bool) -> &'static str {
    if use_backend {
        return match action {
            BrowserAction::Navigate { .. } => "Navigation was sent to the embedded browser.",
            BrowserAction::Reload => "Reload was sent to the embedded browser.",
            BrowserAction::Stop => "Stop was sent to the embedded browser.",
            BrowserAction::Back => "Back navigation was sent to the embedded browser.",
            BrowserAction::Forward => "Forward navigation was sent to the embedded browser.",
            BrowserAction::Click { .. } => "Click was sent to the embedded browser.",
            BrowserAction::Type { .. } => "Text input was sent to the embedded browser.",
            BrowserAction::Press { .. } => "Key press was sent to the embedded browser.",
            BrowserAction::Scroll { .. } => "Scroll was sent to the embedded browser.",
            BrowserAction::Screenshot => "Screenshot was read from the embedded browser.",
        };
    }

    match action {
        BrowserAction::Navigate { .. } => "Navigation was accepted by the browser state layer.",
        BrowserAction::Reload => "Reload was accepted by the browser state layer.",
        BrowserAction::Stop => "Stop was accepted by the browser state layer.",
        BrowserAction::Back => "Back navigation was accepted by the browser state layer.",
        BrowserAction::Forward => "Forward navigation was accepted by the browser state layer.",
        BrowserAction::Click { .. } => "Click was accepted by the browser state layer.",
        BrowserAction::Type { .. } => "Type was accepted by the browser state layer.",
        BrowserAction::Press { .. } => "Key press was accepted by the browser state layer.",
        BrowserAction::Scroll { .. } => "Scroll was accepted by the browser state layer.",
        BrowserAction::Screenshot => "Screenshot was accepted by the browser state layer.",
    }
}

fn browser_frame_png_data_url(frame: &hunk_browser::BrowserFrame) -> Option<String> {
    let metadata = frame.metadata();
    let mut rgba = frame.bgra().to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    let image = image::RgbaImage::from_raw(metadata.width, metadata.height, rgba)?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            metadata.width,
            metadata.height,
            image::ColorType::Rgba8.into(),
        )
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(png)
    ))
}

fn json_success_response(value: serde_json::Value) -> DynamicToolCallResponse {
    json_response(value, true)
}

fn json_success_response_with_items(
    value: serde_json::Value,
    mut additional_items: Vec<DynamicToolCallOutputContentItem>,
) -> DynamicToolCallResponse {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
        format!("{{\"error\":\"browserResponseSerializationFailed\",\"message\":\"{error}\"}}")
    });
    let mut content_items = vec![DynamicToolCallOutputContentItem::InputText { text }];
    content_items.append(&mut additional_items);
    DynamicToolCallResponse {
        content_items,
        success: true,
    }
}

fn json_error_response(value: serde_json::Value) -> DynamicToolCallResponse {
    json_response(value, false)
}

fn json_response(value: serde_json::Value, success: bool) -> DynamicToolCallResponse {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
        format!("{{\"error\":\"browserResponseSerializationFailed\",\"message\":\"{error}\"}}")
    });
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText { text }],
        success,
    }
}
