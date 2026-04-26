use hunk_browser::{BrowserAction, BrowserConsoleLevel, BrowserTabId};
use hunk_codex::browser_tools::{
    BROWSER_BACK_TOOL, BROWSER_CLICK_TOOL, BROWSER_CLOSE_TAB_TOOL, BROWSER_CONSOLE_TOOL,
    BROWSER_DEVELOPER_INSTRUCTIONS, BROWSER_FORWARD_TOOL, BROWSER_NAVIGATE_TOOL,
    BROWSER_NEW_TAB_TOOL, BROWSER_RELOAD_TOOL, BROWSER_SCREENSHOT_TOOL, BROWSER_SCROLL_TOOL,
    BROWSER_SELECT_TAB_TOOL, BROWSER_SNAPSHOT_TOOL, BROWSER_STOP_TOOL, BROWSER_TABS_TOOL,
    BROWSER_TOOL_NAMESPACE, BROWSER_TYPE_TOOL, BrowserDynamicToolRequest,
    apply_browser_thread_start_context, browser_dynamic_tool_specs, is_browser_dynamic_tool,
    is_browser_dynamic_tool_call, parse_browser_dynamic_tool_request,
};
use hunk_codex::protocol::{DynamicToolCallParams, DynamicToolSpec, ThreadStartParams};

#[test]
fn browser_tool_specs_include_core_controls() {
    let specs = browser_dynamic_tool_specs();
    assert!(
        specs
            .iter()
            .all(|spec| spec.namespace.as_deref() == Some(BROWSER_TOOL_NAMESPACE))
    );
    let names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&BROWSER_NAVIGATE_TOOL));
    assert!(names.contains(&BROWSER_RELOAD_TOOL));
    assert!(names.contains(&BROWSER_STOP_TOOL));
    assert!(names.contains(&BROWSER_BACK_TOOL));
    assert!(names.contains(&BROWSER_FORWARD_TOOL));
    assert!(names.contains(&BROWSER_SNAPSHOT_TOOL));
    assert!(names.contains(&BROWSER_CLICK_TOOL));
    assert!(names.contains(&BROWSER_TYPE_TOOL));
    assert!(names.contains(&BROWSER_SCREENSHOT_TOOL));
    assert!(names.contains(&BROWSER_CONSOLE_TOOL));
    assert!(names.contains(&BROWSER_TABS_TOOL));
    assert!(names.contains(&BROWSER_NEW_TAB_TOOL));
    assert!(names.contains(&BROWSER_SELECT_TAB_TOOL));
    assert!(names.contains(&BROWSER_CLOSE_TAB_TOOL));
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
    assert!(is_browser_dynamic_tool_call(
        Some(BROWSER_TOOL_NAMESPACE),
        BROWSER_NAVIGATE_TOOL
    ));
    assert!(!is_browser_dynamic_tool_call(None, BROWSER_NAVIGATE_TOOL));
    assert!(!is_browser_dynamic_tool("navigate.extra"));
    assert!(!is_browser_dynamic_tool("hunk.read_file"));
}

#[test]
fn browser_developer_instructions_describe_snapshot_index_flow() {
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("Hunk's embedded browser dynamic tools"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk_browser namespace"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("Browser Use/browser-use"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("Do not use"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk_browser.navigate"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk_browser.snapshot"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk_browser.console"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk_browser.tabs"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk_browser.select_tab"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("tabId"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("snapshotEpoch"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("element index"));
    assert!(BROWSER_DEVELOPER_INSTRUCTIONS.contains("hunk_browser.back"));
}

#[test]
fn apply_browser_thread_start_context_adds_tools_and_instructions() {
    let mut params = ThreadStartParams {
        developer_instructions: Some("Existing instructions.".to_string()),
        ..ThreadStartParams::default()
    };

    apply_browser_thread_start_context(&mut params);

    let instructions = params
        .developer_instructions
        .as_deref()
        .expect("developer instructions should be set");
    assert!(instructions.contains("Existing instructions."));
    assert!(instructions.contains(BROWSER_DEVELOPER_INSTRUCTIONS));

    let tool_names = params
        .dynamic_tools
        .as_ref()
        .expect("dynamic tools should be set")
        .iter()
        .map(|spec| (spec.namespace.as_deref(), spec.name.as_str()))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&(Some(BROWSER_TOOL_NAMESPACE), BROWSER_NAVIGATE_TOOL)));
    assert!(tool_names.contains(&(Some(BROWSER_TOOL_NAMESPACE), BROWSER_RELOAD_TOOL)));
    assert!(tool_names.contains(&(Some(BROWSER_TOOL_NAMESPACE), BROWSER_SNAPSHOT_TOOL)));
    assert!(tool_names.contains(&(Some(BROWSER_TOOL_NAMESPACE), BROWSER_CONSOLE_TOOL)));
}

#[test]
fn apply_browser_thread_start_context_is_idempotent() {
    let mut params = ThreadStartParams::default();

    apply_browser_thread_start_context(&mut params);
    apply_browser_thread_start_context(&mut params);

    let instructions = params
        .developer_instructions
        .as_deref()
        .expect("developer instructions should be set");
    assert_eq!(
        instructions.matches(BROWSER_DEVELOPER_INSTRUCTIONS).count(),
        1
    );

    let navigate_count = params
        .dynamic_tools
        .as_ref()
        .expect("dynamic tools should be set")
        .iter()
        .filter(|spec| {
            spec.namespace.as_deref() == Some(BROWSER_TOOL_NAMESPACE)
                && spec.name == BROWSER_NAVIGATE_TOOL
        })
        .count();
    assert_eq!(navigate_count, 1);
}

#[test]
fn browser_thread_start_params_are_serializable() {
    let mut params = ThreadStartParams {
        persist_extended_history: true,
        ..ThreadStartParams::default()
    };
    apply_browser_thread_start_context(&mut params);

    let value = serde_json::to_value(&params).expect("thread params should serialize");
    let tools = value
        .get("dynamicTools")
        .and_then(serde_json::Value::as_array)
        .expect("dynamic tools should serialize");
    assert!(tools.iter().any(|tool| {
        tool.get("namespace").and_then(serde_json::Value::as_str) == Some(BROWSER_TOOL_NAMESPACE)
            && tool.get("name").and_then(serde_json::Value::as_str) == Some(BROWSER_NAVIGATE_TOOL)
    }));
}

#[test]
fn parse_browser_click_request_uses_snapshot_epoch_and_index() {
    let request = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_CLICK_TOOL,
        serde_json::json!({
            "snapshotEpoch": 7,
            "index": 42
        }),
    ))
    .expect("browser click args should parse");

    assert_eq!(
        request,
        BrowserDynamicToolRequest::Action {
            tab_id: None,
            action: BrowserAction::Click {
                snapshot_epoch: 7,
                index: 42,
            },
        }
    );
}

#[test]
fn parse_browser_type_request_defaults_to_clear_first() {
    let request = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_TYPE_TOOL,
        serde_json::json!({
            "snapshotEpoch": 7,
            "index": 42,
            "text": "hello"
        }),
    ))
    .expect("browser type args should parse");

    assert_eq!(
        request,
        BrowserDynamicToolRequest::Action {
            tab_id: None,
            action: BrowserAction::Type {
                snapshot_epoch: 7,
                index: 42,
                text: "hello".to_string(),
                clear: true,
            },
        }
    );
}

#[test]
fn parse_browser_scroll_request_applies_defaults() {
    let request = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_SCROLL_TOOL,
        serde_json::json!({}),
    ))
    .expect("browser scroll args should parse");

    assert_eq!(
        request,
        BrowserDynamicToolRequest::Action {
            tab_id: None,
            action: BrowserAction::Scroll {
                down: true,
                pages: 1.0,
                index: None,
            },
        }
    );
}

#[test]
fn parse_browser_navigation_control_requests() {
    for (tool, expected) in [
        (BROWSER_RELOAD_TOOL, BrowserAction::Reload),
        (BROWSER_STOP_TOOL, BrowserAction::Stop),
        (BROWSER_BACK_TOOL, BrowserAction::Back),
        (BROWSER_FORWARD_TOOL, BrowserAction::Forward),
    ] {
        let request =
            parse_browser_dynamic_tool_request(&dynamic_tool_params(tool, serde_json::json!({})))
                .expect("browser navigation control args should parse");

        assert_eq!(
            request,
            BrowserDynamicToolRequest::Action {
                tab_id: None,
                action: expected,
            }
        );
    }
}

#[test]
fn parse_browser_requests_accept_optional_tab_id() {
    let request = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_NAVIGATE_TOOL,
        serde_json::json!({
            "url": "https://example.com",
            "tabId": "tab-2",
        }),
    ))
    .expect("browser navigate with tab id should parse");
    assert_eq!(
        request,
        BrowserDynamicToolRequest::Action {
            tab_id: Some(BrowserTabId::new("tab-2")),
            action: BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
        }
    );

    let request = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_SNAPSHOT_TOOL,
        serde_json::json!({
            "tabId": "tab-2",
        }),
    ))
    .expect("browser snapshot with tab id should parse");
    assert_eq!(
        request,
        BrowserDynamicToolRequest::Snapshot {
            tab_id: Some(BrowserTabId::new("tab-2")),
        }
    );
}

#[test]
fn parse_browser_console_request_applies_defaults_and_filters() {
    let request = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_CONSOLE_TOOL,
        serde_json::json!({}),
    ))
    .expect("browser console args should parse");

    assert_eq!(
        request,
        BrowserDynamicToolRequest::Console {
            tab_id: None,
            level: None,
            since_sequence: None,
            limit: 100,
        }
    );

    let request = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_CONSOLE_TOOL,
        serde_json::json!({
            "level": "warning",
            "sinceSequence": 7,
            "limit": 600
        }),
    ))
    .expect("browser console filters should parse");

    assert_eq!(
        request,
        BrowserDynamicToolRequest::Console {
            tab_id: None,
            level: Some(BrowserConsoleLevel::Warning),
            since_sequence: Some(7),
            limit: 500,
        }
    );
}

#[test]
fn parse_browser_tab_requests() {
    assert_eq!(
        parse_browser_dynamic_tool_request(&dynamic_tool_params(
            BROWSER_TABS_TOOL,
            serde_json::json!({})
        ))
        .expect("tabs should parse"),
        BrowserDynamicToolRequest::Tabs
    );

    assert_eq!(
        parse_browser_dynamic_tool_request(&dynamic_tool_params(
            BROWSER_NEW_TAB_TOOL,
            serde_json::json!({
                "url": "https://example.com",
            })
        ))
        .expect("new tab should parse"),
        BrowserDynamicToolRequest::NewTab {
            url: Some("https://example.com".to_string()),
            activate: true,
        }
    );

    assert_eq!(
        parse_browser_dynamic_tool_request(&dynamic_tool_params(
            BROWSER_SELECT_TAB_TOOL,
            serde_json::json!({
                "tabId": "tab-2",
            })
        ))
        .expect("select tab should parse"),
        BrowserDynamicToolRequest::SelectTab {
            tab_id: BrowserTabId::new("tab-2"),
        }
    );

    assert_eq!(
        parse_browser_dynamic_tool_request(&dynamic_tool_params(
            BROWSER_CLOSE_TAB_TOOL,
            serde_json::json!({
                "tabId": "tab-2",
            })
        ))
        .expect("close tab should parse"),
        BrowserDynamicToolRequest::CloseTab {
            tab_id: BrowserTabId::new("tab-2"),
        }
    );
}

#[test]
fn parse_browser_request_returns_argument_errors() {
    let error = parse_browser_dynamic_tool_request(&dynamic_tool_params(
        BROWSER_CLICK_TOOL,
        serde_json::json!({
            "index": 42
        }),
    ))
    .expect_err("missing snapshot epoch should fail");

    assert!(error.contains("invalid browser dynamic tool arguments"));
}

fn dynamic_tool_params(tool: &str, arguments: serde_json::Value) -> DynamicToolCallParams {
    DynamicToolCallParams {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
        namespace: Some(BROWSER_TOOL_NAMESPACE.to_string()),
        tool: tool.to_string(),
        arguments,
    }
}
