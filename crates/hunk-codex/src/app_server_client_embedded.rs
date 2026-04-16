use std::path::PathBuf;
use std::time::Duration;

use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::app_server_client::AppServerClient;
use crate::app_server_client::AppServerEvent;
use crate::errors::CodexIntegrationError;
use crate::errors::Result;

#[derive(Debug, Clone)]
pub struct EmbeddedAppServerClientStartArgs {
    pub codex_home: PathBuf,
    pub fallback_cwd: PathBuf,
    pub codex_executable: PathBuf,
    pub client_name: String,
    pub client_version: String,
}

impl EmbeddedAppServerClientStartArgs {
    pub fn new(
        codex_home: PathBuf,
        fallback_cwd: PathBuf,
        codex_executable: PathBuf,
        client_name: String,
        client_version: String,
    ) -> Self {
        Self {
            codex_home,
            fallback_cwd,
            codex_executable,
            client_name,
            client_version,
        }
    }
}

pub struct EmbeddedAppServerClient;

impl EmbeddedAppServerClient {
    pub fn is_supported() -> bool {
        false
    }

    pub fn unavailable_reason() -> &'static str {
        "embedded Codex App Server support is not compiled into the desktop workspace yet because \
         the upstream in-process stack links sqlite through sqlx while Hunk already links \
         rusqlite; isolate that dependency graph in a separate binary or crate before enabling it"
    }

    pub fn start(args: EmbeddedAppServerClientStartArgs) -> Result<Self> {
        let _ = args;
        Err(unavailable_error())
    }
}

impl AppServerClient for EmbeddedAppServerClient {
    fn request_typed<P, R>(
        &mut self,
        _method: &str,
        _params: Option<&P>,
        _timeout: Duration,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        Err(unavailable_error())
    }

    fn notify<P>(&mut self, _method: &str, _params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        Err(unavailable_error())
    }

    fn next_event(&mut self, _timeout: Duration) -> Result<Option<AppServerEvent>> {
        Err(unavailable_error())
    }

    fn respond_typed<T>(&mut self, _request_id: RequestId, _result: &T) -> Result<()>
    where
        T: Serialize,
    {
        Err(unavailable_error())
    }

    fn reject_server_request(
        &mut self,
        _request_id: RequestId,
        _error: JSONRPCErrorError,
    ) -> Result<()> {
        Err(unavailable_error())
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

fn unavailable_error() -> CodexIntegrationError {
    CodexIntegrationError::WebSocketTransport(
        EmbeddedAppServerClient::unavailable_reason().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::EmbeddedAppServerClient;

    #[test]
    fn embedded_transport_is_reported_unavailable() {
        assert!(!EmbeddedAppServerClient::is_supported());
        assert!(EmbeddedAppServerClient::unavailable_reason().contains("sqlite"));
    }
}
