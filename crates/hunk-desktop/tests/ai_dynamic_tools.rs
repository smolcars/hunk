use std::fs;

use hunk_browser::{BrowserFrame, BrowserRuntime};
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
    let mut executor = AiDynamicToolExecutor::new();

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
    let mut executor = AiDynamicToolExecutor::new();

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
fn browser_tool_calls_validate_arguments_before_backend_routing() {
    let temp = tempdir().expect("temp dir should be created");
    let mut executor = AiDynamicToolExecutor::new();

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.browser_click",
            serde_json::json!({
                "index": 42
            }),
        ),
    );

    assert!(!response.success);
    let text = response_text(&response);
    assert!(
        text.contains("invalidBrowserToolArguments"),
        "unexpected response: {text}"
    );
    assert!(
        text.contains("snapshotEpoch"),
        "unexpected response: {text}"
    );
}

#[test]
fn unsupported_non_browser_tool_still_returns_workspace_error() {
    let temp = tempdir().expect("temp dir should be created");
    let mut executor = AiDynamicToolExecutor::new();

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.unknown", serde_json::json!({})),
    );

    assert!(!response.success);
    assert!(response_text(&response).contains("unsupported dynamic tool"));
}

#[test]
fn browser_state_only_executor_routes_navigation_and_snapshot() {
    let temp = tempdir().expect("temp dir should be created");
    let mut executor = AiDynamicToolExecutor::with_state_only_browser();

    let navigate = executor.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.browser_navigate",
            serde_json::json!({ "url": "https://example.com" }),
        ),
    );

    assert!(navigate.success);
    let navigate_json = response_json(&navigate);
    assert_eq!(navigate_json["action"], "navigate");
    assert_eq!(navigate_json["url"], "https://example.com");

    let snapshot = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_snapshot", serde_json::json!({})),
    );

    assert!(snapshot.success);
    let snapshot_json = response_json(&snapshot);
    assert_eq!(snapshot_json["url"], "https://example.com");
    assert_eq!(snapshot_json["snapshotEpoch"], 1);
    assert!(
        snapshot_json["elements"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
}

#[test]
fn browser_state_only_executor_routes_navigation_controls() {
    let temp = tempdir().expect("temp dir should be created");
    let mut executor = AiDynamicToolExecutor::with_state_only_browser();

    let first = executor.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.browser_navigate",
            serde_json::json!({ "url": "https://example.com/a" }),
        ),
    );
    assert!(first.success);

    let second = executor.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.browser_navigate",
            serde_json::json!({ "url": "https://example.com/b" }),
        ),
    );
    assert!(second.success);

    let back = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_back", serde_json::json!({})),
    );
    assert!(back.success);
    let back_json = response_json(&back);
    assert_eq!(back_json["action"], "back");
    assert_eq!(back_json["url"], "https://example.com/a");

    let forward = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_forward", serde_json::json!({})),
    );
    assert!(forward.success);
    let forward_json = response_json(&forward);
    assert_eq!(forward_json["action"], "forward");
    assert_eq!(forward_json["url"], "https://example.com/b");

    let reload = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_reload", serde_json::json!({})),
    );
    assert!(reload.success);
    let reload_json = response_json(&reload);
    assert_eq!(reload_json["action"], "reload");
    assert_eq!(reload_json["loading"], true);

    let stop = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_stop", serde_json::json!({})),
    );
    assert!(stop.success);
    let stop_json = response_json(&stop);
    assert_eq!(stop_json["action"], "stop");
    assert_eq!(stop_json["loading"], false);
}

#[test]
fn browser_state_only_executor_returns_confirmation_required_for_sensitive_actions() {
    let temp = tempdir().expect("temp dir should be created");
    let mut executor = AiDynamicToolExecutor::with_state_only_browser();

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.browser_navigate",
            serde_json::json!({ "url": "mailto:support@example.com" }),
        ),
    );

    assert!(!response.success);
    let text = response_text(&response);
    assert!(
        text.contains("browserConfirmationRequired"),
        "unexpected response: {text}"
    );
    assert!(
        text.contains("ExternalProtocol"),
        "unexpected response: {text}"
    );
}

#[test]
fn browser_state_only_executor_rejects_unknown_snapshot_elements() {
    let temp = tempdir().expect("temp dir should be created");
    let mut executor = AiDynamicToolExecutor::with_state_only_browser();

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params(
            "hunk.browser_click",
            serde_json::json!({
                "snapshotEpoch": 0,
                "index": 1
            }),
        ),
    );

    assert!(!response.success);
    let text = response_text(&response);
    assert!(
        text.contains("browserActionRejected"),
        "unexpected response: {text}"
    );
    assert!(
        text.contains("element index 1"),
        "unexpected response: {text}"
    );
}

#[test]
fn browser_screenshot_returns_input_image_when_frame_exists() {
    let temp = tempdir().expect("temp dir should be created");
    let mut runtime = BrowserRuntime::new_disabled();
    runtime.ensure_session("thread-1").set_latest_frame(
        BrowserFrame::from_bgra(1, 1, 7, vec![0, 0, 255, 255])
            .expect("valid frame should be accepted"),
    );
    let mut executor = AiDynamicToolExecutor::with_browser_runtime(runtime);

    let response = executor.execute(
        temp.path(),
        &dynamic_tool_params("hunk.browser_screenshot", serde_json::json!({})),
    );

    assert!(response.success);
    let text = response_text(&response);
    assert!(text.contains("\"frame\""), "unexpected response: {text}");
    let image_url = response_image_url(&response).expect("screenshot should include image item");
    assert!(
        image_url.starts_with("data:image/png;base64,"),
        "unexpected image URL: {image_url}"
    );
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

fn response_image_url(response: &DynamicToolCallResponse) -> Option<String> {
    response.content_items.iter().find_map(|item| match item {
        DynamicToolCallOutputContentItem::InputText { .. } => None,
        DynamicToolCallOutputContentItem::InputImage { image_url } => Some(image_url.clone()),
    })
}

fn response_json(response: &DynamicToolCallResponse) -> serde_json::Value {
    serde_json::from_str(&response_text(response)).expect("response should be JSON")
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
