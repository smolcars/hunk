use std::fs;

use hunk_codex::protocol::{
    DynamicToolCallOutputContentItem, DynamicToolCallParams, DynamicToolCallResponse,
};
use tempfile::tempdir;

#[path = "../src/app/ai_dynamic_tools.rs"]
mod ai_dynamic_tools;

use ai_dynamic_tools::AiDynamicToolExecutor;

#[test]
fn workspace_tool_calls_still_route_to_workspace_registry() {
    let temp = tempdir().expect("temp dir should be created");
    fs::write(temp.path().join("file.txt"), "hello").expect("file should be written");
    let executor = AiDynamicToolExecutor::new();

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.list_directory", serde_json::json!({})),
    );

    assert!(response.success);
    assert!(response_text(&response).contains("file.txt"));
}

#[test]
fn browser_tool_calls_return_disabled_browser_response() {
    let temp = tempdir().expect("temp dir should be created");
    let executor = AiDynamicToolExecutor::new();

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_snapshot", serde_json::json!({})),
    );

    assert!(!response.success);
    let text = response_text(&response);
    assert!(
        text.contains("browserUnavailable"),
        "unexpected response: {text}"
    );
    assert!(
        text.contains("embedded browser executor is not connected yet"),
        "unexpected response: {text}"
    );
}

#[test]
fn unsupported_non_browser_tool_still_returns_workspace_error() {
    let temp = tempdir().expect("temp dir should be created");
    let executor = AiDynamicToolExecutor::new();

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.unknown", serde_json::json!({})),
    );

    assert!(!response.success);
    assert!(response_text(&response).contains("unsupported dynamic tool"));
}

fn dynamic_tool_params(tool: &str, arguments: serde_json::Value) -> DynamicToolCallParams {
    DynamicToolCallParams {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
        namespace: None,
        tool: tool.to_string(),
        arguments,
    }
}

fn response_text(response: &DynamicToolCallResponse) -> String {
    response
        .content_items
        .iter()
        .find_map(|item| match item {
            DynamicToolCallOutputContentItem::InputText { text } => Some(text.clone()),
            DynamicToolCallOutputContentItem::InputImage { .. } => None,
        })
        .unwrap_or_default()
}
