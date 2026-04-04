use std::future::Future;
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
use fastwebsockets::FragmentCollector;
use fastwebsockets::Frame;
use fastwebsockets::OpCode;
use fastwebsockets::Payload;
use fastwebsockets::handshake;
use http_body_util::Empty;
use hyper::Request;
use hyper::body::Bytes;
use hyper::header::CONNECTION;
use hyper::header::HOST;
use hyper::header::UPGRADE;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::runtime::Builder;
use tokio::runtime::Handle;
use tokio::runtime::Runtime;
use url::Url;

use crate::api;
use crate::api::InitializeOptions;
use crate::errors::CodexIntegrationError;
use crate::errors::Result;
use crate::rpc::RequestIdGenerator;

type CodexWebSocket = FragmentCollector<TokioIo<Upgraded>>;

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

    fn as_http_url(&self) -> Result<Url> {
        let raw = format!("http://{}:{}/", self.host, self.port);
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

pub struct JsonRpcSession {
    runtime: Runtime,
    socket: CodexWebSocket,
    request_ids: RequestIdGenerator,
    retry_policy: RequestRetryPolicy,
    server_notifications: Vec<ServerNotification>,
    server_requests: Vec<ServerRequest>,
}

const CODEX_WS_MAX_MESSAGE_SIZE_BYTES: usize = 64 << 20;

impl JsonRpcSession {
    pub fn connect(endpoint: &WebSocketEndpoint) -> Result<Self> {
        let runtime = Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(|error| {
                CodexIntegrationError::WebSocketTransport(format!(
                    "failed to build websocket runtime: {error}"
                ))
            })?;

        let socket = runtime.block_on(connect_fastwebsocket(endpoint, runtime.handle().clone()))?;

        Ok(Self {
            runtime,
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
        let payload = serde_json::to_vec(&message).map_err(CodexIntegrationError::Serialization)?;
        let frame = Frame::text(Payload::Owned(payload));
        self.runtime
            .block_on(self.socket.write_frame(frame))
            .map_err(fastwebsocket_transport_error)
    }

    fn read_message(&mut self, timeout: Duration) -> Result<JSONRPCMessage> {
        loop {
            let frame = self
                .runtime
                .block_on(async { tokio::time::timeout(timeout, self.socket.read_frame()).await })
                .map_err(|_| CodexIntegrationError::RequestTimedOut {
                    method: "read".to_string(),
                    timeout_ms: duration_ms(timeout),
                })?
                .map_err(fastwebsocket_transport_error)?;

            if frame.payload.len() > CODEX_WS_MAX_MESSAGE_SIZE_BYTES {
                return Err(CodexIntegrationError::WebSocketTransport(format!(
                    "message exceeded {} bytes",
                    CODEX_WS_MAX_MESSAGE_SIZE_BYTES
                )));
            }

            match frame.opcode {
                OpCode::Text => {
                    let message: JSONRPCMessage = serde_json::from_slice(frame.payload.as_ref())
                        .map_err(CodexIntegrationError::Serialization)?;
                    return Ok(message);
                }
                OpCode::Binary => {
                    let message: JSONRPCMessage = serde_json::from_slice(frame.payload.as_ref())
                        .map_err(CodexIntegrationError::Serialization)?;
                    return Ok(message);
                }
                OpCode::Close => {
                    let close_text = String::from_utf8_lossy(frame.payload.as_ref()).into_owned();
                    let close_text = if close_text.is_empty() {
                        "closed without reason".to_string()
                    } else {
                        close_text
                    };
                    return Err(CodexIntegrationError::WebSocketTransport(format!(
                        "socket closed: {close_text}"
                    )));
                }
                OpCode::Ping | OpCode::Pong | OpCode::Continuation => {}
            }
        }
    }
}

async fn connect_fastwebsocket(
    endpoint: &WebSocketEndpoint,
    handle: Handle,
) -> Result<CodexWebSocket> {
    let address = format!("{}:{}", endpoint.host(), endpoint.port());
    let stream = TcpStream::connect(address)
        .await
        .map_err(CodexIntegrationError::HostProcessIo)?;
    let request = websocket_handshake_request(endpoint)?;
    let executor = TokioSpawnExecutor { handle };
    let (mut socket, _) = handshake::client(&executor, request, stream)
        .await
        .map_err(fastwebsocket_transport_error)?;
    socket.set_auto_close(true);
    socket.set_auto_pong(true);
    socket.set_writev(true);
    Ok(FragmentCollector::new(socket))
}

fn websocket_handshake_request(endpoint: &WebSocketEndpoint) -> Result<Request<Empty<Bytes>>> {
    let url = endpoint.as_http_url()?;
    Request::builder()
        .method("GET")
        .uri(url.as_str())
        .header(HOST, format!("{}:{}", endpoint.host(), endpoint.port()))
        .header(UPGRADE, "websocket")
        .header(CONNECTION, "upgrade")
        .header("Sec-WebSocket-Key", handshake::generate_key())
        .header("Sec-WebSocket-Version", "13")
        .body(Empty::<Bytes>::new())
        .map_err(|error| {
            CodexIntegrationError::InvalidEndpoint(format!("{} ({error})", url.as_str()))
        })
}

#[derive(Clone)]
struct TokioSpawnExecutor {
    handle: Handle,
}

impl<Fut> hyper::rt::Executor<Fut> for TokioSpawnExecutor
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    fn execute(&self, fut: Fut) {
        self.handle.spawn(fut);
    }
}

fn fastwebsocket_transport_error(error: impl std::fmt::Display) -> CodexIntegrationError {
    CodexIntegrationError::WebSocketTransport(error.to_string())
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

#[cfg(test)]
mod tests {
    use super::{CODEX_WS_MAX_MESSAGE_SIZE_BYTES, WebSocketEndpoint, websocket_handshake_request};

    #[test]
    fn websocket_handshake_request_uses_upgrade_headers() {
        let endpoint = WebSocketEndpoint::loopback(4321);
        let request = websocket_handshake_request(&endpoint).expect("request");
        assert_eq!(request.method(), hyper::Method::GET);
        assert_eq!(request.uri().to_string(), "http://127.0.0.1:4321/");
        assert_eq!(
            request.headers().get(hyper::header::HOST).unwrap(),
            "127.0.0.1:4321"
        );
        assert_eq!(
            request.headers().get(hyper::header::UPGRADE).unwrap(),
            "websocket"
        );
        assert_eq!(
            request.headers().get(hyper::header::CONNECTION).unwrap(),
            "upgrade"
        );
        assert_eq!(
            request.headers().get("Sec-WebSocket-Version").unwrap(),
            "13"
        );
        assert!(request.headers().contains_key("Sec-WebSocket-Key"));
    }

    #[test]
    fn websocket_max_message_size_matches_expected_limit() {
        assert_eq!(CODEX_WS_MAX_MESSAGE_SIZE_BYTES, 64 << 20);
    }
}
