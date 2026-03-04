use std::fs;

use codex_app_server_protocol::DynamicToolCallOutputContentItem;
use codex_app_server_protocol::DynamicToolCallParams;
use codex_app_server_protocol::DynamicToolCallResponse;
use hunk_codex::tools::DynamicToolRegistry;
use tempfile::tempdir;

#[test]
fn unsupported_tool_returns_structured_error() {
    let temp = tempdir().expect("temp dir should be created");
    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        temp.path(),
        &dynamic_tool_params("hunk.unknown", serde_json::json!({})),
    );

    assert!(!response.success);
    assert!(
        response_text(&response).contains("unsupported dynamic tool"),
        "unexpected response: {response:?}"
    );
}

#[test]
fn list_directory_rejects_parent_path_traversal() {
    let temp = tempdir().expect("temp dir should be created");
    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.list_directory",
            serde_json::json!({"path": "../outside"}),
        ),
    );

    assert!(!response.success);
    assert!(
        response_text(&response).contains("parent path traversal is not allowed"),
        "unexpected response: {response:?}"
    );
}

#[test]
fn read_file_honors_max_bytes_and_sets_truncated_flag() {
    let temp = tempdir().expect("temp dir should be created");
    let file_path = temp.path().join("notes.txt");
    fs::write(&file_path, "hello world").expect("test file should be written");

    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.read_file",
            serde_json::json!({
                "path": "notes.txt",
                "maxBytes": 5
            }),
        ),
    );

    assert!(response.success);
    let payload: serde_json::Value =
        serde_json::from_str(&response_text(&response)).expect("response should be json");
    assert_eq!(payload["truncated"], serde_json::json!(true));
    assert_eq!(payload["content"], serde_json::json!("hello"));
}

#[test]
fn workspace_summary_serializes_workspace_counts() {
    let temp = tempdir().expect("temp dir should be created");
    fs::write(temp.path().join("a.txt"), "a").expect("test file should be written");
    fs::create_dir(temp.path().join("src")).expect("test dir should be created");

    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        temp.path(),
        &dynamic_tool_params("hunk.workspace_summary", serde_json::json!({})),
    );

    assert!(response.success);
    let payload: serde_json::Value =
        serde_json::from_str(&response_text(&response)).expect("response should be json");
    assert_eq!(payload["fileCount"], serde_json::json!(1));
    assert_eq!(payload["directoryCount"], serde_json::json!(1));
}

fn dynamic_tool_params(tool: &str, arguments: serde_json::Value) -> DynamicToolCallParams {
    DynamicToolCallParams {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        call_id: "call-1".to_string(),
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
