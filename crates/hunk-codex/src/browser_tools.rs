use crate::protocol::DynamicToolSpec;
use serde_json::Value;
use serde_json::json;

pub const BROWSER_NAVIGATE_TOOL: &str = "hunk.browser_navigate";
pub const BROWSER_SNAPSHOT_TOOL: &str = "hunk.browser_snapshot";
pub const BROWSER_CLICK_TOOL: &str = "hunk.browser_click";
pub const BROWSER_TYPE_TOOL: &str = "hunk.browser_type";
pub const BROWSER_PRESS_TOOL: &str = "hunk.browser_press";
pub const BROWSER_SCROLL_TOOL: &str = "hunk.browser_scroll";
pub const BROWSER_SCREENSHOT_TOOL: &str = "hunk.browser_screenshot";

pub const BROWSER_DEVELOPER_INSTRUCTIONS: &str = r#"When the embedded Hunk browser is available, use it through the hunk.browser_* tools instead of trying to launch an external browser.
Use hunk.browser_snapshot before clicking or typing. The snapshot returns a snapshotEpoch and indexed visible elements; pass that same snapshotEpoch and element index to hunk.browser_click or hunk.browser_type.
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
            | BROWSER_SNAPSHOT_TOOL
            | BROWSER_CLICK_TOOL
            | BROWSER_TYPE_TOOL
            | BROWSER_PRESS_TOOL
            | BROWSER_SCROLL_TOOL
            | BROWSER_SCREENSHOT_TOOL
    )
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
