use std::fs;

use hunk_codex::protocol::JSONRPCMessage;
use hunk_codex::protocol::JSONRPCRequest;
use hunk_codex::protocol::RequestId;
use hunk_codex::protocol::generate_json_with_experimental;
use tempfile::TempDir;

#[test]
fn protocol_json_schema_bundle_generates() {
    let temp_dir = TempDir::new().expect("temp dir must be created");
    generate_json_with_experimental(temp_dir.path(), true)
        .expect("schema bundle generation should succeed");

    let bundle_path = temp_dir
        .path()
        .join("codex_app_server_protocol.v2.schemas.json");
    let bundle = fs::read_to_string(bundle_path).expect("bundle should exist");

    assert!(bundle.contains("thread/start"));
    assert!(bundle.contains("turn/start"));
    assert!(bundle.contains("account/login/start"));
}

#[test]
fn jsonrpc_request_round_trip() {
    let request = JSONRPCRequest {
        id: RequestId::Integer(99),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({ "experimentalApi": true })),
        trace: None,
    };

    let wire = serde_json::to_string(&JSONRPCMessage::Request(request.clone()))
        .expect("serialization should succeed");
    let decoded: JSONRPCMessage =
        serde_json::from_str(&wire).expect("deserialization should succeed");

    assert_eq!(decoded, JSONRPCMessage::Request(request));
}
