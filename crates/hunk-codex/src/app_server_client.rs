#[path = "app_server_client_embedded.rs"]
mod app_server_client_embedded;
#[path = "app_server_client_remote.rs"]
mod app_server_client_remote;

use std::time::Duration;

use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::errors::Result;
use crate::ws_client::JsonRpcSession;

pub use app_server_client_embedded::EmbeddedAppServerClient;
pub use app_server_client_embedded::EmbeddedAppServerClientStartArgs;
pub use app_server_client_remote::RemoteAppServerClient;

pub const DEFAULT_APP_SERVER_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppServerTransportKind {
    RemoteBundled,
    Embedded,
}

impl AppServerTransportKind {
    pub fn status_label(self) -> &'static str {
        match self {
            Self::RemoteBundled => "remote bundled Codex App Server",
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

pub enum ManagedAppServerClient {
    Remote(RemoteAppServerClient),
    Embedded(EmbeddedAppServerClient),
}

impl ManagedAppServerClient {
    pub fn transport_kind(&self) -> AppServerTransportKind {
        match self {
            Self::Remote(_) => AppServerTransportKind::RemoteBundled,
            Self::Embedded(_) => AppServerTransportKind::Embedded,
        }
    }
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

impl AppServerClient for ManagedAppServerClient {
    fn request_typed<P, R>(
        &mut self,
        method: &str,
        params: Option<&P>,
        timeout: Duration,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        match self {
            Self::Remote(client) => client.request_typed(method, params, timeout),
            Self::Embedded(client) => client.request_typed(method, params, timeout),
        }
    }

    fn notify<P>(&mut self, method: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        match self {
            Self::Remote(client) => client.notify(method, params),
            Self::Embedded(client) => client.notify(method, params),
        }
    }

    fn next_event(&mut self, timeout: Duration) -> Result<Option<AppServerEvent>> {
        match self {
            Self::Remote(client) => client.next_event(timeout),
            Self::Embedded(client) => client.next_event(timeout),
        }
    }

    fn respond_typed<T>(&mut self, request_id: RequestId, result: &T) -> Result<()>
    where
        T: Serialize,
    {
        match self {
            Self::Remote(client) => client.respond_typed(request_id, result),
            Self::Embedded(client) => client.respond_typed(request_id, result),
        }
    }

    fn reject_server_request(
        &mut self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> Result<()> {
        match self {
            Self::Remote(client) => client.reject_server_request(request_id, error),
            Self::Embedded(client) => client.reject_server_request(request_id, error),
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        match self {
            Self::Remote(client) => client.shutdown(),
            Self::Embedded(client) => client.shutdown(),
        }
    }
}

impl AppServerClient for JsonRpcSession {
    fn request_typed<P, R>(
        &mut self,
        method: &str,
        params: Option<&P>,
        timeout: Duration,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        JsonRpcSession::request_typed(self, method, params, timeout)
    }

    fn notify<P>(&mut self, method: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        let params = params
            .map(serde_json::to_value)
            .transpose()
            .map_err(crate::errors::CodexIntegrationError::Serialization)?;
        JsonRpcSession::notify(self, method, params)
    }

    fn next_event(&mut self, timeout: Duration) -> Result<Option<AppServerEvent>> {
        if self.poll_server_notifications(timeout)? == 0 {
            return Ok(None);
        }
        if let Some(request) = self.drain_server_requests().into_iter().next() {
            return Ok(Some(AppServerEvent::ServerRequest(request)));
        }
        if let Some(notification) = self.drain_server_notifications().into_iter().next() {
            return Ok(Some(AppServerEvent::ServerNotification(notification)));
        }
        Ok(None)
    }

    fn respond_typed<T>(&mut self, request_id: RequestId, result: &T) -> Result<()>
    where
        T: Serialize,
    {
        JsonRpcSession::respond_typed(self, request_id, result)
    }

    fn reject_server_request(
        &mut self,
        _request_id: RequestId,
        _error: JSONRPCErrorError,
    ) -> Result<()> {
        Err(crate::errors::CodexIntegrationError::WebSocketTransport(
            "rejecting server requests is not supported by the legacy JsonRpcSession adapter"
                .to_string(),
        ))
    }

    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }

    fn drain_buffered_notifications(
        &mut self,
        timeout: Duration,
    ) -> Result<Vec<ServerNotification>> {
        let _ = timeout;
        Ok(self.drain_server_notifications())
    }
}

pub(crate) fn server_notification_requires_delivery(notification: &ServerNotification) -> bool {
    matches!(
        notification,
        ServerNotification::TurnCompleted(_)
            | ServerNotification::ItemCompleted(_)
            | ServerNotification::AgentMessageDelta(_)
            | ServerNotification::PlanDelta(_)
            | ServerNotification::ReasoningSummaryTextDelta(_)
            | ServerNotification::ReasoningTextDelta(_)
    )
}
