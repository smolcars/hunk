const WORKER_RECONNECT_MAX_ATTEMPTS: usize = 4;
const WORKER_RECONNECT_INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const INITIAL_RATE_LIMIT_REFRESH_DELAY: Duration = Duration::from_millis(500);

fn run_ai_worker(
    config: AiWorkerStartConfig,
    command_rx: Receiver<AiWorkerCommand>,
    event_tx: &Sender<AiWorkerEvent>,
) -> Result<(), CodexIntegrationError> {
    let mut runtime = AiWorkerRuntime::bootstrap(config.clone())?;
    let connected_message = runtime.connected_status_message();
    runtime.sync_after_connect(event_tx, connected_message.as_str(), true)?;
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
                runtime.maybe_recover_stalled_turns(&config, event_tx)?;
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
            tracing::warn!(
                transport = %self.transport_kind.status_label(),
                context,
                attempt,
                max_attempts = WORKER_RECONNECT_MAX_ATTEMPTS,
                active_thread_id = preferred_active_thread_id.as_deref().unwrap_or(""),
                "attempting AI transport reconnect"
            );
            self.send_event(
                event_tx,
                AiWorkerEventPayload::Reconnecting(format!(
                    "AI connection lost while {context}. Reconnecting ({attempt}/{WORKER_RECONNECT_MAX_ATTEMPTS})..."
                )),
            );

            match self.try_restore_transport(config, preferred_active_thread_id.as_deref()) {
                Ok(()) => {
                    tracing::info!(
                        transport = %self.transport_kind.status_label(),
                        context,
                        attempt,
                        "AI transport reconnect succeeded"
                    );
                    let connected_message = self.reconnected_status_message();
                    match self.sync_after_connect(event_tx, connected_message.as_str(), false) {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            tracing::warn!(
                                transport = %self.transport_kind.status_label(),
                                context,
                                attempt,
                                error = %error,
                                "AI reconnect succeeded but post-connect sync failed"
                            );
                            last_error = Some(error);
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        transport = %self.transport_kind.status_label(),
                        context,
                        attempt,
                        error = %error,
                        "AI transport reconnect attempt failed"
                    );
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
        tracing::warn!(
            transport = %self.transport_kind.status_label(),
            active_thread_id = preferred_active_thread_id.unwrap_or(""),
            "rebootstrapping embedded AI runtime after reconnect failure"
        );
        self.rebootstrap_runtime(config, preferred_active_thread_id)
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
        let pending_approvals = self.pending_approvals.clone();
        let pending_user_inputs = self.pending_user_inputs.clone();
        let next_approval_sequence = self.next_approval_sequence;
        let next_user_input_sequence = self.next_user_input_sequence;

        let mut replacement = Self::bootstrap(replacement_config)?;
        replacement.tool_registry = tool_registry;
        replacement.mad_max_mode = self.mad_max_mode;
        replacement.include_hidden_models = self.include_hidden_models;
        replacement.pending_approvals = pending_approvals;
        replacement.pending_user_inputs = pending_user_inputs;
        replacement.next_approval_sequence = next_approval_sequence;
        replacement.next_user_input_sequence = next_user_input_sequence;
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
            | AiWorkerCommand::SetIncludeHiddenModels { .. }
            | AiWorkerCommand::SelectThread { .. }
            | AiWorkerCommand::SetMadMaxMode { .. }
    )
}

fn command_context_message(command: &AiWorkerCommand) -> &'static str {
    match command {
        AiWorkerCommand::RefreshThreads => "refreshing AI threads",
        AiWorkerCommand::RefreshThreadMetadata { .. } => "refreshing AI thread metadata",
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
