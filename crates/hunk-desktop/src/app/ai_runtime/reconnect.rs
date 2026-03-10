const WORKER_RECONNECT_MAX_ATTEMPTS: usize = 4;
const WORKER_RECONNECT_INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const INITIAL_RATE_LIMIT_REFRESH_DELAY: Duration = Duration::from_millis(500);

fn run_ai_worker(
    config: AiWorkerStartConfig,
    command_rx: Receiver<AiWorkerCommand>,
    event_tx: &Sender<AiWorkerEvent>,
) -> Result<(), CodexIntegrationError> {
    let mut runtime = AiWorkerRuntime::bootstrap(config.clone())?;
    runtime.sync_after_connect(event_tx, "Codex App Server connected over WebSocket", true)?;
    let mut rate_limit_refresh_deadline = Some(Instant::now() + INITIAL_RATE_LIMIT_REFRESH_DELAY);

    loop {
        match command_rx.recv_timeout(COMMAND_POLL_INTERVAL) {
            Ok(AiWorkerCommand::Shutdown) => break,
            Ok(command) => {
                let retry_command = command.clone();
                if let Err(error) = runtime.handle_command(command, event_tx) {
                    if should_attempt_runtime_reconnect(&error) {
                        runtime.reconnect_after_transport_failure(
                            &config,
                            command_context_message(&retry_command),
                            event_tx,
                        )?;
                        rate_limit_refresh_deadline =
                            Some(Instant::now() + INITIAL_RATE_LIMIT_REFRESH_DELAY);
                        if command_can_retry_after_reconnect(&retry_command) {
                            if let Err(error) = runtime.handle_command(retry_command, event_tx) {
                                runtime.send_event(
                                    event_tx,
                                    AiWorkerEventPayload::Error(error.to_string()),
                                );
                            }
                        } else {
                            runtime.send_event(
                                event_tx,
                                AiWorkerEventPayload::Status(format!(
                                    "AI connection restored after {}. Refreshed state without replaying the last action.",
                                    command_context_message(&retry_command)
                                )),
                            );
                        }
                    } else {
                        runtime.send_event(
                            event_tx,
                            AiWorkerEventPayload::Error(error.to_string()),
                        );
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if rate_limit_refresh_deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
                {
                    if let Err(error) = runtime.refresh_account_rate_limits() {
                        runtime.send_event(
                            event_tx,
                            AiWorkerEventPayload::Status(format!(
                                "Unable to read account rate limits: {error}"
                            )),
                        );
                    }
                    runtime.emit_snapshot(event_tx);
                    rate_limit_refresh_deadline = None;
                }
                if let Err(error) = runtime.poll_notifications(event_tx) {
                    if should_attempt_runtime_reconnect(&error) {
                        runtime.reconnect_after_transport_failure(
                            &config,
                            "streaming AI updates",
                            event_tx,
                        )?;
                        rate_limit_refresh_deadline =
                            Some(Instant::now() + INITIAL_RATE_LIMIT_REFRESH_DELAY);
                    } else {
                        return Err(error);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

impl AiWorkerRuntime {
    fn sync_after_connect(
        &mut self,
        event_tx: &Sender<AiWorkerEvent>,
        connected_message: &str,
        emit_bootstrap_completed: bool,
    ) -> Result<(), CodexIntegrationError> {
        self.send_event(
            event_tx,
            AiWorkerEventPayload::Status(connected_message.to_string()),
        );
        self.emit_snapshot(event_tx);
        self.refresh_thread_list()?;
        self.emit_snapshot(event_tx);
        if let Err(error) = self.refresh_account_state() {
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Status(format!("Unable to read account state: {error}")),
            );
        }
        self.emit_snapshot(event_tx);
        if let Err(error) = self.refresh_session_metadata() {
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Status(format!(
                    "Unable to read model/session metadata: {error}"
                )),
            );
        }
        self.emit_snapshot(event_tx);
        if let Err(error) = self.hydrate_initial_timeline() {
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Status(format!(
                    "Unable to hydrate initial thread timeline: {error}"
                )),
            );
        }
        self.emit_snapshot(event_tx);
        if emit_bootstrap_completed {
            self.send_event(event_tx, AiWorkerEventPayload::BootstrapCompleted);
            self.emit_snapshot(event_tx);
        }
        Ok(())
    }

    fn reconnect_after_transport_failure(
        &mut self,
        config: &AiWorkerStartConfig,
        context: &str,
        event_tx: &Sender<AiWorkerEvent>,
    ) -> Result<(), CodexIntegrationError> {
        let preferred_active_thread_id =
            self.service.active_thread_for_workspace().map(ToOwned::to_owned);
        let mut last_error = None;

        for attempt in 1..=WORKER_RECONNECT_MAX_ATTEMPTS {
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Reconnecting(format!(
                    "AI connection lost while {context}. Reconnecting ({attempt}/{WORKER_RECONNECT_MAX_ATTEMPTS})..."
                )),
            );

            match self.try_restore_transport(config, preferred_active_thread_id.as_deref()) {
                Ok(()) => match self.sync_after_connect(event_tx, "AI connection restored.", false)
                {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        last_error = Some(error);
                    }
                },
                Err(error) => {
                    last_error = Some(error);
                }
            }

            if attempt < WORKER_RECONNECT_MAX_ATTEMPTS {
                thread::sleep(reconnect_backoff(attempt));
            }
        }

        Err(last_error.unwrap_or_else(|| {
            CodexIntegrationError::WebSocketTransport(
                "AI reconnect failed after exhausting retry attempts".to_string(),
            )
        }))
    }

    fn try_restore_transport(
        &mut self,
        config: &AiWorkerStartConfig,
        preferred_active_thread_id: Option<&str>,
    ) -> Result<(), CodexIntegrationError> {
        let soft_reconnect = self.try_reconnect_existing_host_session();
        if soft_reconnect.is_ok() {
            self.restore_active_thread_preference(preferred_active_thread_id);
            return Ok(());
        }

        if self.host.ensure_running(HOST_START_TIMEOUT).is_ok()
            && self.try_reconnect_existing_host_session().is_ok()
        {
            self.restore_active_thread_preference(preferred_active_thread_id);
            return Ok(());
        }

        self.rebootstrap_runtime(config, preferred_active_thread_id)
    }

    fn try_reconnect_existing_host_session(&mut self) -> Result<(), CodexIntegrationError> {
        let endpoint = WebSocketEndpoint::loopback(self.host.port());
        let mut session = JsonRpcSession::connect(&endpoint)?;
        session.initialize(InitializeOptions::default(), self.request_timeout)?;
        self.session = session;
        Ok(())
    }

    fn rebootstrap_runtime(
        &mut self,
        config: &AiWorkerStartConfig,
        preferred_active_thread_id: Option<&str>,
    ) -> Result<(), CodexIntegrationError> {
        let mut replacement_config = config.clone();
        replacement_config.mad_max_mode = self.mad_max_mode;
        replacement_config.include_hidden_models = self.include_hidden_models;

        let tool_registry = self.tool_registry.clone();

        let mut replacement = Self::bootstrap(replacement_config)?;
        replacement.tool_registry = tool_registry;
        replacement.mad_max_mode = self.mad_max_mode;
        replacement.include_hidden_models = self.include_hidden_models;
        replacement.restore_active_thread_preference(preferred_active_thread_id);
        *self = replacement;
        Ok(())
    }

    fn restore_active_thread_preference(&mut self, preferred_active_thread_id: Option<&str>) {
        let Some(thread_id) = preferred_active_thread_id else {
            return;
        };
        self.service
            .state_mut()
            .set_active_thread_for_cwd(self.workspace_key.clone(), thread_id.to_string());
    }
}

fn command_can_retry_after_reconnect(command: &AiWorkerCommand) -> bool {
    matches!(
        command,
        AiWorkerCommand::RefreshThreads
            | AiWorkerCommand::RefreshThreadMetadata { .. }
            | AiWorkerCommand::RefreshAccount
            | AiWorkerCommand::RefreshRateLimits
            | AiWorkerCommand::RefreshSessionMetadata
            | AiWorkerCommand::SetIncludeHiddenModels { .. }
            | AiWorkerCommand::SelectThread { .. }
            | AiWorkerCommand::SetMadMaxMode { .. }
    )
}

fn command_context_message(command: &AiWorkerCommand) -> &'static str {
    match command {
        AiWorkerCommand::RefreshThreads => "refreshing AI threads",
        AiWorkerCommand::RefreshThreadMetadata { .. } => "refreshing AI thread metadata",
        AiWorkerCommand::RefreshAccount => "refreshing the AI account state",
        AiWorkerCommand::RefreshRateLimits => "refreshing AI rate limits",
        AiWorkerCommand::RefreshSessionMetadata => "refreshing AI session metadata",
        AiWorkerCommand::SetIncludeHiddenModels { .. } => "updating AI model visibility",
        AiWorkerCommand::StartThread { .. } => "starting a new AI thread",
        AiWorkerCommand::SelectThread { .. } => "opening the AI thread",
        AiWorkerCommand::ArchiveThread { .. } => "archiving the AI thread",
        AiWorkerCommand::SendPrompt { .. } => "sending the AI prompt",
        AiWorkerCommand::InterruptTurn { .. } => "interrupting the AI turn",
        AiWorkerCommand::StartReview { .. } => "starting AI review mode",
        AiWorkerCommand::ResolveApproval { .. } => "resolving the AI approval request",
        AiWorkerCommand::SubmitUserInput { .. } => "submitting AI follow-up input",
        AiWorkerCommand::SetMadMaxMode { .. } => "updating AI workspace mode",
        AiWorkerCommand::StartChatgptLogin => "starting ChatGPT login",
        AiWorkerCommand::CancelChatgptLogin => "canceling ChatGPT login",
        AiWorkerCommand::LogoutAccount => "logging out of ChatGPT",
        AiWorkerCommand::Shutdown => "shutting down the AI worker",
    }
}

fn should_attempt_runtime_reconnect(error: &CodexIntegrationError) -> bool {
    match error {
        CodexIntegrationError::WebSocketTransport(_) => true,
        CodexIntegrationError::HostExitedBeforeReady { .. }
        | CodexIntegrationError::HostStartupTimedOut { .. } => true,
        CodexIntegrationError::HostProcessIo(error) => matches!(
            error.kind(),
            io::ErrorKind::BrokenPipe
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionRefused
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::NotConnected
                | io::ErrorKind::TimedOut
                | io::ErrorKind::UnexpectedEof
        ),
        _ => false,
    }
}

fn reconnect_backoff(attempt: usize) -> Duration {
    let exponent = attempt.saturating_sub(1).min(8);
    let multiplier = 1u32 << exponent;
    WORKER_RECONNECT_INITIAL_BACKOFF.saturating_mul(multiplier)
}
