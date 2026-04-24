use std::path::Path;

use hunk_codex::browser_tools::is_browser_dynamic_tool;
use hunk_codex::protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
};
use hunk_codex::tools::DynamicToolRegistry;
use serde_json::json;

#[derive(Debug, Clone)]
pub(crate) struct AiDynamicToolExecutor {
    workspace_tools: DynamicToolRegistry,
    browser_tools: AiBrowserDynamicToolExecutor,
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
        }
    }

    pub(crate) fn execute(
        &self,
        cwd: &Path,
        params: &DynamicToolCallParams,
    ) -> DynamicToolCallResponse {
        if is_browser_dynamic_tool(params.tool.as_str()) {
            return self.browser_tools.execute(params);
        }

        self.workspace_tools.execute(cwd, params)
    }
}

#[derive(Debug, Clone, Copy)]
struct AiBrowserDynamicToolExecutor {
    enabled: bool,
}

impl AiBrowserDynamicToolExecutor {
    const fn disabled() -> Self {
        Self { enabled: false }
    }

    fn execute(&self, params: &DynamicToolCallParams) -> DynamicToolCallResponse {
        if !self.enabled {
            return json_error_response(json!({
                "error": "browserUnavailable",
                "message": "The embedded browser executor is not connected yet.",
                "tool": params.tool,
                "threadId": params.thread_id,
                "turnId": params.turn_id,
            }));
        }

        json_error_response(json!({
            "error": "browserExecutorUnimplemented",
            "message": "The embedded browser executor is enabled but not implemented yet.",
            "tool": params.tool,
            "threadId": params.thread_id,
            "turnId": params.turn_id,
        }))
    }
}

fn json_error_response(value: serde_json::Value) -> DynamicToolCallResponse {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
        format!("{{\"error\":\"browserResponseSerializationFailed\",\"message\":\"{error}\"}}")
    });
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText { text }],
        success: false,
    }
}
