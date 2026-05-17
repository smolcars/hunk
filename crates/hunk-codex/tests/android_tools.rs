use hunk_codex::android_tools::{
    ANDROID_DEVELOPER_INSTRUCTIONS, ANDROID_DEVICES_TOOL, ANDROID_LAUNCH_TOOL, ANDROID_PRESS_TOOL,
    ANDROID_SELECT_DEVICE_TOOL, ANDROID_SNAPSHOT_TOOL, ANDROID_TOOL_NAMESPACE, ANDROID_TYPE_TOOL,
    AndroidDynamicToolRequest, android_dynamic_tool_specs, apply_android_thread_start_context,
    is_android_dynamic_tool, is_android_dynamic_tool_call, parse_android_dynamic_tool_request,
};
use hunk_codex::protocol::{DynamicToolCallParams, DynamicToolSpec, ThreadStartParams};
use hunk_mobile::{AndroidAction, AndroidKey};

#[test]
fn android_tool_specs_include_core_controls() {
    let specs = android_dynamic_tool_specs();
    assert!(
        specs
            .iter()
            .all(|spec| spec.namespace.as_deref() == Some(ANDROID_TOOL_NAMESPACE))
    );
    let names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&ANDROID_DEVICES_TOOL));
    assert!(names.contains(&ANDROID_SELECT_DEVICE_TOOL));
    assert!(names.contains(&ANDROID_SNAPSHOT_TOOL));
    assert!(names.contains(&ANDROID_TYPE_TOOL));
    assert!(names.contains(&ANDROID_PRESS_TOOL));
    assert!(names.contains(&ANDROID_LAUNCH_TOOL));
}

#[test]
fn android_tool_specs_are_roundtrip_serializable() {
    for spec in android_dynamic_tool_specs() {
        let value = serde_json::to_value(&spec).expect("spec should serialize");
        let decoded: DynamicToolSpec =
            serde_json::from_value(value).expect("spec should deserialize");

        assert_eq!(decoded, spec);
    }
}

#[test]
fn android_tool_name_detection_is_exact() {
    assert!(is_android_dynamic_tool(ANDROID_DEVICES_TOOL));
    assert!(is_android_dynamic_tool_call(
        Some(ANDROID_TOOL_NAMESPACE),
        ANDROID_DEVICES_TOOL
    ));
    assert!(!is_android_dynamic_tool_call(None, ANDROID_DEVICES_TOOL));
    assert!(!is_android_dynamic_tool("devices.extra"));
    assert!(!is_android_dynamic_tool("hunk.read_file"));
}

#[test]
fn android_developer_instructions_describe_snapshot_index_flow() {
    assert!(ANDROID_DEVELOPER_INSTRUCTIONS.contains("Android Emulator"));
    assert!(ANDROID_DEVELOPER_INSTRUCTIONS.contains("hunk_android namespace"));
    assert!(ANDROID_DEVELOPER_INSTRUCTIONS.contains("Do not use Appium"));
    assert!(ANDROID_DEVELOPER_INSTRUCTIONS.contains("hunk_android.devices"));
    assert!(ANDROID_DEVELOPER_INSTRUCTIONS.contains("hunk_android.snapshot"));
    assert!(ANDROID_DEVELOPER_INSTRUCTIONS.contains("snapshotEpoch"));
    assert!(ANDROID_DEVELOPER_INSTRUCTIONS.contains("element index"));
}

#[test]
fn apply_android_thread_start_context_adds_tools_and_instructions() {
    let mut params = ThreadStartParams {
        developer_instructions: Some("Existing instructions.".to_string()),
        ..ThreadStartParams::default()
    };

    apply_android_thread_start_context(&mut params);

    let instructions = params
        .developer_instructions
        .as_deref()
        .expect("developer instructions should be set");
    assert!(instructions.contains("Existing instructions."));
    assert!(instructions.contains(ANDROID_DEVELOPER_INSTRUCTIONS));

    let tool_names = params
        .dynamic_tools
        .as_ref()
        .expect("dynamic tools should be set")
        .iter()
        .map(|spec| (spec.namespace.as_deref(), spec.name.as_str()))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&(Some(ANDROID_TOOL_NAMESPACE), ANDROID_DEVICES_TOOL)));
    assert!(tool_names.contains(&(Some(ANDROID_TOOL_NAMESPACE), ANDROID_SNAPSHOT_TOOL)));
}

#[test]
fn apply_android_thread_start_context_is_idempotent() {
    let mut params = ThreadStartParams::default();

    apply_android_thread_start_context(&mut params);
    apply_android_thread_start_context(&mut params);

    let instructions = params
        .developer_instructions
        .as_deref()
        .expect("developer instructions should be set");
    assert_eq!(
        instructions.matches(ANDROID_DEVELOPER_INSTRUCTIONS).count(),
        1
    );

    let devices_count = params
        .dynamic_tools
        .as_ref()
        .expect("dynamic tools should be set")
        .iter()
        .filter(|spec| {
            spec.namespace.as_deref() == Some(ANDROID_TOOL_NAMESPACE)
                && spec.name == ANDROID_DEVICES_TOOL
        })
        .count();
    assert_eq!(devices_count, 1);
}

#[test]
fn parse_android_type_request_requires_epoch_when_indexed() {
    let request = parse_android_dynamic_tool_request(&dynamic_tool_params(
        ANDROID_TYPE_TOOL,
        serde_json::json!({
            "snapshotEpoch": 7,
            "index": 42,
            "text": "hello"
        }),
    ))
    .expect("Android type args should parse");

    assert_eq!(
        request,
        AndroidDynamicToolRequest::Action {
            device_id: None,
            action: AndroidAction::Type {
                snapshot_epoch: Some(7),
                index: Some(42),
                text: "hello".to_string(),
                clear: false,
            },
        }
    );

    let error = parse_android_dynamic_tool_request(&dynamic_tool_params(
        ANDROID_TYPE_TOOL,
        serde_json::json!({
            "index": 42,
            "text": "hello"
        }),
    ))
    .expect_err("snapshot epoch should be required with index");
    assert!(error.contains("snapshotEpoch is required"));
}

#[test]
fn parse_android_press_maps_key_names() {
    let request = parse_android_dynamic_tool_request(&dynamic_tool_params(
        ANDROID_PRESS_TOOL,
        serde_json::json!({
            "key": "Back"
        }),
    ))
    .expect("Android press args should parse");

    assert_eq!(
        request,
        AndroidDynamicToolRequest::Action {
            device_id: None,
            action: AndroidAction::Press {
                key: AndroidKey::Back,
            },
        }
    );
}

fn dynamic_tool_params(tool: &str, arguments: serde_json::Value) -> DynamicToolCallParams {
    DynamicToolCallParams {
        namespace: Some(ANDROID_TOOL_NAMESPACE.to_string()),
        tool: tool.to_string(),
        arguments,
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
    }
}
