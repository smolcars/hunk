use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use hunk_codex::android_tools::{AndroidDynamicToolRequest, parse_android_dynamic_tool_request};
use hunk_codex::protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
};
use hunk_mobile::{AndroidAction, AndroidRuntime, redact_mobile_tool_text};
use serde_json::json;

#[derive(Debug, Clone)]
pub(crate) struct AiAndroidDynamicToolExecutor {
    runtime: AndroidRuntime,
}

impl AiAndroidDynamicToolExecutor {
    pub(crate) fn new() -> Self {
        Self {
            runtime: AndroidRuntime::new_auto(),
        }
    }

    pub(crate) fn execute(
        &mut self,
        cwd: &Path,
        params: &DynamicToolCallParams,
    ) -> DynamicToolCallResponse {
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
            params.thread_id.as_str(),
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
                "deviceId": inventory.started_device_id.clone(),
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

fn resolve_workspace_apk_path(cwd: &Path, apk_path: &str) -> Result<PathBuf, String> {
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

fn json_success_response(value: serde_json::Value) -> DynamicToolCallResponse {
    json_response(value, true)
}

fn json_success_response_with_items(
    value: serde_json::Value,
    mut additional_items: Vec<DynamicToolCallOutputContentItem>,
) -> DynamicToolCallResponse {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
        format!("{{\"error\":\"androidResponseSerializationFailed\",\"message\":\"{error}\"}}")
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
        format!("{{\"error\":\"androidResponseSerializationFailed\",\"message\":\"{error}\"}}")
    });
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText { text }],
        success,
    }
}
