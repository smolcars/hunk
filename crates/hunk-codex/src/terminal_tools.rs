use crate::protocol::DynamicToolCallParams;
use crate::protocol::DynamicToolSpec;
use crate::protocol::ThreadStartParams;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

pub const TERMINAL_TOOL_NAMESPACE: &str = "hunk_terminal";
pub const TERMINAL_OPEN_TOOL: &str = "open";
pub const TERMINAL_TABS_TOOL: &str = "tabs";
pub const TERMINAL_NEW_TAB_TOOL: &str = "new_tab";
pub const TERMINAL_SELECT_TAB_TOOL: &str = "select_tab";
pub const TERMINAL_CLOSE_TAB_TOOL: &str = "close_tab";
pub const TERMINAL_SNAPSHOT_TOOL: &str = "snapshot";
pub const TERMINAL_LOGS_TOOL: &str = "logs";
pub const TERMINAL_RUN_TOOL: &str = "run";
pub const TERMINAL_TYPE_TOOL: &str = "type";
pub const TERMINAL_PASTE_TOOL: &str = "paste";
pub const TERMINAL_PRESS_TOOL: &str = "press";
pub const TERMINAL_SCROLL_TOOL: &str = "scroll";
pub const TERMINAL_RESIZE_TOOL: &str = "resize";
pub const TERMINAL_KILL_TOOL: &str = "kill";

pub const TERMINAL_DEVELOPER_INSTRUCTIONS: &str = r#"When the user asks to open, inspect, read, or control Hunk's built-in AI terminal, use Hunk's hunk_terminal dynamic tools.
Use hunk_terminal.open to open the terminal and ensure a shell session exists for the active AI thread.
Use hunk_terminal.snapshot before relying on terminal screen state. Snapshot responses include visible text, cursor, mode, size, active tab, tab summaries, cwd, and status.
Use hunk_terminal.logs for long-running server output or build/test logs.
Use hunk_terminal.tabs to inspect terminal tabs. When multiple tabs are open, pass tabId to snapshot, logs, run, type, paste, press, scroll, resize, and kill, or use hunk_terminal.select_tab before operating on a specific tab.
Use hunk_terminal.run to submit a shell command. Use hunk_terminal.type or hunk_terminal.paste only when interacting with prompts or partial command lines.
Use hunk_terminal.press for interactive keys such as Enter, Ctrl+C, Up, Down, Tab, Shift+PageUp, or Shift+PageDown.
Coordinate terminal and browser work by inspecting both hunk_terminal and hunk_browser surfaces instead of launching external terminal or browser automation.
If a terminal action reports that confirmation is required, stop and wait for the user decision before continuing."#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalTabId(pub usize);

impl TerminalTabId {
    pub fn new(tab_id: usize) -> Self {
        Self(tab_id)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalDynamicToolRequest {
    Open {
        tab_id: Option<TerminalTabId>,
    },
    Tabs,
    NewTab {
        activate: bool,
    },
    SelectTab {
        tab_id: TerminalTabId,
    },
    CloseTab {
        tab_id: TerminalTabId,
    },
    Snapshot {
        tab_id: Option<TerminalTabId>,
        include_cells: bool,
    },
    Logs {
        tab_id: Option<TerminalTabId>,
        since_sequence: Option<u64>,
        limit: usize,
    },
    Run {
        tab_id: Option<TerminalTabId>,
        command: String,
    },
    Type {
        tab_id: Option<TerminalTabId>,
        text: String,
    },
    Paste {
        tab_id: Option<TerminalTabId>,
        text: String,
    },
    Press {
        tab_id: Option<TerminalTabId>,
        keys: String,
    },
    Scroll {
        tab_id: Option<TerminalTabId>,
        lines: i32,
    },
    Resize {
        tab_id: Option<TerminalTabId>,
        rows: u16,
        cols: u16,
    },
    Kill {
        tab_id: Option<TerminalTabId>,
    },
}

pub fn terminal_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        spec(
            TERMINAL_OPEN_TOOL,
            "Open the AI terminal for the active Hunk thread and ensure a shell session exists.",
            object_schema(optional_tab_properties(json!({})), &[]),
        ),
        spec(
            TERMINAL_TABS_TOOL,
            "List AI terminal tabs for the active Hunk thread.",
            object_schema(json!({}), &[]),
        ),
        spec(
            TERMINAL_NEW_TAB_TOOL,
            "Create a new AI terminal tab.",
            object_schema(
                json!({
                    "activate": {
                        "type": "boolean",
                        "description": "Whether to make the new terminal tab active. Defaults to true."
                    }
                }),
                &[],
            ),
        ),
        spec(
            TERMINAL_SELECT_TAB_TOOL,
            "Select an AI terminal tab by tabId.",
            object_schema(tab_id_properties(), &["tabId"]),
        ),
        spec(
            TERMINAL_CLOSE_TAB_TOOL,
            "Close an AI terminal tab by tabId.",
            object_schema(tab_id_properties(), &["tabId"]),
        ),
        spec(
            TERMINAL_SNAPSHOT_TOOL,
            "Read the current AI terminal screen state.",
            object_schema(
                optional_tab_properties(json!({
                    "includeCells": {
                        "type": "boolean",
                        "description": "Whether to include capped raw terminal cells in addition to visible text."
                    }
                })),
                &[],
            ),
        ),
        spec(
            TERMINAL_LOGS_TOOL,
            "Read recent AI terminal transcript output.",
            object_schema(
                optional_tab_properties(json!({
                    "sinceSequence": {
                        "type": "integer",
                        "description": "Optional sequence cursor. Only entries after this sequence are returned when supported."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum log lines or chunks to return. Defaults to 100 and is capped by Hunk."
                    }
                })),
                &[],
            ),
        ),
        spec(
            TERMINAL_RUN_TOOL,
            "Submit a shell command to the AI terminal.",
            object_schema(
                optional_tab_properties(json!({
                    "command": {
                        "type": "string",
                        "description": "Shell command to submit. Hunk appends a newline if needed."
                    }
                })),
                &["command"],
            ),
        ),
        spec(
            TERMINAL_TYPE_TOOL,
            "Type text into the AI terminal without automatically submitting it.",
            object_schema(
                optional_tab_properties(json!({
                    "text": {
                        "type": "string",
                        "description": "Text to type into the terminal."
                    }
                })),
                &["text"],
            ),
        ),
        spec(
            TERMINAL_PASTE_TOOL,
            "Paste text into the AI terminal using terminal paste semantics.",
            object_schema(
                optional_tab_properties(json!({
                    "text": {
                        "type": "string",
                        "description": "Text to paste into the terminal."
                    }
                })),
                &["text"],
            ),
        ),
        spec(
            TERMINAL_PRESS_TOOL,
            "Press keyboard keys in the AI terminal.",
            object_schema(
                optional_tab_properties(json!({
                    "keys": {
                        "type": "string",
                        "description": "Key sequence such as Enter, Ctrl+C, Up, Down, Tab, Shift+PageUp, or Shift+PageDown."
                    }
                })),
                &["keys"],
            ),
        ),
        spec(
            TERMINAL_SCROLL_TOOL,
            "Scroll the AI terminal viewport.",
            object_schema(
                optional_tab_properties(json!({
                    "lines": {
                        "type": "integer",
                        "description": "Signed number of terminal lines to scroll. Positive scrolls down, negative scrolls up."
                    }
                })),
                &["lines"],
            ),
        ),
        spec(
            TERMINAL_RESIZE_TOOL,
            "Resize the AI terminal grid.",
            object_schema(
                optional_tab_properties(json!({
                    "rows": {
                        "type": "integer",
                        "description": "Terminal row count."
                    },
                    "cols": {
                        "type": "integer",
                        "description": "Terminal column count."
                    }
                })),
                &["rows", "cols"],
            ),
        ),
        spec(
            TERMINAL_KILL_TOOL,
            "Stop the selected AI terminal process.",
            object_schema(optional_tab_properties(json!({})), &[]),
        ),
    ]
}

pub fn is_terminal_dynamic_tool(tool: &str) -> bool {
    matches!(
        tool,
        TERMINAL_OPEN_TOOL
            | TERMINAL_TABS_TOOL
            | TERMINAL_NEW_TAB_TOOL
            | TERMINAL_SELECT_TAB_TOOL
            | TERMINAL_CLOSE_TAB_TOOL
            | TERMINAL_SNAPSHOT_TOOL
            | TERMINAL_LOGS_TOOL
            | TERMINAL_RUN_TOOL
            | TERMINAL_TYPE_TOOL
            | TERMINAL_PASTE_TOOL
            | TERMINAL_PRESS_TOOL
            | TERMINAL_SCROLL_TOOL
            | TERMINAL_RESIZE_TOOL
            | TERMINAL_KILL_TOOL
    )
}

pub fn is_terminal_dynamic_tool_call(namespace: Option<&str>, tool: &str) -> bool {
    namespace == Some(TERMINAL_TOOL_NAMESPACE) && is_terminal_dynamic_tool(tool)
}

pub fn apply_terminal_thread_start_context(params: &mut ThreadStartParams) {
    append_terminal_developer_instructions(&mut params.developer_instructions);

    let mut dynamic_tools = params.dynamic_tools.take().unwrap_or_default();
    for spec in terminal_dynamic_tool_specs() {
        if !dynamic_tools
            .iter()
            .any(|existing| existing.name == spec.name && existing.namespace == spec.namespace)
        {
            dynamic_tools.push(spec);
        }
    }
    params.dynamic_tools = Some(dynamic_tools);
}

pub fn parse_terminal_dynamic_tool_request(
    params: &DynamicToolCallParams,
) -> Result<TerminalDynamicToolRequest, String> {
    if !is_terminal_dynamic_tool_call(params.namespace.as_deref(), params.tool.as_str()) {
        return Err(format!(
            "unsupported terminal dynamic tool '{}{}'",
            params
                .namespace
                .as_deref()
                .map(|namespace| format!("{namespace}."))
                .unwrap_or_default(),
            params.tool
        ));
    }

    match params.tool.as_str() {
        TERMINAL_OPEN_TOOL => {
            let args = parse_args::<OptionalTabArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Open {
                tab_id: optional_tab_id(args.tab_id)?,
            })
        }
        TERMINAL_TABS_TOOL => Ok(TerminalDynamicToolRequest::Tabs),
        TERMINAL_NEW_TAB_TOOL => {
            let args = parse_args::<NewTabArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::NewTab {
                activate: args.activate.unwrap_or(true),
            })
        }
        TERMINAL_SELECT_TAB_TOOL => {
            let args = parse_args::<TabIdArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::SelectTab {
                tab_id: terminal_tab_id(args.tab_id)?,
            })
        }
        TERMINAL_CLOSE_TAB_TOOL => {
            let args = parse_args::<TabIdArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::CloseTab {
                tab_id: terminal_tab_id(args.tab_id)?,
            })
        }
        TERMINAL_SNAPSHOT_TOOL => {
            let args = parse_args::<SnapshotArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Snapshot {
                tab_id: optional_tab_id(args.tab_id)?,
                include_cells: args.include_cells.unwrap_or(false),
            })
        }
        TERMINAL_LOGS_TOOL => {
            let args = parse_args::<LogsArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Logs {
                tab_id: optional_tab_id(args.tab_id)?,
                since_sequence: args.since_sequence,
                limit: args.limit.unwrap_or(100).clamp(1, 500),
            })
        }
        TERMINAL_RUN_TOOL => {
            let args = parse_args::<RunArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Run {
                tab_id: optional_tab_id(args.tab_id)?,
                command: args.command,
            })
        }
        TERMINAL_TYPE_TOOL => {
            let args = parse_args::<TextArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Type {
                tab_id: optional_tab_id(args.tab_id)?,
                text: args.text,
            })
        }
        TERMINAL_PASTE_TOOL => {
            let args = parse_args::<TextArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Paste {
                tab_id: optional_tab_id(args.tab_id)?,
                text: args.text,
            })
        }
        TERMINAL_PRESS_TOOL => {
            let args = parse_args::<PressArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Press {
                tab_id: optional_tab_id(args.tab_id)?,
                keys: args.keys,
            })
        }
        TERMINAL_SCROLL_TOOL => {
            let args = parse_args::<ScrollArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Scroll {
                tab_id: optional_tab_id(args.tab_id)?,
                lines: args.lines,
            })
        }
        TERMINAL_RESIZE_TOOL => {
            let args = parse_args::<ResizeArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Resize {
                tab_id: optional_tab_id(args.tab_id)?,
                rows: checked_u16("rows", args.rows)?,
                cols: checked_u16("cols", args.cols)?,
            })
        }
        TERMINAL_KILL_TOOL => {
            let args = parse_args::<OptionalTabArgs>(&params.arguments)?;
            Ok(TerminalDynamicToolRequest::Kill {
                tab_id: optional_tab_id(args.tab_id)?,
            })
        }
        _ => Err(format!(
            "unsupported terminal dynamic tool '{}'",
            params.tool
        )),
    }
}

fn append_terminal_developer_instructions(instructions: &mut Option<String>) {
    match instructions {
        Some(existing) if existing.contains(TERMINAL_DEVELOPER_INSTRUCTIONS) => {}
        Some(existing) if existing.trim().is_empty() => {
            *existing = TERMINAL_DEVELOPER_INSTRUCTIONS.to_string();
        }
        Some(existing) => {
            existing.push_str("\n\n");
            existing.push_str(TERMINAL_DEVELOPER_INSTRUCTIONS);
        }
        None => {
            *instructions = Some(TERMINAL_DEVELOPER_INSTRUCTIONS.to_string());
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionalTabArgs {
    #[serde(default)]
    tab_id: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabIdArgs {
    tab_id: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewTabArgs {
    #[serde(default)]
    activate: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotArgs {
    #[serde(default)]
    tab_id: Option<usize>,
    #[serde(default)]
    include_cells: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogsArgs {
    #[serde(default)]
    tab_id: Option<usize>,
    #[serde(default)]
    since_sequence: Option<u64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunArgs {
    command: String,
    #[serde(default)]
    tab_id: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextArgs {
    text: String,
    #[serde(default)]
    tab_id: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PressArgs {
    keys: String,
    #[serde(default)]
    tab_id: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrollArgs {
    lines: i32,
    #[serde(default)]
    tab_id: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResizeArgs {
    rows: usize,
    cols: usize,
    #[serde(default)]
    tab_id: Option<usize>,
}

fn optional_tab_id(tab_id: Option<usize>) -> Result<Option<TerminalTabId>, String> {
    tab_id.map(terminal_tab_id).transpose()
}

fn terminal_tab_id(tab_id: usize) -> Result<TerminalTabId, String> {
    if tab_id == 0 {
        return Err("terminal tabId must be greater than zero".to_string());
    }
    Ok(TerminalTabId::new(tab_id))
}

fn checked_u16(field: &str, value: usize) -> Result<u16, String> {
    if value == 0 || value > u16::MAX as usize {
        return Err(format!("{field} must be between 1 and {}", u16::MAX));
    }
    Ok(value as u16)
}

fn parse_args<T>(arguments: &Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|error| format!("invalid terminal dynamic tool arguments: {error}"))
}

fn spec(name: &str, description: &str, input_schema: Value) -> DynamicToolSpec {
    DynamicToolSpec {
        namespace: Some(TERMINAL_TOOL_NAMESPACE.to_string()),
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

fn optional_tab_properties(mut properties: Value) -> Value {
    if let Some(properties) = properties.as_object_mut() {
        properties.insert(
            "tabId".to_string(),
            json!({
                "type": "integer",
                "description": "Optional tab ID returned by hunk_terminal.tabs or hunk_terminal.snapshot."
            }),
        );
    }
    properties
}

fn tab_id_properties() -> Value {
    json!({
        "tabId": {
            "type": "integer",
            "description": "Tab ID returned by hunk_terminal.tabs or hunk_terminal.snapshot."
        }
    })
}
