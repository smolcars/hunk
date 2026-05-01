use crate::protocol::DynamicToolCallParams;
use crate::protocol::DynamicToolSpec;
use crate::protocol::ThreadStartParams;
use hunk_mobile::{AndroidAction, AndroidKey, AndroidTapTarget, MobileDeviceId};
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

pub const ANDROID_TOOL_NAMESPACE: &str = "hunk_android";
pub const ANDROID_DEVICES_TOOL: &str = "devices";
pub const ANDROID_START_TOOL: &str = "start";
pub const ANDROID_SELECT_DEVICE_TOOL: &str = "select_device";
pub const ANDROID_INSTALL_APK_TOOL: &str = "install_apk";
pub const ANDROID_LAUNCH_TOOL: &str = "launch";
pub const ANDROID_SNAPSHOT_TOOL: &str = "snapshot";
pub const ANDROID_SCREENSHOT_TOOL: &str = "screenshot";
pub const ANDROID_TAP_TOOL: &str = "tap";
pub const ANDROID_TYPE_TOOL: &str = "type";
pub const ANDROID_PRESS_TOOL: &str = "press";
pub const ANDROID_SWIPE_TOOL: &str = "swipe";
pub const ANDROID_LOGS_TOOL: &str = "logs";

pub const ANDROID_DEVELOPER_INSTRUCTIONS: &str = r#"When the user asks to inspect, test, or control the Android Emulator, use Hunk's Android emulator dynamic tools.
Use the tools in the hunk_android namespace directly. Do not use Appium, Maestro, Detox, raw adb shell commands, or external scripts for Android emulator control unless the user explicitly asks for that fallback.
Use hunk_android.devices before assuming an emulator is available.
Use hunk_android.snapshot before tapping, typing, or swiping by UI element. The snapshot returns a snapshotEpoch and indexed visible elements; pass that same snapshotEpoch and element index to hunk_android.tap or hunk_android.type.
Use hunk_android.screenshot when visual verification is needed.
Use hunk_android.logs when the user asks for Android logs, Logcat, crashes, or runtime debugging output.
If an Android action reports that confirmation is required, stop and wait for the user decision before continuing."#;

pub fn android_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        spec(
            ANDROID_DEVICES_TOOL,
            "List Android SDK tool availability, running emulators, and available AVDs.",
            object_schema(json!({}), &[]),
        ),
        spec(
            ANDROID_START_TOOL,
            "Start an Android Virtual Device by AVD name.",
            object_schema(
                json!({
                    "avdName": {
                        "type": "string",
                        "description": "AVD name returned by hunk_android.devices."
                    },
                    "waitForBoot": {
                        "type": "boolean",
                        "description": "Whether to wait for the emulator to finish booting. Defaults to true."
                    },
                    "timeoutSeconds": {
                        "type": "integer",
                        "description": "Optional boot wait timeout in seconds. Defaults to 90."
                    }
                }),
                &["avdName"],
            ),
        ),
        spec(
            ANDROID_SELECT_DEVICE_TOOL,
            "Select a running Android emulator for the active AI thread.",
            object_schema(
                json!({
                    "deviceId": {
                        "type": "string",
                        "description": "Device serial returned by hunk_android.devices, for example emulator-5554."
                    }
                }),
                &["deviceId"],
            ),
        ),
        spec(
            ANDROID_INSTALL_APK_TOOL,
            "Install an APK from the current workspace onto the active Android emulator.",
            object_schema(
                optional_device_properties(json!({
                    "apkPath": {
                        "type": "string",
                        "description": "Workspace-relative APK path, or an absolute path inside the workspace."
                    }
                })),
                &["apkPath"],
            ),
        ),
        spec(
            ANDROID_LAUNCH_TOOL,
            "Launch an installed Android app on the active emulator.",
            object_schema(
                optional_device_properties(json!({
                    "package": {
                        "type": "string",
                        "description": "Android package name to launch."
                    },
                    "activity": {
                        "type": "string",
                        "description": "Optional activity class. If omitted, monkey launches the package."
                    }
                })),
                &["package"],
            ),
        ),
        spec(
            ANDROID_SNAPSHOT_TOOL,
            "Read the Android emulator screen state and visible UI element index map.",
            object_schema(optional_device_properties(json!({})), &[]),
        ),
        spec(
            ANDROID_SCREENSHOT_TOOL,
            "Capture a PNG screenshot of the active Android emulator.",
            object_schema(optional_device_properties(json!({})), &[]),
        ),
        spec(
            ANDROID_TAP_TOOL,
            "Tap an indexed Android UI element from the latest snapshot or a raw screen coordinate.",
            object_schema(
                optional_device_properties(json!({
                    "snapshotEpoch": {
                        "type": "integer",
                        "description": "Epoch from the latest Android snapshot. Required with index."
                    },
                    "index": {
                        "type": "integer",
                        "description": "Element index from the latest Android snapshot."
                    },
                    "x": {
                        "type": "integer",
                        "description": "Raw x coordinate in device pixels. Required with y when index is omitted."
                    },
                    "y": {
                        "type": "integer",
                        "description": "Raw y coordinate in device pixels. Required with x when index is omitted."
                    }
                })),
                &[],
            ),
        ),
        spec(
            ANDROID_TYPE_TOOL,
            "Type simple text into an Android UI element or the currently focused field.",
            object_schema(
                optional_device_properties(json!({
                    "snapshotEpoch": {
                        "type": "integer",
                        "description": "Epoch from the latest Android snapshot. Required when index is provided."
                    },
                    "index": {
                        "type": "integer",
                        "description": "Optional element index from the latest Android snapshot to tap before typing."
                    },
                    "text": {
                        "type": "string",
                        "description": "Simple text to type. Newlines and complex Unicode may be rejected in v1."
                    },
                    "clear": {
                        "type": "boolean",
                        "description": "Reserved for future reliable clear-first behavior. Defaults to false in v1."
                    }
                })),
                &["text"],
            ),
        ),
        spec(
            ANDROID_PRESS_TOOL,
            "Press an Android key such as Back, Home, Enter, Tab, Delete, RecentApps, or Power.",
            object_schema(
                optional_device_properties(json!({
                    "key": {
                        "type": "string",
                        "description": "Android key name such as Back, Home, Enter, Tab, Escape, Delete, RecentApps, Power, VolumeUp, or KEYCODE_BACK."
                    }
                })),
                &["key"],
            ),
        ),
        spec(
            ANDROID_SWIPE_TOOL,
            "Swipe between two raw Android screen coordinates.",
            object_schema(
                optional_device_properties(json!({
                    "startX": { "type": "integer" },
                    "startY": { "type": "integer" },
                    "endX": { "type": "integer" },
                    "endY": { "type": "integer" },
                    "durationMs": {
                        "type": "integer",
                        "description": "Swipe duration in milliseconds. Defaults to 300."
                    }
                })),
                &["startX", "startY", "endX", "endY"],
            ),
        ),
        spec(
            ANDROID_LOGS_TOOL,
            "Read recent Logcat output from the active Android emulator.",
            object_schema(
                optional_device_properties(json!({
                    "maxLines": {
                        "type": "integer",
                        "description": "Maximum log lines to return. Defaults to 200 and is capped at 2000."
                    }
                })),
                &[],
            ),
        ),
    ]
}

pub fn is_android_dynamic_tool(tool: &str) -> bool {
    matches!(
        tool,
        ANDROID_DEVICES_TOOL
            | ANDROID_START_TOOL
            | ANDROID_SELECT_DEVICE_TOOL
            | ANDROID_INSTALL_APK_TOOL
            | ANDROID_LAUNCH_TOOL
            | ANDROID_SNAPSHOT_TOOL
            | ANDROID_SCREENSHOT_TOOL
            | ANDROID_TAP_TOOL
            | ANDROID_TYPE_TOOL
            | ANDROID_PRESS_TOOL
            | ANDROID_SWIPE_TOOL
            | ANDROID_LOGS_TOOL
    )
}

pub fn is_android_dynamic_tool_call(namespace: Option<&str>, tool: &str) -> bool {
    namespace == Some(ANDROID_TOOL_NAMESPACE) && is_android_dynamic_tool(tool)
}

pub fn apply_android_thread_start_context(params: &mut ThreadStartParams) {
    append_android_developer_instructions(&mut params.developer_instructions);

    let mut dynamic_tools = params.dynamic_tools.take().unwrap_or_default();
    for spec in android_dynamic_tool_specs() {
        if !dynamic_tools
            .iter()
            .any(|existing| existing.name == spec.name && existing.namespace == spec.namespace)
        {
            dynamic_tools.push(spec);
        }
    }
    params.dynamic_tools = Some(dynamic_tools);
}

#[derive(Debug, Clone, PartialEq)]
pub enum AndroidDynamicToolRequest {
    Devices,
    Start {
        avd_name: String,
        wait_for_boot: bool,
        timeout_seconds: Option<u64>,
    },
    SelectDevice {
        device_id: MobileDeviceId,
    },
    InstallApk {
        device_id: Option<MobileDeviceId>,
        apk_path: String,
    },
    Launch {
        device_id: Option<MobileDeviceId>,
        package: String,
        activity: Option<String>,
    },
    Snapshot {
        device_id: Option<MobileDeviceId>,
    },
    Screenshot {
        device_id: Option<MobileDeviceId>,
    },
    Logs {
        device_id: Option<MobileDeviceId>,
        max_lines: usize,
    },
    Action {
        device_id: Option<MobileDeviceId>,
        action: AndroidAction,
    },
}

pub fn parse_android_dynamic_tool_request(
    params: &DynamicToolCallParams,
) -> Result<AndroidDynamicToolRequest, String> {
    if !is_android_dynamic_tool_call(params.namespace.as_deref(), params.tool.as_str()) {
        return Err(format!(
            "unsupported Android dynamic tool '{}{}'",
            params
                .namespace
                .as_deref()
                .map(|namespace| format!("{namespace}."))
                .unwrap_or_default(),
            params.tool
        ));
    }

    match params.tool.as_str() {
        ANDROID_DEVICES_TOOL => Ok(AndroidDynamicToolRequest::Devices),
        ANDROID_START_TOOL => {
            let args = parse_args::<StartArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::Start {
                avd_name: args.avd_name,
                wait_for_boot: args.wait_for_boot.unwrap_or(true),
                timeout_seconds: args.timeout_seconds,
            })
        }
        ANDROID_SELECT_DEVICE_TOOL => {
            let args = parse_args::<DeviceIdArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::SelectDevice {
                device_id: MobileDeviceId::new(args.device_id),
            })
        }
        ANDROID_INSTALL_APK_TOOL => {
            let args = parse_args::<InstallApkArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::InstallApk {
                device_id: optional_device_id(args.device_id),
                apk_path: args.apk_path,
            })
        }
        ANDROID_LAUNCH_TOOL => {
            let args = parse_args::<LaunchArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::Launch {
                device_id: optional_device_id(args.device_id),
                package: args.package,
                activity: args.activity,
            })
        }
        ANDROID_SNAPSHOT_TOOL => {
            let args = parse_args::<OptionalDeviceArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::Snapshot {
                device_id: optional_device_id(args.device_id),
            })
        }
        ANDROID_SCREENSHOT_TOOL => {
            let args = parse_args::<OptionalDeviceArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::Screenshot {
                device_id: optional_device_id(args.device_id),
            })
        }
        ANDROID_TAP_TOOL => {
            let args = parse_args::<TapArgs>(&params.arguments)?;
            let target = match (args.snapshot_epoch, args.index, args.x, args.y) {
                (Some(snapshot_epoch), Some(index), _, _) => AndroidTapTarget::Element {
                    snapshot_epoch,
                    index,
                },
                (_, _, Some(x), Some(y)) => AndroidTapTarget::Point { x, y },
                _ => {
                    return Err(
                        "invalid Android tap arguments: provide snapshotEpoch and index, or x and y"
                            .to_string(),
                    );
                }
            };
            Ok(AndroidDynamicToolRequest::Action {
                device_id: optional_device_id(args.device_id),
                action: AndroidAction::Tap { target },
            })
        }
        ANDROID_TYPE_TOOL => {
            let args = parse_args::<TypeArgs>(&params.arguments)?;
            if args.index.is_some() && args.snapshot_epoch.is_none() {
                return Err(
                    "invalid Android type arguments: snapshotEpoch is required when index is provided"
                        .to_string(),
                );
            }
            Ok(AndroidDynamicToolRequest::Action {
                device_id: optional_device_id(args.device_id),
                action: AndroidAction::Type {
                    snapshot_epoch: args.snapshot_epoch,
                    index: args.index,
                    text: args.text,
                    clear: args.clear.unwrap_or(false),
                },
            })
        }
        ANDROID_PRESS_TOOL => {
            let args = parse_args::<PressArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::Action {
                device_id: optional_device_id(args.device_id),
                action: AndroidAction::Press {
                    key: parse_android_key(args.key),
                },
            })
        }
        ANDROID_SWIPE_TOOL => {
            let args = parse_args::<SwipeArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::Action {
                device_id: optional_device_id(args.device_id),
                action: AndroidAction::Swipe {
                    start_x: args.start_x,
                    start_y: args.start_y,
                    end_x: args.end_x,
                    end_y: args.end_y,
                    duration_ms: args.duration_ms.unwrap_or(300).clamp(1, 30_000),
                },
            })
        }
        ANDROID_LOGS_TOOL => {
            let args = parse_args::<LogsArgs>(&params.arguments)?;
            Ok(AndroidDynamicToolRequest::Logs {
                device_id: optional_device_id(args.device_id),
                max_lines: args.max_lines.unwrap_or(200).clamp(1, 2_000),
            })
        }
        _ => Err(format!(
            "unsupported Android dynamic tool '{}'",
            params.tool
        )),
    }
}

fn append_android_developer_instructions(instructions: &mut Option<String>) {
    match instructions {
        Some(existing) if existing.contains(ANDROID_DEVELOPER_INSTRUCTIONS) => {}
        Some(existing) if existing.trim().is_empty() => {
            *existing = ANDROID_DEVELOPER_INSTRUCTIONS.to_string();
        }
        Some(existing) => {
            existing.push_str("\n\n");
            existing.push_str(ANDROID_DEVELOPER_INSTRUCTIONS);
        }
        None => {
            *instructions = Some(ANDROID_DEVELOPER_INSTRUCTIONS.to_string());
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartArgs {
    avd_name: String,
    #[serde(default)]
    wait_for_boot: Option<bool>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionalDeviceArgs {
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceIdArgs {
    device_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallApkArgs {
    apk_path: String,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchArgs {
    package: String,
    #[serde(default)]
    activity: Option<String>,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TapArgs {
    #[serde(default)]
    snapshot_epoch: Option<u64>,
    #[serde(default)]
    index: Option<u32>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeArgs {
    #[serde(default)]
    snapshot_epoch: Option<u64>,
    #[serde(default)]
    index: Option<u32>,
    text: String,
    #[serde(default)]
    clear: Option<bool>,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PressArgs {
    key: String,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwipeArgs {
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogsArgs {
    #[serde(default)]
    max_lines: Option<usize>,
    #[serde(default)]
    device_id: Option<String>,
}

fn optional_device_id(device_id: Option<String>) -> Option<MobileDeviceId> {
    device_id
        .map(|device_id| device_id.trim().to_string())
        .filter(|device_id| !device_id.is_empty())
        .map(MobileDeviceId::new)
}

fn parse_android_key(key: String) -> AndroidKey {
    match key.trim().to_ascii_lowercase().as_str() {
        "back" => AndroidKey::Back,
        "home" => AndroidKey::Home,
        "enter" | "return" => AndroidKey::Enter,
        "tab" => AndroidKey::Tab,
        "escape" | "esc" => AndroidKey::Escape,
        "delete" | "backspace" => AndroidKey::Delete,
        "recentapps" | "recents" | "appswitch" => AndroidKey::RecentApps,
        "power" => AndroidKey::Power,
        "volumeup" => AndroidKey::VolumeUp,
        "volumedown" => AndroidKey::VolumeDown,
        _ => AndroidKey::Raw(key),
    }
}

fn parse_args<T>(arguments: &Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|error| format!("invalid Android dynamic tool arguments: {error}"))
}

fn spec(name: &str, description: &str, input_schema: Value) -> DynamicToolSpec {
    DynamicToolSpec {
        namespace: Some(ANDROID_TOOL_NAMESPACE.to_string()),
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        defer_loading: false,
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn optional_device_properties(mut properties: Value) -> Value {
    if let Some(properties) = properties.as_object_mut() {
        properties.insert(
            "deviceId".to_string(),
            json!({
                "type": "string",
                "description": "Optional Android device serial returned by hunk_android.devices."
            }),
        );
    }
    properties
}
