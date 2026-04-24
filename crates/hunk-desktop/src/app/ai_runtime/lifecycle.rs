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
        Self::bootstrap_embedded(&config)
    }

    fn bootstrap_embedded(config: &AiWorkerStartConfig) -> Result<Self, CodexIntegrationError> {
        let session = EmbeddedAppServerClient::start(EmbeddedAppServerClientStartArgs::new(
            config.codex_home.clone(),
            config.cwd.clone(),
            config.codex_executable.clone(),
            "hunk-desktop".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ))?;

        Ok(Self::new(config, session, AppServerTransportKind::Embedded))
    }

    fn new(
        config: &AiWorkerStartConfig,
        session: EmbeddedAppServerClient,
        transport_kind: AppServerTransportKind,
    ) -> Self {
        Self {
            session,
            transport_kind,
            service: ThreadService::new(config.cwd.clone()),
            codex_home: config.codex_home.clone(),
            workspace_key: config.workspace_key.clone(),
            request_timeout: config.request_timeout,
            mad_max_mode: config.mad_max_mode,
            browser_tools_enabled: config.browser_tools_enabled,
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
            dynamic_tool_executor: if config.browser_tools_enabled {
                AiDynamicToolExecutor::with_state_only_browser()
            } else {
                AiDynamicToolExecutor::new()
            },
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
