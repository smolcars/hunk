use hunk_codex::protocol::{DynamicToolCallParams, DynamicToolSpec, ThreadStartParams};
use hunk_codex::terminal_tools::{
    TERMINAL_CLOSE_TAB_TOOL, TERMINAL_DEVELOPER_INSTRUCTIONS, TERMINAL_KILL_TOOL,
    TERMINAL_LOGS_TOOL, TERMINAL_NEW_TAB_TOOL, TERMINAL_OPEN_TOOL, TERMINAL_PASTE_TOOL,
    TERMINAL_PRESS_TOOL, TERMINAL_RESIZE_TOOL, TERMINAL_RUN_TOOL, TERMINAL_SCROLL_TOOL,
    TERMINAL_SELECT_TAB_TOOL, TERMINAL_SNAPSHOT_TOOL, TERMINAL_TABS_TOOL, TERMINAL_TOOL_NAMESPACE,
    TERMINAL_TYPE_TOOL, TerminalDynamicToolRequest, TerminalTabId,
    apply_terminal_thread_start_context, is_terminal_dynamic_tool, is_terminal_dynamic_tool_call,
    parse_terminal_dynamic_tool_request, terminal_dynamic_tool_specs,
};

#[test]
fn terminal_tool_specs_include_core_controls() {
    let specs = terminal_dynamic_tool_specs();
    assert!(
        specs
            .iter()
            .all(|spec| spec.namespace.as_deref() == Some(TERMINAL_TOOL_NAMESPACE))
    );
    let names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&TERMINAL_OPEN_TOOL));
    assert!(names.contains(&TERMINAL_TABS_TOOL));
    assert!(names.contains(&TERMINAL_NEW_TAB_TOOL));
    assert!(names.contains(&TERMINAL_SELECT_TAB_TOOL));
    assert!(names.contains(&TERMINAL_CLOSE_TAB_TOOL));
    assert!(names.contains(&TERMINAL_SNAPSHOT_TOOL));
    assert!(names.contains(&TERMINAL_LOGS_TOOL));
    assert!(names.contains(&TERMINAL_RUN_TOOL));
    assert!(names.contains(&TERMINAL_TYPE_TOOL));
    assert!(names.contains(&TERMINAL_PASTE_TOOL));
    assert!(names.contains(&TERMINAL_PRESS_TOOL));
    assert!(names.contains(&TERMINAL_SCROLL_TOOL));
    assert!(names.contains(&TERMINAL_RESIZE_TOOL));
    assert!(names.contains(&TERMINAL_KILL_TOOL));
}

#[test]
fn terminal_tool_specs_are_roundtrip_serializable() {
    for spec in terminal_dynamic_tool_specs() {
        let value = serde_json::to_value(&spec).expect("spec should serialize");
        let decoded: DynamicToolSpec =
            serde_json::from_value(value).expect("spec should deserialize");

        assert_eq!(decoded, spec);
    }
}

#[test]
fn terminal_tool_name_detection_is_exact() {
    assert!(is_terminal_dynamic_tool(TERMINAL_OPEN_TOOL));
    assert!(is_terminal_dynamic_tool_call(
        Some(TERMINAL_TOOL_NAMESPACE),
        TERMINAL_OPEN_TOOL
    ));
    assert!(!is_terminal_dynamic_tool_call(None, TERMINAL_OPEN_TOOL));
    assert!(!is_terminal_dynamic_tool("open.extra"));
    assert!(!is_terminal_dynamic_tool("hunk.read_file"));
}

#[test]
fn terminal_developer_instructions_describe_terminal_flow() {
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("hunk_terminal dynamic tools"));
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("hunk_terminal.open"));
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("hunk_terminal.snapshot"));
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("hunk_terminal.logs"));
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("hunk_terminal.tabs"));
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("hunk_terminal.run"));
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("hunk_browser"));
    assert!(TERMINAL_DEVELOPER_INSTRUCTIONS.contains("tabId"));
}

#[test]
fn apply_terminal_thread_start_context_adds_tools_and_instructions() {
    let mut params = ThreadStartParams {
        developer_instructions: Some("Existing instructions.".to_string()),
        ..ThreadStartParams::default()
    };

    apply_terminal_thread_start_context(&mut params);

    let instructions = params
        .developer_instructions
        .as_deref()
        .expect("developer instructions should be set");
    assert!(instructions.contains("Existing instructions."));
    assert!(instructions.contains(TERMINAL_DEVELOPER_INSTRUCTIONS));

    let tool_names = params
        .dynamic_tools
        .as_ref()
        .expect("dynamic tools should be set")
        .iter()
        .map(|spec| (spec.namespace.as_deref(), spec.name.as_str()))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&(Some(TERMINAL_TOOL_NAMESPACE), TERMINAL_OPEN_TOOL)));
    assert!(tool_names.contains(&(Some(TERMINAL_TOOL_NAMESPACE), TERMINAL_SNAPSHOT_TOOL)));
    assert!(tool_names.contains(&(Some(TERMINAL_TOOL_NAMESPACE), TERMINAL_LOGS_TOOL)));
    assert!(tool_names.contains(&(Some(TERMINAL_TOOL_NAMESPACE), TERMINAL_RUN_TOOL)));
}

#[test]
fn apply_terminal_thread_start_context_is_idempotent() {
    let mut params = ThreadStartParams::default();

    apply_terminal_thread_start_context(&mut params);
    apply_terminal_thread_start_context(&mut params);

    let instructions = params
        .developer_instructions
        .as_deref()
        .expect("developer instructions should be set");
    assert_eq!(
        instructions
            .matches(TERMINAL_DEVELOPER_INSTRUCTIONS)
            .count(),
        1
    );

    let open_count = params
        .dynamic_tools
        .as_ref()
        .expect("dynamic tools should be set")
        .iter()
        .filter(|spec| {
            spec.namespace.as_deref() == Some(TERMINAL_TOOL_NAMESPACE)
                && spec.name == TERMINAL_OPEN_TOOL
        })
        .count();
    assert_eq!(open_count, 1);
}

#[test]
fn terminal_thread_start_params_are_serializable() {
    let mut params = ThreadStartParams {
        persist_extended_history: true,
        ..ThreadStartParams::default()
    };
    apply_terminal_thread_start_context(&mut params);

    let value = serde_json::to_value(&params).expect("thread params should serialize");
    let tools = value
        .get("dynamicTools")
        .and_then(serde_json::Value::as_array)
        .expect("dynamic tools should serialize");
    assert!(tools.iter().any(|tool| {
        tool.get("namespace").and_then(serde_json::Value::as_str) == Some(TERMINAL_TOOL_NAMESPACE)
            && tool.get("name").and_then(serde_json::Value::as_str) == Some(TERMINAL_OPEN_TOOL)
    }));
}

#[test]
fn parse_terminal_open_and_snapshot_requests() {
    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_OPEN_TOOL,
            serde_json::json!({})
        ))
        .expect("open should parse"),
        TerminalDynamicToolRequest::Open { tab_id: None }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_SNAPSHOT_TOOL,
            serde_json::json!({
                "tabId": 2,
                "includeCells": true,
            })
        ))
        .expect("snapshot should parse"),
        TerminalDynamicToolRequest::Snapshot {
            tab_id: Some(TerminalTabId::new(2)),
            include_cells: true,
        }
    );
}

#[test]
fn parse_terminal_tab_requests() {
    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_TABS_TOOL,
            serde_json::json!({})
        ))
        .expect("tabs should parse"),
        TerminalDynamicToolRequest::Tabs
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_NEW_TAB_TOOL,
            serde_json::json!({})
        ))
        .expect("new tab should parse"),
        TerminalDynamicToolRequest::NewTab { activate: true }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_NEW_TAB_TOOL,
            serde_json::json!({ "activate": false })
        ))
        .expect("new inactive tab should parse"),
        TerminalDynamicToolRequest::NewTab { activate: false }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_SELECT_TAB_TOOL,
            serde_json::json!({ "tabId": 3 })
        ))
        .expect("select tab should parse"),
        TerminalDynamicToolRequest::SelectTab {
            tab_id: TerminalTabId::new(3),
        }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_CLOSE_TAB_TOOL,
            serde_json::json!({ "tabId": 3 })
        ))
        .expect("close tab should parse"),
        TerminalDynamicToolRequest::CloseTab {
            tab_id: TerminalTabId::new(3),
        }
    );
}

#[test]
fn parse_terminal_logs_request_applies_defaults_and_limits() {
    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_LOGS_TOOL,
            serde_json::json!({})
        ))
        .expect("logs should parse"),
        TerminalDynamicToolRequest::Logs {
            tab_id: None,
            since_sequence: None,
            limit: 100,
        }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_LOGS_TOOL,
            serde_json::json!({
                "tabId": 2,
                "sinceSequence": 7,
                "limit": 900
            })
        ))
        .expect("logs filters should parse"),
        TerminalDynamicToolRequest::Logs {
            tab_id: Some(TerminalTabId::new(2)),
            since_sequence: Some(7),
            limit: 500,
        }
    );
}

#[test]
fn parse_terminal_input_requests() {
    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_RUN_TOOL,
            serde_json::json!({
                "tabId": 2,
                "command": "npm run dev"
            })
        ))
        .expect("run should parse"),
        TerminalDynamicToolRequest::Run {
            tab_id: Some(TerminalTabId::new(2)),
            command: "npm run dev".to_string(),
        }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_TYPE_TOOL,
            serde_json::json!({ "text": "hello" })
        ))
        .expect("type should parse"),
        TerminalDynamicToolRequest::Type {
            tab_id: None,
            text: "hello".to_string(),
        }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_PASTE_TOOL,
            serde_json::json!({ "text": "hello\nworld" })
        ))
        .expect("paste should parse"),
        TerminalDynamicToolRequest::Paste {
            tab_id: None,
            text: "hello\nworld".to_string(),
        }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_PRESS_TOOL,
            serde_json::json!({ "keys": "Ctrl+C" })
        ))
        .expect("press should parse"),
        TerminalDynamicToolRequest::Press {
            tab_id: None,
            keys: "Ctrl+C".to_string(),
        }
    );
}

#[test]
fn parse_terminal_scroll_resize_and_kill_requests() {
    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_SCROLL_TOOL,
            serde_json::json!({
                "tabId": 2,
                "lines": -20
            })
        ))
        .expect("scroll should parse"),
        TerminalDynamicToolRequest::Scroll {
            tab_id: Some(TerminalTabId::new(2)),
            lines: -20,
        }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_RESIZE_TOOL,
            serde_json::json!({
                "rows": 32,
                "cols": 120
            })
        ))
        .expect("resize should parse"),
        TerminalDynamicToolRequest::Resize {
            tab_id: None,
            rows: 32,
            cols: 120,
        }
    );

    assert_eq!(
        parse_terminal_dynamic_tool_request(&dynamic_tool_params(
            TERMINAL_KILL_TOOL,
            serde_json::json!({
                "tabId": 2
            })
        ))
        .expect("kill should parse"),
        TerminalDynamicToolRequest::Kill {
            tab_id: Some(TerminalTabId::new(2)),
        }
    );
}

#[test]
fn parse_terminal_request_returns_argument_errors() {
    let error = parse_terminal_dynamic_tool_request(&dynamic_tool_params(
        TERMINAL_RUN_TOOL,
        serde_json::json!({}),
    ))
    .expect_err("missing command should fail");

    assert!(error.contains("invalid terminal dynamic tool arguments"));

    let error = parse_terminal_dynamic_tool_request(&dynamic_tool_params(
        TERMINAL_SELECT_TAB_TOOL,
        serde_json::json!({ "tabId": 0 }),
    ))
    .expect_err("zero tab id should fail");

    assert!(error.contains("terminal tabId must be greater than zero"));

    let error = parse_terminal_dynamic_tool_request(&dynamic_tool_params(
        TERMINAL_RESIZE_TOOL,
        serde_json::json!({
            "rows": 0,
            "cols": 80
        }),
    ))
    .expect_err("zero rows should fail");

    assert!(error.contains("rows must be between 1"));
}

fn dynamic_tool_params(tool: &str, arguments: serde_json::Value) -> DynamicToolCallParams {
    DynamicToolCallParams {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
        namespace: Some(TERMINAL_TOOL_NAMESPACE.to_string()),
        tool: tool.to_string(),
        arguments,
    }
}
