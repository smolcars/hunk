use std::io::ErrorKind;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::stream::MaybeTlsStream;
use url::Url;

use crate::api;
use crate::api::InitializeOptions;
use crate::errors::CodexIntegrationError;
use crate::errors::Result;
use crate::rpc::RequestIdGenerator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketEndpoint {
    host: String,
    port: u16,
}

impl WebSocketEndpoint {
    pub fn loopback(port: u16) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn as_url(&self) -> Result<Url> {
        let raw = format!("ws://{}:{}/", self.host, self.port);
        Url::parse(&raw)
            .map_err(|error| CodexIntegrationError::InvalidEndpoint(format!("{raw} ({error})")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestRetryPolicy {
    pub max_overload_retries: u8,
    pub initial_backoff: Duration,
}

impl Default for RequestRetryPolicy {
    fn default() -> Self {
        Self {
            max_overload_retries: 3,
            initial_backoff: Duration::from_millis(200),
        }
    }
}

#[derive(Debug)]
pub struct JsonRpcSession {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    request_ids: RequestIdGenerator,
    retry_policy: RequestRetryPolicy,
    server_notifications: Vec<ServerNotification>,
    server_requests: Vec<ServerRequest>,
}

impl JsonRpcSession {
    pub fn connect(endpoint: &WebSocketEndpoint) -> Result<Self> {
        let url = endpoint.as_url()?.to_string();
        let (socket, _) = tungstenite::connect(url.as_str())
            .map_err(|error| CodexIntegrationError::WebSocketTransport(error.to_string()))?;

        Ok(Self {
            socket,
            request_ids: RequestIdGenerator::default(),
            retry_policy: RequestRetryPolicy::default(),
            server_notifications: Vec::new(),
            server_requests: Vec::new(),
        })
    }

    pub fn with_retry_policy(mut self, retry_policy: RequestRetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn initialize(
        &mut self,
        options: InitializeOptions,
        timeout: Duration,
    ) -> Result<InitializeResponse> {
        let params = serde_json::to_value(options.into_params())
            .map_err(CodexIntegrationError::Serialization)?;

        let response_value = self.request(api::method::INITIALIZE, Some(params), timeout)?;
        // The app-server now reports its resolved `$CODEX_HOME` in the initialize
        // response. Hunk currently keeps using its own configured `codex_home`
        // for host launch and rollout lookup, but we still deserialize the field
        // here to stay compatible with the upstream protocol schema.
        let response: InitializeResponse =
            serde_json::from_value(response_value).map_err(CodexIntegrationError::Serialization)?;

        self.notify(api::method::INITIALIZED, None)?;
        Ok(response)
    }

    pub fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value> {
        for attempt in 0..=self.retry_policy.max_overload_retries {
            match self.request_once(method, params.clone(), timeout) {
                Ok(value) => return Ok(value),
                Err(CodexIntegrationError::JsonRpcServerError { code, .. })
                    if code == -32001 && attempt < self.retry_policy.max_overload_retries =>
                {
                    thread::sleep(backoff_for_attempt(
                        self.retry_policy.initial_backoff,
                        attempt,
                    ));
                }
                Err(error) => return Err(error),
            }
        }

        Err(CodexIntegrationError::JsonRpcServerError {
            code: -32001,
            message: "server overloaded after retries".to_string(),
        })
    }

    pub fn request_typed<P, R>(
        &mut self,
        method: &str,
        params: Option<&P>,
        timeout: Duration,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let params = params
            .map(serde_json::to_value)
            .transpose()
            .map_err(CodexIntegrationError::Serialization)?;
        let value = self.request(method, params, timeout)?;
        serde_json::from_value(value).map_err(CodexIntegrationError::Serialization)
    }

    pub fn drain_server_notifications(&mut self) -> Vec<ServerNotification> {
        std::mem::take(&mut self.server_notifications)
    }

    pub fn drain_server_requests(&mut self) -> Vec<ServerRequest> {
        std::mem::take(&mut self.server_requests)
    }

    pub fn poll_server_notifications(&mut self, timeout: Duration) -> Result<usize> {
        let message = match self.read_message(timeout) {
            Ok(message) => message,
            Err(error) if is_read_timeout(&error) => return Ok(0),
            Err(error) => return Err(error),
        };

        Ok(self.capture_incoming_message(message))
    }

    pub fn notify(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let message = JSONRPCMessage::Notification(JSONRPCNotification {
            method: method.to_string(),
            params,
        });
        self.send_message(message)
    }

    pub fn respond_typed<T>(&mut self, request_id: RequestId, result: &T) -> Result<()>
    where
        T: Serialize,
    {
        let result = serde_json::to_value(result).map_err(CodexIntegrationError::Serialization)?;
        self.send_message(JSONRPCMessage::Response(JSONRPCResponse {
            id: request_id,
            result,
        }))
    }

    fn request_once(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value> {
        let request_id = self.request_ids.next_request_id();
        let request = JSONRPCMessage::Request(JSONRPCRequest {
            id: request_id.clone(),
            method: method.to_string(),
            params,
            trace: None,
        });
        self.send_message(request)?;

        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(CodexIntegrationError::RequestTimedOut {
                    method: method.to_string(),
                    timeout_ms: duration_ms(timeout),
                });
            }

            let remaining = deadline.saturating_duration_since(now);
            let message = match self.read_message(remaining) {
                Ok(message) => message,
                Err(CodexIntegrationError::RequestTimedOut { .. }) => {
                    return Err(CodexIntegrationError::RequestTimedOut {
                        method: method.to_string(),
                        timeout_ms: duration_ms(timeout),
                    });
                }
                Err(error) => return Err(error),
            };
            match message {
                JSONRPCMessage::Response(response) => {
                    if response.id == request_id {
                        return Ok(response.result);
                    }
                }
                JSONRPCMessage::Error(error) => {
                    if error.id == request_id {
                        return Err(CodexIntegrationError::JsonRpcServerError {
                            code: error.error.code,
                            message: error.error.message,
                        });
                    }
                }
                JSONRPCMessage::Request(request) => {
                    self.capture_server_request(request);
                }
                JSONRPCMessage::Notification(notification) => {
                    self.capture_server_notification(notification);
                }
            }
        }
    }

    fn capture_server_notification(&mut self, notification: JSONRPCNotification) {
        if let Ok(notification) = ServerNotification::try_from(notification) {
            self.server_notifications.push(notification);
        }
    }

    fn capture_server_request(&mut self, request: JSONRPCRequest) {
        if let Ok(request) = ServerRequest::try_from(request) {
            self.server_requests.push(request);
        }
    }

    fn capture_incoming_message(&mut self, message: JSONRPCMessage) -> usize {
        match message {
            JSONRPCMessage::Notification(notification) => {
                self.capture_server_notification(notification);
                1
            }
            JSONRPCMessage::Request(request) => {
                self.capture_server_request(request);
                1
            }
            JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => 0,
        }
    }

    fn send_message(&mut self, message: JSONRPCMessage) -> Result<()> {
        let payload =
            serde_json::to_string(&message).map_err(CodexIntegrationError::Serialization)?;
        self.socket
            .send(Message::Text(payload.into()))
            .map_err(|error| CodexIntegrationError::WebSocketTransport(error.to_string()))
    }

    fn read_message(&mut self, timeout: Duration) -> Result<JSONRPCMessage> {
        self.set_read_timeout(Some(timeout))?;

        loop {
            let frame = match self.socket.read() {
                Ok(frame) => frame,
                Err(tungstenite::Error::Io(error))
                    if error.kind() == ErrorKind::WouldBlock
                        || error.kind() == ErrorKind::TimedOut =>
                {
                    return Err(CodexIntegrationError::RequestTimedOut {
                        method: "read".to_string(),
                        timeout_ms: duration_ms(timeout),
                    });
                }
                Err(error) => {
                    return Err(CodexIntegrationError::WebSocketTransport(error.to_string()));
                }
            };

            match frame {
                Message::Text(text) => {
                    let message: JSONRPCMessage = serde_json::from_str(text.as_ref())
                        .map_err(CodexIntegrationError::Serialization)?;
                    return Ok(message);
                }
                Message::Binary(binary) => {
                    let message: JSONRPCMessage = serde_json::from_slice(binary.as_ref())
                        .map_err(CodexIntegrationError::Serialization)?;
                    return Ok(message);
                }
                Message::Ping(payload) => {
                    self.socket.send(Message::Pong(payload)).map_err(|error| {
                        CodexIntegrationError::WebSocketTransport(error.to_string())
                    })?;
                }
                Message::Pong(_) | Message::Frame(_) => {}
                Message::Close(frame) => {
                    let close_text = frame
                        .map(|value| value.reason.to_string())
                        .unwrap_or_else(|| "closed without reason".to_string());
                    return Err(CodexIntegrationError::WebSocketTransport(format!(
                        "socket closed: {close_text}"
                    )));
                }
            }
        }
    }

    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> Result<()> {
        if let MaybeTlsStream::Plain(stream) = self.socket.get_mut() {
            stream
                .set_read_timeout(timeout)
                .map_err(CodexIntegrationError::HostProcessIo)?;
        }
        Ok(())
    }
}

fn backoff_for_attempt(base: Duration, attempt: u8) -> Duration {
    let capped_shift = u32::from(attempt).min(20);
    let multiplier = 1u128 << capped_shift;
    let base_ms = base.as_millis();
    let delay_ms = base_ms.saturating_mul(multiplier).min(u128::from(u64::MAX)) as u64;
    Duration::from_millis(delay_ms)
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn is_read_timeout(error: &CodexIntegrationError) -> bool {
    matches!(
        error,
        CodexIntegrationError::RequestTimedOut { method, .. } if method == "read"
    )
}
