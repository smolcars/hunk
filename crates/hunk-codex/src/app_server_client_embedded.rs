use std::path::PathBuf;
use std::time::Duration;

use crate::protocol::ClientNotification;
use crate::protocol::ClientRequest;
use crate::protocol::JSONRPCErrorError;
use crate::protocol::RequestId;
use crate::protocol::SessionSource;
use codex_app_server::in_process::InProcessServerEvent;
use codex_arg0::Arg0DispatchPaths;
use codex_core::config::ConfigBuilder;
use codex_exec_server::EnvironmentManager;
use codex_feedback::CodexFeedback;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Map;
use serde_json::Value;
use tokio::runtime::Builder;
use tokio::runtime::Runtime;
use tokio::time::timeout;

use crate::app_server_client::AppServerClient;
use crate::app_server_client::AppServerEvent;
use crate::app_server_client::DEFAULT_APP_SERVER_CHANNEL_CAPACITY;
use crate::errors::CodexIntegrationError;
use crate::errors::Result;
use crate::in_process_app_server_client::InProcessAppServerClient;
use crate::in_process_app_server_client::InProcessClientStartArgs;
use crate::in_process_app_server_client::TypedRequestError;
use crate::rpc::RequestIdGenerator;

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

pub struct EmbeddedAppServerClient {
    runtime: Runtime,
    client: Option<InProcessAppServerClient>,
    request_ids: RequestIdGenerator,
}

impl EmbeddedAppServerClient {
    pub fn start(args: EmbeddedAppServerClientStartArgs) -> Result<Self> {
        let runtime = build_runtime()?;
        let client = runtime.block_on(async move {
            let config = ConfigBuilder::default()
                .codex_home(args.codex_home.clone())
                .fallback_cwd(Some(args.fallback_cwd.clone()))
                .build()
                .await
                .map_err(|error| {
                    CodexIntegrationError::WebSocketTransport(format!(
                        "failed to load embedded Codex config: {error}"
                    ))
                })?;

            let config_warnings = config
                .startup_warnings
                .iter()
                .map(|warning| crate::protocol::ConfigWarningNotification {
                    summary: warning.clone(),
                    details: None,
                    path: None,
                    range: None,
                })
                .collect();

            InProcessAppServerClient::start(InProcessClientStartArgs {
                arg0_paths: Arg0DispatchPaths {
                    codex_self_exe: Some(args.codex_executable.clone()),
                    codex_linux_sandbox_exe: None,
                    main_execve_wrapper_exe: None,
                },
                config: std::sync::Arc::new(config),
                feedback: CodexFeedback::new(),
                environment_manager: std::sync::Arc::new(EnvironmentManager::from_env()),
                config_warnings,
                session_source: SessionSource::Custom(args.client_name.clone()),
                enable_codex_api_key_env: true,
                client_name: args.client_name,
                client_version: args.client_version,
                experimental_api: true,
                opt_out_notification_methods: Vec::new(),
                channel_capacity: DEFAULT_APP_SERVER_CHANNEL_CAPACITY.max(1),
            })
            .await
            .map_err(|error| {
                CodexIntegrationError::WebSocketTransport(format!(
                    "failed to start embedded Codex app server: {error}"
                ))
            })
        })?;

        Ok(Self {
            runtime,
            client: Some(client),
            request_ids: RequestIdGenerator::default(),
        })
    }

    fn client(&self) -> Result<&InProcessAppServerClient> {
        self.client.as_ref().ok_or_else(missing_client_error)
    }
}

impl AppServerClient for EmbeddedAppServerClient {
    fn request_typed<P, R>(
        &mut self,
        method: &str,
        params: Option<&P>,
        timeout_duration: Duration,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let request_id = self.request_ids.next_request_id();
        let request = client_request(method, params, request_id)?;
        let runtime = &self.runtime;
        let client = self.client()?;
        match runtime
            .block_on(async { timeout(timeout_duration, client.request_typed(request)).await })
        {
            Err(_) => Err(CodexIntegrationError::RequestTimedOut {
                method: method.to_string(),
                timeout_ms: timeout_duration.as_millis().min(u128::from(u64::MAX)) as u64,
            }),
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => Err(map_typed_request_error(error)),
        }
    }

    fn notify<P>(&mut self, method: &str, params: Option<&P>) -> Result<()>
    where
        P: Serialize,
    {
        let notification = client_notification(method, params)?;
        let runtime = &self.runtime;
        let client = self.client()?;
        runtime
            .block_on(client.notify(notification))
            .map_err(|error| {
                CodexIntegrationError::WebSocketTransport(format!(
                    "embedded app-server notification failed: {error}"
                ))
            })
    }

    fn next_event(&mut self, timeout_duration: Duration) -> Result<Option<AppServerEvent>> {
        let Self {
            runtime, client, ..
        } = self;
        let client = client.as_mut().ok_or_else(missing_client_error)?;
        match runtime.block_on(async { timeout(timeout_duration, client.next_event()).await }) {
            Err(_) => Ok(None),
            Ok(Some(event)) => Ok(Some(map_event(event))),
            Ok(None) => Ok(None),
        }
    }

    fn respond_typed<T>(&mut self, request_id: RequestId, result: &T) -> Result<()>
    where
        T: Serialize,
    {
        let result = serde_json::to_value(result).map_err(CodexIntegrationError::Serialization)?;
        let runtime = &self.runtime;
        let client = self.client()?;
        runtime
            .block_on(client.resolve_server_request(request_id, result))
            .map_err(|error| {
                CodexIntegrationError::WebSocketTransport(format!(
                    "embedded app-server response failed: {error}"
                ))
            })
    }

    fn reject_server_request(
        &mut self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> Result<()> {
        let runtime = &self.runtime;
        let client = self.client()?;
        runtime
            .block_on(client.reject_server_request(request_id, error))
            .map_err(|error| {
                CodexIntegrationError::WebSocketTransport(format!(
                    "embedded app-server rejection failed: {error}"
                ))
            })
    }

    fn shutdown(&mut self) -> Result<()> {
        let Some(client) = self.client.take() else {
            return Ok(());
        };
        self.runtime.block_on(client.shutdown()).map_err(|error| {
            CodexIntegrationError::WebSocketTransport(format!(
                "embedded app-server shutdown failed: {error}"
            ))
        })
    }
}

impl Drop for EmbeddedAppServerClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn build_runtime() -> Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(2)
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| {
            CodexIntegrationError::WebSocketTransport(format!(
                "failed to build embedded app-server runtime: {error}"
            ))
        })
}

fn client_request<P>(
    method: &str,
    params: Option<&P>,
    request_id: RequestId,
) -> Result<ClientRequest>
where
    P: Serialize,
{
    let mut value = Map::new();
    value.insert("method".to_string(), Value::String(method.to_string()));
    value.insert(
        "id".to_string(),
        serde_json::to_value(request_id).map_err(CodexIntegrationError::Serialization)?,
    );
    if let Some(params) = params {
        value.insert(
            "params".to_string(),
            serde_json::to_value(params).map_err(CodexIntegrationError::Serialization)?,
        );
    }
    serde_json::from_value(Value::Object(value)).map_err(CodexIntegrationError::Serialization)
}

fn client_notification<P>(method: &str, params: Option<&P>) -> Result<ClientNotification>
where
    P: Serialize,
{
    let mut value = Map::new();
    value.insert("method".to_string(), Value::String(method.to_string()));
    if let Some(params) = params {
        value.insert(
            "params".to_string(),
            serde_json::to_value(params).map_err(CodexIntegrationError::Serialization)?,
        );
    }
    serde_json::from_value(Value::Object(value)).map_err(CodexIntegrationError::Serialization)
}

fn map_event(event: InProcessServerEvent) -> AppServerEvent {
    match event {
        InProcessServerEvent::Lagged { skipped } => AppServerEvent::Lagged { skipped },
        InProcessServerEvent::ServerNotification(notification) => {
            AppServerEvent::ServerNotification(notification)
        }
        InProcessServerEvent::ServerRequest(request) => AppServerEvent::ServerRequest(request),
    }
}

fn map_typed_request_error(error: TypedRequestError) -> CodexIntegrationError {
    match error {
        TypedRequestError::Transport { source, .. } => CodexIntegrationError::WebSocketTransport(
            format!("embedded app-server request failed: {source}"),
        ),
        TypedRequestError::Server { source, .. } => CodexIntegrationError::JsonRpcServerError {
            code: source.code,
            message: source.message,
        },
        TypedRequestError::Deserialize { source, .. } => {
            CodexIntegrationError::Serialization(source)
        }
    }
}

fn missing_client_error() -> CodexIntegrationError {
    CodexIntegrationError::WebSocketTransport(
        "embedded app-server client is not available".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::EmbeddedAppServerClient;

    #[test]
    fn embedded_client_start_args_type_is_available() {
        let _ = std::mem::size_of::<EmbeddedAppServerClient>();
    }
}
