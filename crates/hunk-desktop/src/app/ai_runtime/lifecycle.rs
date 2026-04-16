pub fn spawn_ai_worker(
    config: AiWorkerStartConfig,
    command_rx: Receiver<AiWorkerCommand>,
    event_tx: Sender<AiWorkerEvent>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let workspace_key = config.workspace_key.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_ai_worker(config, command_rx, &event_tx)
        }));
        dispatch_ai_worker_result(result, workspace_key.as_str(), &event_tx);
    })
}

fn dispatch_ai_worker_result(
    result: std::thread::Result<Result<(), CodexIntegrationError>>,
    workspace_key: &str,
    event_tx: &Sender<AiWorkerEvent>,
) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            send_ai_worker_event(
                event_tx,
                workspace_key,
                AiWorkerEventPayload::Fatal(error.to_string()),
            );
        }
        Err(payload) => {
            send_ai_worker_event(
                event_tx,
                workspace_key,
                AiWorkerEventPayload::Fatal(format!(
                    "AI worker panicked: {}",
                    panic_payload_message(payload)
                )),
            );
        }
    }
}

fn send_ai_worker_event(
    event_tx: &Sender<AiWorkerEvent>,
    workspace_key: &str,
    payload: AiWorkerEventPayload,
) {
    let _ = event_tx.send(AiWorkerEvent::new(workspace_key.to_string(), payload));
}

fn panic_payload_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "unknown panic payload".to_string(),
        },
    }
}

impl AiWorkerRuntime {
    fn bootstrap(config: AiWorkerStartConfig) -> Result<Self, CodexIntegrationError> {
        std::fs::create_dir_all(&config.codex_home)
            .map_err(CodexIntegrationError::HostProcessIo)?;

        let mut fallback_note = None;
        let mut last_error = None;
        for transport_kind in config.transport_preference.bootstrap_candidates() {
            match transport_kind {
                AppServerTransportKind::Embedded => match Self::bootstrap_embedded(&config) {
                    Ok(mut runtime) => {
                        runtime.transport_bootstrap_note = fallback_note;
                        return Ok(runtime);
                    }
                    Err(error) => {
                        fallback_note = fallback_note.or_else(|| {
                            Some(format!(
                                "Embedded Codex App Server startup failed. Falling back to remote bundled runtime: {error}"
                            ))
                        });
                        last_error = Some(error);
                    }
                },
                AppServerTransportKind::RemoteBundled => match Self::bootstrap_remote(&config) {
                    Ok(mut runtime) => {
                        runtime.transport_bootstrap_note = fallback_note;
                        return Ok(runtime);
                    }
                    Err(error) => {
                        last_error = Some(error);
                    }
                },
            }
        }

        Err(last_error.unwrap_or(CodexIntegrationError::WebSocketTransport(
            "unable to start any configured Codex App Server transport".to_string(),
        )))
    }

    fn bootstrap_remote(config: &AiWorkerStartConfig) -> Result<Self, CodexIntegrationError> {
        let mut last_retryable_error = None;
        for _attempt in 0..HOST_BOOTSTRAP_MAX_ATTEMPTS {
            let port = allocate_loopback_port();
            match Self::bootstrap_remote_on_port(config, port) {
                Ok(runtime) => return Ok(runtime),
                Err(error) if should_retry_bootstrap_with_new_port(&error) => {
                    last_retryable_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }

        Err(last_retryable_error.unwrap_or(CodexIntegrationError::HostStartupTimedOut {
            port: 0,
            timeout_ms: HOST_START_TIMEOUT
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        }))
    }

    fn bootstrap_remote_on_port(
        config: &AiWorkerStartConfig,
        port: u16,
    ) -> Result<Self, CodexIntegrationError> {
        let host_config = HostConfig::codex_app_server(
            config.codex_executable.clone(),
            config.host_working_directory.clone(),
            config.codex_home.clone(),
            port,
        );
        let host = SharedHostLease::acquire(host_config, HOST_START_TIMEOUT)?;

        let session = ManagedAppServerClient::Remote(RemoteAppServerClient::connect_loopback(
            host.port(),
            config.request_timeout,
        )?);

        Ok(Self::new(
            config,
            Some(host),
            session,
            AppServerTransportKind::RemoteBundled,
        ))
    }

    fn bootstrap_embedded(config: &AiWorkerStartConfig) -> Result<Self, CodexIntegrationError> {
        let session =
            ManagedAppServerClient::Embedded(EmbeddedAppServerClient::start(
                EmbeddedAppServerClientStartArgs::new(
                    config.codex_home.clone(),
                    config.cwd.clone(),
                    config.codex_executable.clone(),
                    "hunk-desktop".to_string(),
                    env!("CARGO_PKG_VERSION").to_string(),
                ),
            )?);

        Ok(Self::new(
            config,
            None,
            session,
            AppServerTransportKind::Embedded,
        ))
    }

    fn new(
        config: &AiWorkerStartConfig,
        host: Option<SharedHostLease>,
        session: ManagedAppServerClient,
        transport_kind: AppServerTransportKind,
    ) -> Self {
        Self {
            host,
            session,
            transport_kind,
            transport_bootstrap_note: None,
            service: ThreadService::new(config.cwd.clone()),
            codex_home: config.codex_home.clone(),
            workspace_key: config.workspace_key.clone(),
            request_timeout: config.request_timeout,
            mad_max_mode: config.mad_max_mode,
            account: None,
            requires_openai_auth: false,
            pending_chatgpt_login_id: None,
            pending_chatgpt_auth_url: None,
            rate_limits: None,
            rate_limits_by_limit_id: HashMap::new(),
            models: Vec::new(),
            experimental_features: Vec::new(),
            collaboration_modes: Vec::new(),
            skills: Vec::new(),
            include_hidden_models: config.include_hidden_models,
            tool_registry: DynamicToolRegistry::new(),
            pending_approvals: BTreeMap::new(),
            pending_user_inputs: BTreeMap::new(),
            next_approval_sequence: 1,
            next_user_input_sequence: 1,
            turn_stream_watches: BTreeMap::new(),
        }
    }

    fn send_event(&self, event_tx: &Sender<AiWorkerEvent>, payload: AiWorkerEventPayload) {
        send_ai_worker_event(event_tx, self.workspace_key.as_str(), payload);
    }

    fn connected_status_message(&self) -> String {
        format!("Connected to {}.", self.transport_kind.status_label())
    }

    fn reconnected_status_message(&self) -> String {
        format!(
            "AI connection restored on {}.",
            self.transport_kind.status_label()
        )
    }
}
