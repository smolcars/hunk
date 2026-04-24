use std::fs;

use hunk_codex::protocol::DynamicToolCallOutputContentItem;
use hunk_codex::protocol::DynamicToolCallParams;
use hunk_codex::protocol::DynamicToolCallResponse;
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
fn browser_tool_returns_structured_error_until_executor_is_connected() {
    let temp = tempdir().expect("temp dir should be created");
    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_snapshot", serde_json::json!({})),
    );

    assert!(!response.success);
    assert!(
        response_text(&response).contains("no embedded browser executor is connected"),
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

#[cfg(unix)]
#[test]
fn read_file_rejects_symlink_escape_outside_workspace() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("workspace dir should be created");
    let outside = tempdir().expect("outside dir should be created");
    let outside_file = outside.path().join("secret.txt");
    fs::write(&outside_file, "top-secret").expect("outside file should be created");

    let link_path = workspace.path().join("leak.txt");
    symlink(&outside_file, &link_path).expect("symlink should be created");

    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        workspace.path(),
        &dynamic_tool_params(
            "hunk.read_file",
            serde_json::json!({
                "path": "leak.txt"
            }),
        ),
    );

    assert!(!response.success);
    assert!(
        response_text(&response).contains("escapes workspace root"),
        "unexpected response: {response:?}"
    );
}

#[cfg(unix)]
#[test]
fn list_directory_rejects_symlink_escape_outside_workspace() {
    use std::os::unix::fs::symlink;

    let workspace = tempdir().expect("workspace dir should be created");
    let outside = tempdir().expect("outside dir should be created");
    let outside_dir = outside.path().join("external");
    fs::create_dir_all(&outside_dir).expect("outside directory should be created");
    fs::write(outside_dir.join("file.txt"), "data").expect("outside file should be created");

    let link_path = workspace.path().join("external-link");
    symlink(&outside_dir, &link_path).expect("symlink should be created");

    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        workspace.path(),
        &dynamic_tool_params(
            "hunk.list_directory",
            serde_json::json!({
                "path": "external-link"
            }),
        ),
    );

    assert!(!response.success);
    assert!(
        response_text(&response).contains("escapes workspace root"),
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
fn list_directory_applies_max_entries_after_hidden_filtering() {
    let temp = tempdir().expect("temp dir should be created");
    fs::write(temp.path().join(".hidden-a"), "a").expect("hidden file should be written");
    fs::write(temp.path().join(".hidden-b"), "b").expect("hidden file should be written");
    fs::write(temp.path().join("visible-a.txt"), "a").expect("visible file should be written");
    fs::write(temp.path().join("visible-b.txt"), "b").expect("visible file should be written");

    let registry = DynamicToolRegistry::new();
    let response = registry.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.list_directory",
            serde_json::json!({
                "maxEntries": 2
            }),
        ),
    );

    assert!(response.success);
    let payload: serde_json::Value =
        serde_json::from_str(&response_text(&response)).expect("response should be json");
    let entry_names = payload["entries"]
        .as_array()
        .expect("entries should be an array")
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect::<Vec<_>>();

    assert_eq!(entry_names.len(), 2);
    assert!(entry_names.iter().all(|name| !name.starts_with('.')));
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
