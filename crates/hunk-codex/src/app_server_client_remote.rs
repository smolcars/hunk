use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::Duration;

use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
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
use tokio::runtime::Builder;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::warn;

use crate::api;
use crate::api::InitializeOptions;
use crate::app_server_client::AppServerClient;
use crate::app_server_client::AppServerEvent;
use crate::app_server_client::DEFAULT_APP_SERVER_CHANNEL_CAPACITY;
use crate::app_server_client::server_notification_requires_delivery;
use crate::errors::CodexIntegrationError;
use crate::errors::Result;
use crate::rpc::RequestIdGenerator;
use crate::ws_client::WebSocketEndpoint;

const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

enum RemoteClientCommand {
    Request {
        request: JSONRPCRequest,
        method: String,
        response_tx: oneshot::Sender<Result<Value>>,
    },
    Notify {
        notification: JSONRPCNotification,
        response_tx: oneshot::Sender<Result<()>>,
    },
    Respond {
        response: JSONRPCResponse,
        response_tx: oneshot::Sender<Result<()>>,
    },
    Reject {
        error: JSONRPCError,
        response_tx: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        response_tx: oneshot::Sender<Result<()>>,
    },
}

struct PendingRequest {
    method: String,
    response_tx: oneshot::Sender<Result<Value>>,
}

pub struct RemoteAppServerClient {
    runtime: Runtime,
    command_tx: mpsc::Sender<RemoteClientCommand>,
    event_rx: mpsc::Receiver<AppServerEvent>,
    pending_events: VecDeque<AppServerEvent>,
    request_ids: RequestIdGenerator,
    worker_handle: Option<JoinHandle<()>>,
}

impl RemoteAppServerClient {
    pub fn connect_loopback(port: u16, timeout: Duration) -> Result<Self> {
        let endpoint = WebSocketEndpoint::loopback(port);
        Self::connect(&endpoint, timeout)
    }

    pub fn connect(endpoint: &WebSocketEndpoint, timeout: Duration) -> Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_io()
            .enable_time()
            .build()
            .map_err(|error| {
                CodexIntegrationError::WebSocketTransport(format!(
                    "failed to build app-server runtime: {error}"
                ))
            })?;

        let websocket_url = endpoint.as_url()?.to_string();
        let channel_capacity = DEFAULT_APP_SERVER_CHANNEL_CAPACITY.max(1);
        let (command_tx, command_rx) = mpsc::channel(channel_capacity);
        let (event_tx, event_rx) = mpsc::channel(channel_capacity);

        let (init_tx, init_rx) = std::sync::mpsc::channel();
        let worker_handle = runtime.spawn(async move {
            remote_worker(websocket_url, timeout, command_rx, event_tx, init_tx).await;
        });

        let pending_events = init_rx.recv().map_err(|_| {
            CodexIntegrationError::WebSocketTransport(
                "remote app-server worker exited before initialization".to_string(),
            )
        })??;

        Ok(Self {
            runtime,
            command_tx,
            event_rx,
            pending_events: pending_events.into(),
            request_ids: RequestIdGenerator::default(),
            worker_handle: Some(worker_handle),
        })
    }

    fn send_command<T>(
        &mut self,
        command: RemoteClientCommand,
        response_rx: oneshot::Receiver<Result<T>>,
        timeout_duration: Duration,
    ) -> Result<T> {
        self.command_tx.blocking_send(command).map_err(|_| {
            CodexIntegrationError::WebSocketTransport(
                "remote app-server worker channel is closed".to_string(),
            )
        })?;

        self.runtime
            .block_on(async { timeout(timeout_duration, response_rx).await })
            .map_err(|_| CodexIntegrationError::RequestTimedOut {
                method: "app-server command".to_string(),
                timeout_ms: timeout_duration.as_millis().min(u128::from(u64::MAX)) as u64,
            })?
            .map_err(|_| {
                CodexIntegrationError::WebSocketTransport(
                    "remote app-server response channel is closed".to_string(),
                )
            })?
    }
}

impl AppServerClient for RemoteAppServerClient {
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
        let params = params
            .map(serde_json::to_value)
            .transpose()
            .map_err(CodexIntegrationError::Serialization)?;
        let request_id = self.request_ids.next_request_id();
        let request = JSONRPCRequest {
            id: request_id,
            method: method.to_string(),
            params,
            trace: None,
        };
        let method_name = method.to_string();
        let (response_tx, response_rx) = oneshot::channel();
        let value = self.send_command(
            RemoteClientCommand::Request {
                request,
                method: method_name,
                response_tx,
            },
            response_rx,
            timeout,
        )?;
        serde_json::from_value(value).map_err(CodexIntegrationError::Serialization)
    }

    fn notify<P>(&mut self, method: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        let params = params
            .map(serde_json::to_value)
            .transpose()
            .map_err(CodexIntegrationError::Serialization)?;
        let notification = JSONRPCNotification {
            method: method.to_string(),
            params,
        };
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(
            RemoteClientCommand::Notify {
                notification,
                response_tx,
            },
            response_rx,
            Duration::from_secs(5),
        )
    }

    fn next_event(&mut self, timeout_duration: Duration) -> Result<Option<AppServerEvent>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(Some(event));
        }

        let recv = self.event_rx.recv();
        match self
            .runtime
            .block_on(async { timeout(timeout_duration, recv).await })
        {
            Ok(event) => Ok(event),
            Err(_) => Ok(None),
        }
    }

    fn respond_typed<T>(&mut self, request_id: RequestId, result: &T) -> Result<()>
    where
        T: Serialize,
    {
        let result = serde_json::to_value(result).map_err(CodexIntegrationError::Serialization)?;
        let response = JSONRPCResponse {
            id: request_id,
            result,
        };
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(
            RemoteClientCommand::Respond {
                response,
                response_tx,
            },
            response_rx,
            Duration::from_secs(5),
        )
    }

    fn reject_server_request(
        &mut self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> Result<()> {
        let error = JSONRPCError {
            error,
            id: request_id,
        };
        let (response_tx, response_rx) = oneshot::channel();
        self.send_command(
            RemoteClientCommand::Reject { error, response_tx },
            response_rx,
            Duration::from_secs(5),
        )
    }

    fn shutdown(&mut self) -> Result<()> {
        let Some(worker_handle) = self.worker_handle.as_mut() else {
            return Ok(());
        };

        let (response_tx, response_rx) = oneshot::channel();
        let _ = self
            .command_tx
            .blocking_send(RemoteClientCommand::Shutdown { response_tx });
        let _ = self
            .runtime
            .block_on(async { timeout(WORKER_SHUTDOWN_TIMEOUT, response_rx).await });
        let _ = self
            .runtime
            .block_on(async { timeout(WORKER_SHUTDOWN_TIMEOUT, worker_handle).await });
        self.worker_handle.take();
        Ok(())
    }
}

impl Drop for RemoteAppServerClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
        if let Some(handle) = self.worker_handle.take() {
            handle.abort();
        }
    }
}

async fn remote_worker(
    websocket_url: String,
    initialize_timeout: Duration,
    mut command_rx: mpsc::Receiver<RemoteClientCommand>,
    event_tx: mpsc::Sender<AppServerEvent>,
    init_tx: std::sync::mpsc::Sender<Result<Vec<AppServerEvent>>>,
) {
    let connect_result = connect_async(websocket_url.as_str())
        .await
        .map_err(|error| {
            CodexIntegrationError::WebSocketTransport(format!(
                "failed to connect to remote app server at `{websocket_url}`: {error}"
            ))
        });

    let (mut stream, _response) = match connect_result {
        Ok(stream) => stream,
        Err(error) => {
            let _ = init_tx.send(Err(error));
            return;
        }
    };

    match initialize_remote_connection(&mut stream, websocket_url.as_str(), initialize_timeout)
        .await
    {
        Ok(pending_events) => {
            if init_tx.send(Ok(pending_events)).is_err() {
                let _ = stream.close(None).await;
                return;
            }
        }
        Err(error) => {
            let _ = init_tx.send(Err(error));
            let _ = stream.close(None).await;
            return;
        }
    }

    let mut pending_requests = HashMap::<RequestId, PendingRequest>::new();
    let mut skipped_events = 0usize;

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    let _ = stream.close(None).await;
                    break;
                };
                match command {
                    RemoteClientCommand::Request { request, method, response_tx } => {
                        let request_id = request.id.clone();
                        if pending_requests.contains_key(&request_id) {
                            let _ = response_tx.send(Err(CodexIntegrationError::WebSocketTransport(
                                format!("duplicate remote request id `{request_id}`"),
                            )));
                            continue;
                        }
                        pending_requests.insert(request_id.clone(), PendingRequest { method, response_tx });
                        if let Err(error) = write_jsonrpc_message(
                            &mut stream,
                            JSONRPCMessage::Request(request),
                            websocket_url.as_str(),
                        ).await {
                            let message = error.to_string();
                            if let Some(pending) = pending_requests.remove(&request_id) {
                                let _ = pending.response_tx.send(Err(error));
                            }
                            let _ = deliver_event(
                                &event_tx,
                                &mut skipped_events,
                                AppServerEvent::Disconnected { message },
                                &mut stream,
                            ).await;
                            break;
                        }
                    }
                    RemoteClientCommand::Notify { notification, response_tx } => {
                        let result = write_jsonrpc_message(
                            &mut stream,
                            JSONRPCMessage::Notification(notification),
                            websocket_url.as_str(),
                        ).await;
                        let _ = response_tx.send(result);
                    }
                    RemoteClientCommand::Respond { response, response_tx } => {
                        let result = write_jsonrpc_message(
                            &mut stream,
                            JSONRPCMessage::Response(response),
                            websocket_url.as_str(),
                        ).await;
                        let _ = response_tx.send(result);
                    }
                    RemoteClientCommand::Reject { error, response_tx } => {
                        let result = write_jsonrpc_message(
                            &mut stream,
                            JSONRPCMessage::Error(error),
                            websocket_url.as_str(),
                        ).await;
                        let _ = response_tx.send(result);
                    }
                    RemoteClientCommand::Shutdown { response_tx } => {
                        let result = stream.close(None).await.map_err(|error| {
                            CodexIntegrationError::WebSocketTransport(format!(
                                "failed to close remote app server `{websocket_url}`: {error}"
                            ))
                        });
                        let _ = response_tx.send(result);
                        break;
                    }
                }
            }
            message = stream.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<JSONRPCMessage>(&text) {
                            Ok(JSONRPCMessage::Response(response)) => {
                                if let Some(pending) = pending_requests.remove(&response.id) {
                                    let _ = pending.response_tx.send(Ok(response.result));
                                }
                            }
                            Ok(JSONRPCMessage::Error(error)) => {
                                if let Some(pending) = pending_requests.remove(&error.id) {
                                    let _ = pending.response_tx.send(Err(CodexIntegrationError::JsonRpcServerError {
                                        code: error.error.code,
                                        message: if error.error.message.is_empty() {
                                            pending.method
                                        } else {
                                            error.error.message
                                        },
                                    }));
                                }
                            }
                            Ok(JSONRPCMessage::Notification(notification)) => {
                                if let Some(event) = app_server_event_from_notification(notification)
                                    && let Err(error) = deliver_event(&event_tx, &mut skipped_events, event, &mut stream).await
                                {
                                    warn!(%error, "failed to deliver remote app-server notification");
                                    break;
                                }
                            }
                            Ok(JSONRPCMessage::Request(request)) => {
                                let request_id = request.id.clone();
                                let method = request.method.clone();
                                match ServerRequest::try_from(request) {
                                    Ok(request) => {
                                        if let Err(error) = deliver_event(
                                            &event_tx,
                                            &mut skipped_events,
                                            AppServerEvent::ServerRequest(request),
                                            &mut stream,
                                        ).await {
                                            warn!(%error, "failed to deliver remote app-server request");
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        warn!(%error, method, "rejecting unknown remote app-server request");
                                        if let Err(reject_error) = write_jsonrpc_message(
                                            &mut stream,
                                            JSONRPCMessage::Error(JSONRPCError {
                                                error: JSONRPCErrorError {
                                                    code: -32601,
                                                    message: format!("unsupported remote app-server request `{method}`"),
                                                    data: None,
                                                },
                                                id: request_id,
                                            }),
                                            websocket_url.as_str(),
                                        ).await {
                                            let _ = deliver_event(
                                                &event_tx,
                                                &mut skipped_events,
                                                AppServerEvent::Disconnected { message: reject_error.to_string() },
                                                &mut stream,
                                            ).await;
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = deliver_event(
                                    &event_tx,
                                    &mut skipped_events,
                                    AppServerEvent::Disconnected {
                                        message: format!(
                                            "remote app server sent invalid JSON-RPC: {error}"
                                        ),
                                    },
                                    &mut stream,
                                )
                                .await;
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(payload))) => {
                        match serde_json::from_slice::<JSONRPCMessage>(&payload) {
                            Ok(JSONRPCMessage::Notification(notification)) => {
                                if let Some(event) = app_server_event_from_notification(notification)
                                    && let Err(error) = deliver_event(&event_tx, &mut skipped_events, event, &mut stream).await
                                {
                                    warn!(%error, "failed to deliver binary remote app-server notification");
                                    break;
                                }
                            }
                            Ok(JSONRPCMessage::Request(request)) => {
                                let request_id = request.id.clone();
                                let method = request.method.clone();
                                match ServerRequest::try_from(request) {
                                    Ok(request) => {
                                        if let Err(error) = deliver_event(
                                            &event_tx,
                                            &mut skipped_events,
                                            AppServerEvent::ServerRequest(request),
                                            &mut stream,
                                        ).await {
                                            warn!(%error, "failed to deliver binary remote app-server request");
                                            break;
                                        }
                                    }
                                    Err(error) => {
                                        warn!(%error, method, "rejecting unknown remote app-server binary request");
                                        if let Err(reject_error) = write_jsonrpc_message(
                                            &mut stream,
                                            JSONRPCMessage::Error(JSONRPCError {
                                                error: JSONRPCErrorError {
                                                    code: -32601,
                                                    message: format!("unsupported remote app-server request `{method}`"),
                                                    data: None,
                                                },
                                                id: request_id,
                                            }),
                                            websocket_url.as_str(),
                                        ).await {
                                            let _ = deliver_event(
                                                &event_tx,
                                                &mut skipped_events,
                                                AppServerEvent::Disconnected { message: reject_error.to_string() },
                                                &mut stream,
                                            ).await;
                                            break;
                                        }
                                    }
                                }
                            }
                            Ok(JSONRPCMessage::Response(response)) => {
                                if let Some(pending) = pending_requests.remove(&response.id) {
                                    let _ = pending.response_tx.send(Ok(response.result));
                                }
                            }
                            Ok(JSONRPCMessage::Error(error)) => {
                                if let Some(pending) = pending_requests.remove(&error.id) {
                                    let _ = pending.response_tx.send(Err(CodexIntegrationError::JsonRpcServerError {
                                        code: error.error.code,
                                        message: if error.error.message.is_empty() {
                                            pending.method
                                        } else {
                                            error.error.message
                                        },
                                    }));
                                }
                            }
                            Err(error) => {
                                let _ = deliver_event(
                                    &event_tx,
                                    &mut skipped_events,
                                    AppServerEvent::Disconnected {
                                        message: format!(
                                            "remote app server sent invalid binary JSON-RPC: {error}"
                                        ),
                                    },
                                    &mut stream,
                                )
                                .await;
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let reason = frame
                            .as_ref()
                            .map(|frame| frame.reason.to_string())
                            .filter(|reason| !reason.is_empty())
                            .unwrap_or_else(|| "connection closed".to_string());
                        let _ = deliver_event(
                            &event_tx,
                            &mut skipped_events,
                            AppServerEvent::Disconnected {
                                message: format!("remote app server disconnected: {reason}"),
                            },
                            &mut stream,
                        ).await;
                        break;
                    }
                    Some(Ok(Message::Ping(_)))
                    | Some(Ok(Message::Pong(_)))
                    | Some(Ok(Message::Frame(_))) => {}
                    Some(Err(error)) => {
                        let _ = deliver_event(
                            &event_tx,
                            &mut skipped_events,
                            AppServerEvent::Disconnected {
                                message: format!("remote app server transport failed: {error}"),
                            },
                            &mut stream,
                        ).await;
                        break;
                    }
                    None => {
                        let _ = deliver_event(
                            &event_tx,
                            &mut skipped_events,
                            AppServerEvent::Disconnected {
                                message: "remote app server closed the connection".to_string(),
                            },
                            &mut stream,
                        ).await;
                        break;
                    }
                }
            }
        }
    }

    let closed_error = CodexIntegrationError::WebSocketTransport(
        "remote app-server worker channel is closed".to_string(),
    );
    for (_, pending) in pending_requests {
        let _ = pending
            .response_tx
            .send(Err(CodexIntegrationError::WebSocketTransport(
                closed_error.to_string(),
            )));
    }
}

async fn initialize_remote_connection<S>(
    stream: &mut tokio_tungstenite::WebSocketStream<S>,
    websocket_url: &str,
    initialize_timeout: Duration,
) -> Result<Vec<AppServerEvent>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    // Match the legacy session/request generator behavior for the initial handshake.
    // The bundled 0.120 runtime is known to initialize correctly on numeric ids.
    let request_id = RequestId::Integer(1);
    let request = JSONRPCRequest {
        id: request_id.clone(),
        method: api::method::INITIALIZE.to_string(),
        params: Some(
            serde_json::to_value(InitializeOptions::default().into_params())
                .map_err(CodexIntegrationError::Serialization)?,
        ),
        trace: None,
    };

    write_jsonrpc_message(stream, JSONRPCMessage::Request(request), websocket_url).await?;

    let mut pending_events = Vec::new();
    timeout(initialize_timeout, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    let message = serde_json::from_str::<JSONRPCMessage>(&text)
                        .map_err(CodexIntegrationError::Serialization)?;
                    match message {
                        JSONRPCMessage::Response(response) if response.id == request_id => break Ok(()),
                        JSONRPCMessage::Error(error) if error.id == request_id => {
                            break Err(CodexIntegrationError::JsonRpcServerError {
                                code: error.error.code,
                                message: error.error.message,
                            });
                        }
                        JSONRPCMessage::Notification(notification) => {
                            if let Some(event) = app_server_event_from_notification(notification) {
                                pending_events.push(event);
                            }
                        }
                        JSONRPCMessage::Request(request) => {
                            if let Ok(request) = ServerRequest::try_from(request) {
                                pending_events.push(AppServerEvent::ServerRequest(request));
                            }
                        }
                        JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => {}
                    }
                }
                Some(Ok(Message::Binary(payload))) => {
                    let message = serde_json::from_slice::<JSONRPCMessage>(&payload)
                        .map_err(CodexIntegrationError::Serialization)?;
                    match message {
                        JSONRPCMessage::Response(response) if response.id == request_id => break Ok(()),
                        JSONRPCMessage::Error(error) if error.id == request_id => {
                            break Err(CodexIntegrationError::JsonRpcServerError {
                                code: error.error.code,
                                message: error.error.message,
                            });
                        }
                        JSONRPCMessage::Notification(notification) => {
                            if let Some(event) = app_server_event_from_notification(notification) {
                                pending_events.push(event);
                            }
                        }
                        JSONRPCMessage::Request(request) => {
                            if let Ok(request) = ServerRequest::try_from(request) {
                                pending_events.push(AppServerEvent::ServerRequest(request));
                            }
                        }
                        JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => {}
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    let reason = frame
                        .as_ref()
                        .map(|frame| frame.reason.to_string())
                        .filter(|reason| !reason.is_empty())
                        .unwrap_or_else(|| "connection closed during initialize".to_string());
                    break Err(CodexIntegrationError::WebSocketTransport(format!(
                        "remote app server `{websocket_url}` closed during initialize: {reason}"
                    )));
                }
                Some(Ok(Message::Ping(_)))
                | Some(Ok(Message::Pong(_)))
                | Some(Ok(Message::Frame(_))) => {}
                Some(Err(error)) => {
                    break Err(CodexIntegrationError::WebSocketTransport(format!(
                        "remote app server `{websocket_url}` transport failed during initialize: {error}"
                    )));
                }
                None => {
                    break Err(CodexIntegrationError::WebSocketTransport(format!(
                        "remote app server `{websocket_url}` closed during initialize"
                    )));
                }
            }
        }
    })
    .await
    .map_err(|_| CodexIntegrationError::RequestTimedOut {
        method: api::method::INITIALIZE.to_string(),
        timeout_ms: initialize_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
    })??;

    write_jsonrpc_message(
        stream,
        JSONRPCMessage::Notification(JSONRPCNotification {
            method: api::method::INITIALIZED.to_string(),
            params: None,
        }),
        websocket_url,
    )
    .await?;

    Ok(pending_events)
}

fn app_server_event_from_notification(notification: JSONRPCNotification) -> Option<AppServerEvent> {
    match ServerNotification::try_from(notification) {
        Ok(notification) => Some(AppServerEvent::ServerNotification(notification)),
        Err(_) => None,
    }
}

fn event_requires_delivery(event: &AppServerEvent) -> bool {
    match event {
        AppServerEvent::ServerNotification(notification) => {
            server_notification_requires_delivery(notification)
        }
        AppServerEvent::Disconnected { .. } => true,
        AppServerEvent::Lagged { .. } | AppServerEvent::ServerRequest(_) => false,
    }
}

async fn deliver_event<S>(
    event_tx: &mpsc::Sender<AppServerEvent>,
    skipped_events: &mut usize,
    event: AppServerEvent,
    stream: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    if *skipped_events > 0 {
        if event_requires_delivery(&event) {
            if event_tx
                .send(AppServerEvent::Lagged {
                    skipped: *skipped_events,
                })
                .await
                .is_err()
            {
                return Err(CodexIntegrationError::WebSocketTransport(
                    "remote app-server event consumer channel is closed".to_string(),
                ));
            }
            *skipped_events = 0;
        } else {
            match event_tx.try_send(AppServerEvent::Lagged {
                skipped: *skipped_events,
            }) {
                Ok(()) => *skipped_events = 0,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    *skipped_events = skipped_events.saturating_add(1);
                    reject_if_server_request_dropped(stream, &event).await?;
                    return Ok(());
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    return Err(CodexIntegrationError::WebSocketTransport(
                        "remote app-server event consumer channel is closed".to_string(),
                    ));
                }
            }
        }
    }

    if event_requires_delivery(&event) {
        event_tx.send(event).await.map_err(|_| {
            CodexIntegrationError::WebSocketTransport(
                "remote app-server event consumer channel is closed".to_string(),
            )
        })?;
        return Ok(());
    }

    match event_tx.try_send(event) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(event)) => {
            *skipped_events = skipped_events.saturating_add(1);
            reject_if_server_request_dropped(stream, &event).await
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            Err(CodexIntegrationError::WebSocketTransport(
                "remote app-server event consumer channel is closed".to_string(),
            ))
        }
    }
}

async fn reject_if_server_request_dropped<S>(
    stream: &mut tokio_tungstenite::WebSocketStream<S>,
    event: &AppServerEvent,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let AppServerEvent::ServerRequest(request) = event else {
        return Ok(());
    };

    write_jsonrpc_message(
        stream,
        JSONRPCMessage::Error(JSONRPCError {
            error: JSONRPCErrorError {
                code: -32001,
                message: "remote app-server event queue is full".to_string(),
                data: None,
            },
            id: request.id().clone(),
        }),
        "<remote-app-server>",
    )
    .await
}

async fn write_jsonrpc_message<S>(
    stream: &mut tokio_tungstenite::WebSocketStream<S>,
    message: JSONRPCMessage,
    websocket_url: &str,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload = serde_json::to_string(&message).map_err(CodexIntegrationError::Serialization)?;
    use futures_util::SinkExt;
    stream
        .send(Message::Text(payload.into()))
        .await
        .map_err(|error| {
            CodexIntegrationError::WebSocketTransport(format!(
                "failed to write websocket message to `{websocket_url}`: {error}"
            ))
        })
}

use futures_util::StreamExt;

#[cfg(test)]
mod tests {
    use super::AppServerEvent;
    use super::event_requires_delivery;
    use codex_app_server_protocol::AgentMessageDeltaNotification;
    use codex_app_server_protocol::ItemCompletedNotification;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::ThreadItem;

    #[test]
    fn event_requires_delivery_marks_transcript_and_disconnect_events() {
        assert!(event_requires_delivery(
            &AppServerEvent::ServerNotification(ServerNotification::AgentMessageDelta(
                AgentMessageDeltaNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item_id: "item".to_string(),
                    delta: "hello".to_string(),
                },
            )),
        ));
        assert!(event_requires_delivery(
            &AppServerEvent::ServerNotification(ServerNotification::ItemCompleted(
                ItemCompletedNotification {
                    thread_id: "thread".to_string(),
                    turn_id: "turn".to_string(),
                    item: ThreadItem::Plan {
                        id: "item".to_string(),
                        text: "step".to_string(),
                    },
                },
            )),
        ));
        assert!(event_requires_delivery(&AppServerEvent::Disconnected {
            message: "closed".to_string(),
        }));
        assert!(!event_requires_delivery(&AppServerEvent::Lagged {
            skipped: 1
        }));
    }
}
