use hunk_codex::browser_tools::{
    BROWSER_CLICK_TOOL, BROWSER_DEVELOPER_INSTRUCTIONS, BROWSER_NAVIGATE_TOOL,
    BROWSER_SCREENSHOT_TOOL, BROWSER_SNAPSHOT_TOOL, BROWSER_TYPE_TOOL, browser_dynamic_tool_specs,
    is_browser_dynamic_tool,
};
use hunk_codex::protocol::DynamicToolSpec;

#[test]
fn browser_tool_specs_include_core_controls() {
    let specs = browser_dynamic_tool_specs();
    let names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&BROWSER_NAVIGATE_TOOL));
    assert!(names.contains(&BROWSER_SNAPSHOT_TOOL));
    assert!(names.contains(&BROWSER_CLICK_TOOL));
    assert!(names.contains(&BROWSER_TYPE_TOOL));
    assert!(names.contains(&BROWSER_SCREENSHOT_TOOL));
}

#[test]
fn browser_tool_specs_are_roundtrip_serializable() {
    for spec in browser_dynamic_tool_specs() {
        let value = serde_json::to_value(&spec).expect("spec should serialize");
        let decoded: DynamicToolSpec =
            serde_json::from_value(value).expect("spec should deserialize");

        assert_eq!(decoded, spec);
    }
}

#[test]
fn browser_tool_name_detection_is_exact() {
    assert!(is_browser_dynamic_tool(BROWSER_NAVIGATE_TOOL));
    assert!(!is_browser_dynamic_tool("hunk.browser_navigate.extra"));
    assert!(!is_browser_dynamic_tool("hunk.read_file"));
}

#[test]
fn browser_developer_instructions_describe_snapshot_index_flow() {
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk.browser_snapshot"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("snapshotEpoch"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("element index"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("external browser"));
}
