fn thread_missing_item_turn_ids(state: &AiState, thread_id: &str) -> BTreeSet<String> {
    let turn_ids = state
        .turns
        .values()
        .filter(|turn| turn.thread_id == thread_id)
        .map(|turn| turn.id.clone())
        .collect::<BTreeSet<_>>();
    if turn_ids.is_empty() {
        return BTreeSet::new();
    }

    let turns_with_items = state
        .items
        .values()
        .filter(|item| item.thread_id == thread_id && turn_ids.contains(item.turn_id.as_str()))
        .map(|item| item.turn_id.clone())
        .collect::<BTreeSet<_>>();
    turn_ids
        .into_iter()
        .filter(|turn_id| !turns_with_items.contains(turn_id))
        .collect()
}

fn should_retry_stale_turn_after_steer_error(error: &CodexIntegrationError) -> bool {
    let CodexIntegrationError::JsonRpcServerError { code, message } = error else {
        return false;
    };
    if !matches!(*code, -32600 | -32602 | -32000 | -32001) {
        return false;
    }

    let normalized_message = message.to_ascii_lowercase();
    normalized_message.contains("stale")
        || normalized_message.contains("expected_turn_id")
        || normalized_message.contains("expected turn id")
        || normalized_message.contains("expected active turn id")
        || normalized_message.contains("turn id mismatch")
        || normalized_message.contains("in-progress turn")
        || normalized_message.contains("in progress turn")
        || normalized_message.contains("no active turn to steer")
}

fn is_transient_rollout_load_error(error: &CodexIntegrationError) -> bool {
    let CodexIntegrationError::JsonRpcServerError { code, message } = error else {
        return false;
    };
    if *code != -32603 {
        return false;
    }

    let normalized_message = message.to_ascii_lowercase();
    normalized_message.contains("failed to load rollout")
        && normalized_message.contains("is empty")
}

fn is_missing_thread_rollout_error(error: &CodexIntegrationError) -> bool {
    let CodexIntegrationError::JsonRpcServerError { code, message } = error else {
        return false;
    };
    if *code != -32600 {
        return false;
    }

    let normalized_message = message.to_ascii_lowercase();
    normalized_message.contains("no rollout found for thread id")
}

fn retry_transient_rollout_load<T, F>(
    max_retries: usize,
    retry_delay: std::time::Duration,
    mut operation: F,
) -> Result<T, CodexIntegrationError>
where
    F: FnMut() -> Result<T, CodexIntegrationError>,
{
    let mut attempts = 0usize;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error)
                if is_transient_rollout_load_error(&error) && attempts < max_retries =>
            {
                attempts = attempts.saturating_add(1);
                if !retry_delay.is_zero() {
                    std::thread::sleep(retry_delay);
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn map_command_approval_decision(decision: AiApprovalDecision) -> CommandExecutionApprovalDecision {
    match decision {
        AiApprovalDecision::Accept => CommandExecutionApprovalDecision::Accept,
        AiApprovalDecision::Decline => CommandExecutionApprovalDecision::Decline,
    }
}

fn map_file_change_approval_decision(decision: AiApprovalDecision) -> FileChangeApprovalDecision {
    match decision {
        AiApprovalDecision::Accept => FileChangeApprovalDecision::Accept,
        AiApprovalDecision::Decline => FileChangeApprovalDecision::Decline,
    }
}

fn map_server_request_decision(decision: AiApprovalDecision) -> ServerRequestDecision {
    match decision {
        AiApprovalDecision::Accept => ServerRequestDecision::Accept,
        AiApprovalDecision::Decline => ServerRequestDecision::Decline,
    }
}

fn apply_thread_start_policy(mad_max_mode: bool, params: &mut ThreadStartParams) {
    if mad_max_mode {
        params.approval_policy = Some(AskForApproval::Never);
        params.sandbox = Some(SandboxMode::DangerFullAccess);
    } else {
        params.approval_policy = Some(AskForApproval::OnRequest);
        params.sandbox = Some(non_mad_max_thread_sandbox_mode());
    }
}

fn apply_turn_start_policy(mad_max_mode: bool, params: &mut TurnStartParams) {
    if mad_max_mode {
        params.approval_policy = Some(AskForApproval::Never);
        params.sandbox_policy = Some(SandboxPolicy::DangerFullAccess);
    } else {
        params.approval_policy = Some(AskForApproval::OnRequest);
        params.sandbox_policy = Some(non_mad_max_turn_sandbox_policy());
    }
}

fn non_mad_max_thread_sandbox_mode() -> SandboxMode {
    SandboxMode::WorkspaceWrite
}

fn non_mad_max_turn_sandbox_policy() -> SandboxPolicy {
    SandboxPolicy::WorkspaceWrite {
        writable_roots: Vec::new(),
        read_only_access: ReadOnlyAccess::FullAccess,
        network_access: false,
        exclude_tmpdir_env_var: false,
        exclude_slash_tmp: false,
    }
}

fn apply_thread_start_session_overrides(
    session_overrides: &AiTurnSessionOverrides,
    params: &mut ThreadStartParams,
) {
    params.model = session_overrides.model.clone();
    params.service_tier = selected_ai_service_tier(session_overrides.service_tier);
}

fn selected_ai_service_tier(
    selection: AiServiceTierSelection,
) -> Option<Option<ServiceTier>> {
    Some(match selection {
        AiServiceTierSelection::Standard => None,
        AiServiceTierSelection::Fast => Some(ServiceTier::Fast),
        AiServiceTierSelection::Flex => Some(ServiceTier::Flex),
    })
}

fn parse_reasoning_effort(raw: &str) -> Option<ReasoningEffort> {
    serde_json::from_value::<ReasoningEffort>(serde_json::Value::String(raw.to_string())).ok()
}

fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        RequestId::String(value) => format!("str:{value}"),
        RequestId::Integer(value) => format!("int:{value}"),
    }
}

fn preferred_rate_limit_snapshot(
    snapshots_by_limit_id: &HashMap<String, RateLimitSnapshot>,
    fallback: Option<&RateLimitSnapshot>,
) -> Option<RateLimitSnapshot> {
    codex_rate_limit_snapshot(snapshots_by_limit_id)
        .or_else(|| fallback.cloned())
        .or_else(|| first_rate_limit_snapshot(snapshots_by_limit_id))
}

fn codex_rate_limit_snapshot(
    snapshots_by_limit_id: &HashMap<String, RateLimitSnapshot>,
) -> Option<RateLimitSnapshot> {
    snapshots_by_limit_id
        .iter()
        .find(|(limit_id, _)| limit_id.eq_ignore_ascii_case("codex"))
        .map(|(_, snapshot)| snapshot.clone())
}

fn first_rate_limit_snapshot(
    snapshots_by_limit_id: &HashMap<String, RateLimitSnapshot>,
) -> Option<RateLimitSnapshot> {
    let first_limit_id = snapshots_by_limit_id.keys().min()?.clone();
    snapshots_by_limit_id.get(first_limit_id.as_str()).cloned()
}

fn ordered_pending_approvals(
    pending_approvals: &BTreeMap<String, PendingApproval>,
) -> Vec<AiPendingApproval> {
    let mut approvals = pending_approvals.values().cloned().collect::<Vec<_>>();
    approvals.sort_by_key(|pending| pending.sequence);
    approvals
        .into_iter()
        .map(|pending| pending.approval)
        .collect::<Vec<_>>()
}

fn ordered_pending_user_inputs(
    pending_user_inputs: &BTreeMap<String, PendingUserInput>,
) -> Vec<AiPendingUserInputRequest> {
    let mut requests = pending_user_inputs.values().cloned().collect::<Vec<_>>();
    requests.sort_by_key(|pending| pending.sequence);
    requests
        .into_iter()
        .map(|pending| pending.request)
        .collect::<Vec<_>>()
}

fn map_pending_user_input_question(
    question: ToolRequestUserInputQuestion,
) -> AiPendingUserInputQuestion {
    AiPendingUserInputQuestion {
        id: question.id,
        header: question.header,
        question: question.question,
        is_other: question.is_other,
        is_secret: question.is_secret,
        options: question
            .options
            .unwrap_or_default()
            .into_iter()
            .map(|option| AiPendingUserInputQuestionOption {
                label: option.label,
                description: option.description,
            })
            .collect::<Vec<_>>(),
    }
}

fn default_user_input_answers(
    questions: &[AiPendingUserInputQuestion],
) -> BTreeMap<String, Vec<String>> {
    questions
        .iter()
        .map(|question| {
            let answer = question
                .options
                .first()
                .map(|option| option.label.clone())
                .unwrap_or_default();
            (question.id.clone(), vec![answer])
        })
        .collect::<BTreeMap<_, _>>()
}

fn map_user_input_answers(
    answers: BTreeMap<String, Vec<String>>,
) -> HashMap<String, ToolRequestUserInputAnswer> {
    answers
        .into_iter()
        .map(|(question_id, answers)| (question_id, ToolRequestUserInputAnswer { answers }))
        .collect::<HashMap<_, _>>()
}

fn apply_login_completed_state(
    pending_chatgpt_login_id: &mut Option<String>,
    pending_chatgpt_auth_url: &mut Option<String>,
    completed: &codex_app_server_protocol::AccountLoginCompletedNotification,
) -> String {
    *pending_chatgpt_login_id = None;
    *pending_chatgpt_auth_url = None;
    if completed.success {
        return "ChatGPT login completed.".to_string();
    }

    completed
        .error
        .clone()
        .map(|error| format!("ChatGPT login failed: {error}"))
        .unwrap_or_else(|| "ChatGPT login failed.".to_string())
}

fn allocate_loopback_port() -> u16 {
    if let Ok(listener) = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        && let Ok(address) = listener.local_addr()
        && address.port() != 0
    {
        return address.port();
    }

    let initial_seed = {
        let pid = std::process::id() as u16;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos() as u16)
            .unwrap_or(0);
        let mixed = pid.wrapping_mul(977).wrapping_add(nanos);
        mixed.max(1)
    };
    let _ = NEXT_LOOPBACK_PORT_OFFSET.compare_exchange(
        0,
        initial_seed,
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
    let offset = NEXT_LOOPBACK_PORT_OFFSET.fetch_add(1, Ordering::Relaxed) % LOOPBACK_PORT_RANGE_SIZE;
    LOOPBACK_PORT_RANGE_START.saturating_add(offset)
}

fn should_retry_bootstrap_with_new_port(error: &CodexIntegrationError) -> bool {
    match error {
        CodexIntegrationError::HostExitedBeforeReady { status } => {
            let normalized = status.to_ascii_lowercase();
            normalized.contains("address already in use")
                || normalized.contains("addrinuse")
                || normalized.contains("10048")
                || normalized.contains("10013")
                || normalized.contains("forbidden by its access permissions")
                || normalized.contains("access permissions")
        }
        CodexIntegrationError::HostStartupTimedOut { .. } => true,
        CodexIntegrationError::WebSocketTransport(message) => {
            let normalized = message.to_ascii_lowercase();
            normalized.contains("connection refused")
                || normalized.contains("timed out")
                || normalized.contains("closed")
        }
        _ => false,
    }
}

fn open_url_in_system_browser(url: &str) -> Result<(), CodexIntegrationError> {
    crate::app::url_open::open_url_in_browser(url).map_err(|error| {
        CodexIntegrationError::HostProcessIo(io::Error::other(error.to_string()))
    })
}
