fn sorted_threads(state: &hunk_codex::state::AiState) -> Vec<ThreadSummary> {
    let mut threads = state.threads.values().cloned().collect::<Vec<_>>();
    threads.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    threads
}

fn workspace_mad_max_mode(state: &AppState, workspace_key: Option<&str>) -> bool {
    workspace_key
        .and_then(|workspace| state.ai_workspace_mad_max.get(workspace))
        .copied()
        .unwrap_or(false)
}

fn workspace_include_hidden_models(state: &AppState, workspace_key: Option<&str>) -> bool {
    workspace_key
        .and_then(|workspace| state.ai_workspace_include_hidden_models.get(workspace))
        .copied()
        .unwrap_or(false)
}

fn reasoning_effort_key(effort: &codex_protocol::openai_models::ReasoningEffort) -> String {
    serde_json::to_value(effort)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{effort:?}").to_lowercase())
}

fn should_scroll_timeline_to_bottom_on_selection_change(
    previous_thread_id: Option<&str>,
    next_thread_id: Option<&str>,
) -> bool {
    previous_thread_id != next_thread_id && next_thread_id.is_some()
}

fn should_sync_selected_thread_from_active_thread(
    selected_thread_id: Option<&str>,
    active_thread_id: Option<&str>,
    previous_active_thread_id: Option<&str>,
    state: &hunk_codex::state::AiState,
) -> bool {
    let Some(active_thread_id) = active_thread_id else {
        return false;
    };
    let Some(active_thread) = state.threads.get(active_thread_id) else {
        return false;
    };
    if active_thread.status == ThreadLifecycleStatus::Archived {
        return false;
    }

    let selection_missing_or_invalid =
        selected_thread_id.is_none_or(|selected| !state.threads.contains_key(selected));

    selection_missing_or_invalid || previous_active_thread_id != Some(active_thread_id)
}

fn thread_latest_timeline_sequence(state: &hunk_codex::state::AiState, thread_id: &str) -> u64 {
    let thread_sequence = state
        .threads
        .get(thread_id)
        .map(|thread| thread.last_sequence)
        .unwrap_or(0);
    let turn_sequences = state
        .turns
        .values()
        .filter(|turn| turn.thread_id == thread_id)
        .map(|turn| turn.last_sequence);
    let item_sequences = state
        .items
        .values()
        .filter(|item| item.thread_id == thread_id)
        .map(|item| item.last_sequence);

    turn_sequences
        .chain(item_sequences)
        .max()
        .map_or(thread_sequence, |max_sequence| max_sequence.max(thread_sequence))
}

fn resolve_bundled_codex_executable_from_exe(current_exe: &std::path::Path) -> Option<std::path::PathBuf> {
    bundled_codex_executable_candidates(current_exe)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn bundled_codex_executable_candidates(current_exe: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Some(exe_dir) = current_exe.parent() else {
        return Vec::new();
    };

    let binary_name = codex_runtime_binary_name();
    let platform_dir = codex_runtime_platform_dir();
    let mut candidates = vec![
        exe_dir
            .join("codex-runtime")
            .join(platform_dir)
            .join(binary_name),
        exe_dir.join(binary_name),
    ];

    if cfg!(target_os = "macos")
        && let Some(contents_dir) = exe_dir.parent()
    {
        candidates.push(
            contents_dir
                .join("Resources")
                .join("codex-runtime")
                .join(platform_dir)
                .join(binary_name),
        );
    }

    candidates
}

fn codex_runtime_platform_dir() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn codex_runtime_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "codex.exe"
    } else {
        "codex"
    }
}

fn is_command_name_without_path(path: &std::path::Path) -> bool {
    if path.is_absolute() {
        return false;
    }
    let text = path.to_string_lossy();
    !text.contains(std::path::MAIN_SEPARATOR) && !text.contains('/')
}

fn normalized_thread_session_state(
    session: AiThreadSessionState,
) -> Option<AiThreadSessionState> {
    let is_empty =
        session.model.is_none() && session.effort.is_none() && session.collaboration_mode.is_none();
    if is_empty {
        return None;
    }
    Some(session)
}

fn normalized_user_input_answers(
    request: &AiPendingUserInputRequest,
    previous: Option<&BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, Vec<String>> {
    request
        .questions
        .iter()
        .map(|question| {
            let answer = previous
                .and_then(|answers| answers.get(question.id.as_str()))
                .cloned()
                .unwrap_or_else(|| default_user_input_question_answers(question));
            (question.id.clone(), answer)
        })
        .collect::<BTreeMap<_, _>>()
}

fn default_user_input_question_answers(question: &AiPendingUserInputQuestion) -> Vec<String> {
    question
        .options
        .first()
        .map(|option| vec![option.label.clone()])
        .unwrap_or_else(|| vec![String::new()])
}

