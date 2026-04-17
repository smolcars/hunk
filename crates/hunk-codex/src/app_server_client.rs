#[path = "app_server_client_embedded.rs"]
mod app_server_client_embedded;

use std::time::Duration;

use crate::protocol::JSONRPCErrorError;
use crate::protocol::RequestId;
use crate::protocol::ServerNotification;
use crate::protocol::ServerRequest;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::errors::Result;

pub use app_server_client_embedded::EmbeddedAppServerClient;
pub use app_server_client_embedded::EmbeddedAppServerClientStartArgs;

pub const DEFAULT_APP_SERVER_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppServerTransportKind {
    Embedded,
}

impl AppServerTransportKind {
    pub fn status_label(self) -> &'static str {
        match self {
            Self::Embedded => "embedded Codex App Server",
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppServerEvent {
    Lagged { skipped: usize },
    ServerNotification(ServerNotification),
    ServerRequest(ServerRequest),
    Disconnected { message: String },
}

pub trait AppServerClient {
    fn request_typed<P, R>(
        &mut self,
        method: &str,
        params: Option<&P>,
        timeout: Duration,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned;

    fn notify<P>(&mut self, method: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize;

    fn next_event(&mut self, timeout: Duration) -> Result<Option<AppServerEvent>>;

    fn drain_buffered_notifications(
        &mut self,
        timeout: Duration,
    ) -> Result<Vec<ServerNotification>> {
        let _ = timeout;
        Ok(Vec::new())
    }

    fn respond_typed<T>(&mut self, request_id: RequestId, result: &T) -> Result<()>
    where
        T: Serialize;

    fn reject_server_request(
        &mut self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> Result<()>;

    fn shutdown(&mut self) -> Result<()>;
}
