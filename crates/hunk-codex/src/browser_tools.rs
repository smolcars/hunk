use crate::protocol::DynamicToolCallParams;
use crate::protocol::DynamicToolSpec;
use crate::protocol::ThreadStartParams;
use hunk_browser::BrowserAction;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

pub const BROWSER_NAVIGATE_TOOL: &str = "hunk.browser_navigate";
pub const BROWSER_RELOAD_TOOL: &str = "hunk.browser_reload";
pub const BROWSER_STOP_TOOL: &str = "hunk.browser_stop";
pub const BROWSER_BACK_TOOL: &str = "hunk.browser_back";
pub const BROWSER_FORWARD_TOOL: &str = "hunk.browser_forward";
pub const BROWSER_SNAPSHOT_TOOL: &str = "hunk.browser_snapshot";
pub const BROWSER_CLICK_TOOL: &str = "hunk.browser_click";
pub const BROWSER_TYPE_TOOL: &str = "hunk.browser_type";
pub const BROWSER_PRESS_TOOL: &str = "hunk.browser_press";
pub const BROWSER_SCROLL_TOOL: &str = "hunk.browser_scroll";
pub const BROWSER_SCREENSHOT_TOOL: &str = "hunk.browser_screenshot";

pub const BROWSER_DEVELOPER_INSTRUCTIONS: &str = r#"When the embedded Hunk browser is available, use it through the hunk.browser_* tools instead of trying to launch an external browser.
Use hunk.browser_snapshot before clicking or typing. The snapshot returns a snapshotEpoch and indexed visible elements; pass that same snapshotEpoch and element index to hunk.browser_click or hunk.browser_type.
Use hunk.browser_reload, hunk.browser_stop, hunk.browser_back, and hunk.browser_forward for browser-level navigation controls.
If a browser action reports that confirmation is required, stop and wait for the user decision before continuing."#;

pub fn browser_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        spec(
            BROWSER_NAVIGATE_TOOL,
            "Navigate the embedded Hunk browser for the active AI thread to a URL.",
            object_schema(
                json!({
                    "url": {
                        "type": "string",
                        "description": "Absolute http or https URL to load."
                    }
                }),
                &["url"],
            ),
        ),
        spec(
            BROWSER_SNAPSHOT_TOOL,
            "Read the embedded browser page state and visible interactive element index map.",
            object_schema(json!({}), &[]),
        ),
        spec(
            BROWSER_RELOAD_TOOL,
            "Reload the current embedded browser page.",
            object_schema(json!({}), &[]),
        ),
        spec(
            BROWSER_STOP_TOOL,
            "Stop loading the current embedded browser page.",
            object_schema(json!({}), &[]),
        ),
        spec(
            BROWSER_BACK_TOOL,
            "Navigate the embedded browser back in its history.",
            object_schema(json!({}), &[]),
        ),
        spec(
            BROWSER_FORWARD_TOOL,
            "Navigate the embedded browser forward in its history.",
            object_schema(json!({}), &[]),
        ),
        spec(
            BROWSER_CLICK_TOOL,
            "Click an indexed element from the latest embedded browser snapshot.",
            object_schema(
                json!({
                    "snapshotEpoch": {
                        "type": "integer",
                        "description": "Epoch from the latest browser snapshot."
                    },
                    "index": {
                        "type": "integer",
                        "description": "Element index from the latest browser snapshot."
                    }
                }),
                &["snapshotEpoch", "index"],
            ),
        ),
        spec(
            BROWSER_TYPE_TOOL,
            "Type text into an indexed element from the latest embedded browser snapshot.",
            object_schema(
                json!({
                    "snapshotEpoch": {
                        "type": "integer",
                        "description": "Epoch from the latest browser snapshot."
                    },
                    "index": {
                        "type": "integer",
                        "description": "Element index from the latest browser snapshot."
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type into the target element."
                    },
                    "clear": {
                        "type": "boolean",
                        "description": "Whether to clear existing text before typing."
                    }
                }),
                &["snapshotEpoch", "index", "text"],
            ),
        ),
        spec(
            BROWSER_PRESS_TOOL,
            "Press keyboard keys in the embedded browser.",
            object_schema(
                json!({
                    "keys": {
                        "type": "string",
                        "description": "Key sequence such as Enter, Escape, Tab, Ctrl+L, or Cmd+L."
                    }
                }),
                &["keys"],
            ),
        ),
        spec(
            BROWSER_SCROLL_TOOL,
            "Scroll the embedded browser page or an indexed scrollable element.",
            object_schema(
                json!({
                    "down": {
                        "type": "boolean",
                        "description": "True to scroll down, false to scroll up."
                    },
                    "pages": {
                        "type": "number",
                        "description": "Number of viewport pages to scroll."
                    },
                    "index": {
                        "type": "integer",
                        "description": "Optional element index from the latest browser snapshot."
                    }
                }),
                &[],
            ),
        ),
        spec(
            BROWSER_SCREENSHOT_TOOL,
            "Capture a screenshot of the embedded browser viewport.",
            object_schema(json!({}), &[]),
        ),
    ]
}

pub fn is_browser_dynamic_tool(tool: &str) -> bool {
    matches!(
        tool,
        BROWSER_NAVIGATE_TOOL
            | BROWSER_RELOAD_TOOL
            | BROWSER_STOP_TOOL
            | BROWSER_BACK_TOOL
            | BROWSER_FORWARD_TOOL
            | BROWSER_SNAPSHOT_TOOL
            | BROWSER_CLICK_TOOL
            | BROWSER_TYPE_TOOL
            | BROWSER_PRESS_TOOL
            | BROWSER_SCROLL_TOOL
            | BROWSER_SCREENSHOT_TOOL
    )
}

pub fn apply_browser_thread_start_context(params: &mut ThreadStartParams) {
    append_browser_developer_instructions(&mut params.developer_instructions);

    let mut dynamic_tools = params.dynamic_tools.take().unwrap_or_default();
    for spec in browser_dynamic_tool_specs() {
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
pub enum BrowserDynamicToolRequest {
    Snapshot,
    Screenshot,
    Action(BrowserAction),
}

pub fn parse_browser_dynamic_tool_request(
    params: &DynamicToolCallParams,
) -> Result<BrowserDynamicToolRequest, String> {
    match params.tool.as_str() {
        BROWSER_NAVIGATE_TOOL => {
            let args = parse_args::<NavigateArgs>(&params.arguments)?;
            Ok(BrowserDynamicToolRequest::Action(BrowserAction::Navigate {
                url: args.url,
            }))
        }
        BROWSER_SNAPSHOT_TOOL => Ok(BrowserDynamicToolRequest::Snapshot),
        BROWSER_RELOAD_TOOL => Ok(BrowserDynamicToolRequest::Action(BrowserAction::Reload)),
        BROWSER_STOP_TOOL => Ok(BrowserDynamicToolRequest::Action(BrowserAction::Stop)),
        BROWSER_BACK_TOOL => Ok(BrowserDynamicToolRequest::Action(BrowserAction::Back)),
        BROWSER_FORWARD_TOOL => Ok(BrowserDynamicToolRequest::Action(BrowserAction::Forward)),
        BROWSER_CLICK_TOOL => {
            let args = parse_args::<IndexedElementArgs>(&params.arguments)?;
            Ok(BrowserDynamicToolRequest::Action(BrowserAction::Click {
                snapshot_epoch: args.snapshot_epoch,
                index: args.index,
            }))
        }
        BROWSER_TYPE_TOOL => {
            let args = parse_args::<TypeArgs>(&params.arguments)?;
            Ok(BrowserDynamicToolRequest::Action(BrowserAction::Type {
                snapshot_epoch: args.snapshot_epoch,
                index: args.index,
                text: args.text,
                clear: args.clear.unwrap_or(true),
            }))
        }
        BROWSER_PRESS_TOOL => {
            let args = parse_args::<PressArgs>(&params.arguments)?;
            Ok(BrowserDynamicToolRequest::Action(BrowserAction::Press {
                keys: args.keys,
            }))
        }
        BROWSER_SCROLL_TOOL => {
            let args = parse_args::<ScrollArgs>(&params.arguments)?;
            Ok(BrowserDynamicToolRequest::Action(BrowserAction::Scroll {
                down: args.down.unwrap_or(true),
                pages: args.pages.unwrap_or(1.0),
                index: args.index,
            }))
        }
        BROWSER_SCREENSHOT_TOOL => Ok(BrowserDynamicToolRequest::Screenshot),
        _ => Err(format!(
            "unsupported browser dynamic tool '{}'",
            params.tool
        )),
    }
}

fn append_browser_developer_instructions(instructions: &mut Option<String>) {
    match instructions {
        Some(existing) if existing.contains(BROWSER_DEVELOPER_INSTRUCTIONS) => {}
        Some(existing) if existing.trim().is_empty() => {
            *existing = BROWSER_DEVELOPER_INSTRUCTIONS.to_string();
        }
        Some(existing) => {
            existing.push_str("\n\n");
            existing.push_str(BROWSER_DEVELOPER_INSTRUCTIONS);
        }
        None => {
            *instructions = Some(BROWSER_DEVELOPER_INSTRUCTIONS.to_string());
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NavigateArgs {
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexedElementArgs {
    snapshot_epoch: u64,
    index: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeArgs {
    snapshot_epoch: u64,
    index: u32,
    text: String,
    #[serde(default)]
    clear: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PressArgs {
    keys: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrollArgs {
    #[serde(default)]
    down: Option<bool>,
    #[serde(default)]
    pages: Option<f64>,
    #[serde(default)]
    index: Option<u32>,
}

fn parse_args<T>(arguments: &Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|error| format!("invalid browser dynamic tool arguments: {error}"))
}

fn spec(name: &str, description: &str, input_schema: Value) -> DynamicToolSpec {
    DynamicToolSpec {
        namespace: None,
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
