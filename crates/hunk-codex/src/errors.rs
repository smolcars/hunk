use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodexIntegrationError {
    #[error("invalid websocket endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("invalid path '{path}': {source}")]
    InvalidPath {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("host process io failure: {0}")]
    HostProcessIo(#[source] io::Error),
    #[error("json serialization failure: {0}")]
    Serialization(#[source] serde_json::Error),
    #[error("websocket transport failure: {0}")]
    WebSocketTransport(String),
    #[error("json-rpc server error {code}: {message}")]
    JsonRpcServerError { code: i64, message: String },
    #[error("request '{method}' timed out after {timeout_ms}ms")]
    RequestTimedOut { method: String, timeout_ms: u64 },
    #[error("host process is already running")]
    HostAlreadyRunning,
    #[error("host process failed to start within {timeout_ms}ms on port {port}")]
    HostStartupTimedOut { port: u16, timeout_ms: u64 },
    #[error("host process exited before readiness: {status}")]
    HostExitedBeforeReady { status: String },
    #[error(
        "thread '{thread_id}' is outside workspace cwd '{expected_cwd}' (actual: '{actual_cwd}')"
    )]
    ThreadOutsideWorkspace {
        thread_id: String,
        expected_cwd: String,
        actual_cwd: String,
    },
}

pub type Result<T> = std::result::Result<T, CodexIntegrationError>;
