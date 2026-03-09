#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(target_os = "windows")]
use std::ffi::OsString;
use std::collections::BTreeSet;

use codex_app_server_protocol::CollaborationModeMask;
use codex_protocol::config_types::ModeKind;

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
        .unwrap_or(true)
}

#[cfg(test)]
fn resolved_ai_workspace_cwd(
    project_path: Option<&std::path::Path>,
    repo_root: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    match (project_path, repo_root) {
        (Some(project_path), Some(repo_root)) => {
            if project_path.starts_with(repo_root) || repo_root.starts_with(project_path) {
                Some(repo_root.to_path_buf())
            } else {
                Some(project_path.to_path_buf())
            }
        }
        (Some(project_path), None) => Some(project_path.to_path_buf()),
        (None, Some(repo_root)) => Some(repo_root.to_path_buf()),
        (None, None) => None,
    }
}

fn ai_thread_start_mode_for_workspace(
    repo_root: Option<&std::path::Path>,
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    thread_cwd: &std::path::Path,
) -> Option<AiNewThreadStartMode> {
    if let Some(target) = workspace_targets
        .iter()
        .find(|target| target.root.as_path() == thread_cwd)
    {
        return Some(match target.kind {
            hunk_git::worktree::WorkspaceTargetKind::PrimaryCheckout => {
                AiNewThreadStartMode::Local
            }
            hunk_git::worktree::WorkspaceTargetKind::LinkedWorktree => {
                AiNewThreadStartMode::Worktree
            }
        });
    }

    repo_root.map(|repo_root| {
        if repo_root == thread_cwd {
            AiNewThreadStartMode::Local
        } else {
            AiNewThreadStartMode::Worktree
        }
    })
}

fn ai_collaboration_mode_matches_kind(mask: &CollaborationModeMask, kind: ModeKind) -> bool {
    mask.mode == Some(kind) || mask.name.eq_ignore_ascii_case(kind.display_name())
}

fn ai_collaboration_mode_from_mask(
    mask: &CollaborationModeMask,
) -> Option<AiCollaborationModeSelection> {
    if ai_collaboration_mode_matches_kind(mask, ModeKind::Default) {
        Some(AiCollaborationModeSelection::Default)
    } else if ai_collaboration_mode_matches_kind(mask, ModeKind::Plan) {
        Some(AiCollaborationModeSelection::Plan)
    } else {
        None
    }
}

fn ai_collaboration_mode_mask(
    modes: &[CollaborationModeMask],
    selection: AiCollaborationModeSelection,
) -> Option<&CollaborationModeMask> {
    modes.iter()
        .find(|mask| ai_collaboration_mode_from_mask(mask) == Some(selection))
}

fn ai_composer_draft_key(
    thread_id: Option<&str>,
    workspace_key: Option<&str>,
) -> Option<AiComposerDraftKey> {
    thread_id
        .map(|thread_id| AiComposerDraftKey::Thread(thread_id.to_string()))
        .or_else(|| {
            workspace_key.map(|workspace| AiComposerDraftKey::Workspace(workspace.to_string()))
        })
}

fn ai_composer_prompt_for_target(
    drafts: &BTreeMap<AiComposerDraftKey, AiComposerDraft>,
    target_key: Option<&AiComposerDraftKey>,
) -> String {
    target_key
        .and_then(|key| drafts.get(key))
        .map(|draft| draft.prompt.clone())
        .unwrap_or_default()
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

fn should_scroll_timeline_to_bottom_on_new_activity(
    latest_sequence: u64,
    previous_sequence: u64,
    follow_output: bool,
) -> bool {
    follow_output && latest_sequence > previous_sequence
}

fn should_follow_timeline_output(
    row_count: usize,
    scroll_offset_y: f32,
    max_scroll_offset_y: f32,
) -> bool {
    const TIMELINE_BOTTOM_EPSILON_PX: f32 = 1.0;

    if row_count == 0 || max_scroll_offset_y <= TIMELINE_BOTTOM_EPSILON_PX {
        return true;
    }

    let current_scroll_offset_y = (-scroll_offset_y).clamp(0.0, max_scroll_offset_y);
    current_scroll_offset_y >= max_scroll_offset_y - TIMELINE_BOTTOM_EPSILON_PX
}

fn timeline_visible_turn_ids(
    turn_ids: &[String],
    configured_limit: usize,
) -> (usize, usize, usize, Vec<String>) {
    let total_turn_count = turn_ids.len();
    let visible_turn_count = configured_limit.min(total_turn_count);
    let hidden_turn_count = total_turn_count.saturating_sub(visible_turn_count);
    let visible_turn_ids = turn_ids
        .iter()
        .skip(hidden_turn_count)
        .cloned()
        .collect::<Vec<_>>();
    (
        total_turn_count,
        visible_turn_count,
        hidden_turn_count,
        visible_turn_ids,
    )
}

fn timeline_turn_ids_by_thread(
    state: &hunk_codex::state::AiState,
) -> BTreeMap<String, Vec<String>> {
    let mut turn_ids_by_thread = BTreeMap::<String, Vec<(u64, String)>>::new();
    for turn in state.turns.values() {
        turn_ids_by_thread
            .entry(turn.thread_id.clone())
            .or_default()
            .push((turn.last_sequence, turn.id.clone()));
    }

    turn_ids_by_thread
        .into_iter()
        .map(|(thread_id, mut entries)| {
            entries.sort_by(|left, right| {
                left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1))
            });
            let ids = entries
                .into_iter()
                .map(|(_, turn_id)| turn_id)
                .collect::<Vec<_>>();
            (thread_id, ids)
        })
        .collect()
}

fn timeline_visible_row_ids_for_turns(
    row_ids: &[String],
    rows_by_id: &BTreeMap<String, AiTimelineRow>,
    visible_turn_ids: &[String],
) -> Vec<String> {
    if visible_turn_ids.is_empty() {
        return Vec::new();
    }
    let visible_turn_ids = visible_turn_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    row_ids
        .iter()
        .filter(|row_id| {
            rows_by_id
                .get(row_id.as_str())
                .is_some_and(|row| visible_turn_ids.contains(row.turn_id.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>()
}

fn ai_timeline_item_is_renderable_for_layout(item: &hunk_codex::state::ItemSummary) -> bool {
    if matches!(item.kind.as_str(), "reasoning" | "webSearch") {
        let has_content = !item.content.trim().is_empty();
        let has_metadata = item.display_metadata.as_ref().is_some_and(|metadata| {
            metadata
                .summary
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || metadata
                    .details_json
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        });
        return has_content || has_metadata;
    }

    true
}

fn ai_timeline_row_is_renderable_for_layout(
    state: &hunk_codex::state::AiState,
    row: &AiTimelineRow,
) -> bool {
    match &row.source {
        AiTimelineRowSource::Item { item_key } => state
            .items
            .get(item_key.as_str())
            .is_some_and(ai_timeline_item_is_renderable_for_layout),
        AiTimelineRowSource::Group { .. } => true,
        AiTimelineRowSource::TurnDiff { turn_key } => state
            .turn_diffs
            .get(turn_key.as_str())
            .is_some_and(|diff| !diff.trim().is_empty()),
    }
}

fn current_ai_renderable_visible_row_ids(this: &DiffViewer, thread_id: &str) -> Vec<String> {
    let (_, _, _, visible_row_ids) = this.ai_timeline_visible_rows_for_thread(thread_id);
    visible_row_ids
        .into_iter()
        .filter(|row_id| {
            this.ai_timeline_row(row_id.as_str()).is_some_and(|row| {
                ai_timeline_row_is_renderable_for_layout(&this.ai_state_snapshot, row)
            })
        })
        .collect()
}

fn timeline_row_ids_with_height_changes(
    previous_state: &hunk_codex::state::AiState,
    next_state: &hunk_codex::state::AiState,
    thread_id: &str,
) -> BTreeSet<String> {
    let mut changed_row_ids = BTreeSet::new();

    let item_keys = previous_state
        .items
        .iter()
        .filter(|(_, item)| item.thread_id == thread_id)
        .map(|(item_key, _)| item_key.clone())
        .chain(
            next_state
                .items
                .iter()
                .filter(|(_, item)| item.thread_id == thread_id)
                .map(|(item_key, _)| item_key.clone()),
        )
        .collect::<BTreeSet<_>>();
    for item_key in item_keys {
        if previous_state.items.get(item_key.as_str()) != next_state.items.get(item_key.as_str()) {
            changed_row_ids.insert(format!("item:{item_key}"));
        }
    }

    let turn_keys = previous_state
        .turns
        .iter()
        .filter(|(_, turn)| turn.thread_id == thread_id)
        .map(|(turn_key, _)| turn_key.clone())
        .chain(
            next_state
                .turns
                .iter()
                .filter(|(_, turn)| turn.thread_id == thread_id)
                .map(|(turn_key, _)| turn_key.clone()),
        )
        .collect::<BTreeSet<_>>();
    for turn_key in turn_keys {
        if previous_state.turn_diffs.get(turn_key.as_str())
            != next_state.turn_diffs.get(turn_key.as_str())
        {
            changed_row_ids.insert(format!("turn-diff:{turn_key}"));
        }
    }

    changed_row_ids
}

fn should_reset_ai_timeline_measurements(
    previous_thread_id: Option<&str>,
    next_thread_id: Option<&str>,
    previous_visible_row_ids: &[String],
    next_visible_row_ids: &[String],
    cached_row_count: usize,
) -> bool {
    previous_thread_id != next_thread_id
        || previous_visible_row_ids != next_visible_row_ids
        || cached_row_count != next_visible_row_ids.len()
}

fn reset_ai_timeline_list_measurements(this: &mut DiffViewer, row_count: usize) {
    let previous_top = this.ai_timeline_list_state.logical_scroll_top();
    this.ai_timeline_list_state.reset(row_count);
    let item_ix = if row_count == 0 {
        0
    } else {
        previous_top.item_ix.min(row_count.saturating_sub(1))
    };
    let offset_in_item = if row_count == 0 || item_ix != previous_top.item_ix {
        px(0.)
    } else {
        previous_top.offset_in_item
    };
    this.ai_timeline_list_state.scroll_to(ListOffset {
        item_ix,
        offset_in_item,
    });
    this.ai_timeline_list_row_count = row_count;
}

fn invalidate_ai_timeline_row_measurements(
    this: &mut DiffViewer,
    visible_row_ids: &[String],
    changed_row_ids: &BTreeSet<String>,
) {
    if visible_row_ids.is_empty() || changed_row_ids.is_empty() {
        return;
    }
    if this.ai_timeline_list_row_count != visible_row_ids.len() {
        reset_ai_timeline_list_measurements(this, visible_row_ids.len());
        return;
    }

    let invalidated_indexes = visible_row_ids
        .iter()
        .enumerate()
        .filter_map(|(ix, row_id)| changed_row_ids.contains(row_id.as_str()).then_some(ix))
        .collect::<Vec<_>>();
    if invalidated_indexes.is_empty() {
        return;
    }
    if invalidated_indexes.len() == visible_row_ids.len() {
        reset_ai_timeline_list_measurements(this, visible_row_ids.len());
        return;
    }

    for ix in invalidated_indexes {
        this.ai_timeline_list_state.splice(ix..ix + 1, 1);
    }
}

fn should_sync_selected_thread_from_active_thread(
    selected_thread_id: Option<&str>,
    active_thread_id: Option<&str>,
    preserving_workspace_draft: bool,
    state: &hunk_codex::state::AiState,
) -> bool {
    if preserving_workspace_draft {
        return false;
    }
    let Some(active_thread_id) = active_thread_id else {
        return false;
    };
    let Some(active_thread) = state.threads.get(active_thread_id) else {
        return false;
    };
    if active_thread.status == ThreadLifecycleStatus::Archived {
        return false;
    }

    selected_thread_id.is_none_or(|selected| !state.threads.contains_key(selected))
}

pub(crate) fn ai_prompt_send_waiting_on_connection(
    connection_state: AiConnectionState,
    bootstrap_loading: bool,
) -> bool {
    bootstrap_loading
        || matches!(
            connection_state,
            AiConnectionState::Connecting | AiConnectionState::Reconnecting
        )
}

fn thread_metadata_refresh_key(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
) -> Option<String> {
    let thread = state.threads.get(thread_id)?;
    if thread
        .title
        .as_deref()
        .is_some_and(|title| !title.trim().is_empty())
    {
        return None;
    }

    let mut turn_states = state
        .turns
        .values()
        .filter(|turn| turn.thread_id == thread_id)
        .map(|turn| {
            let status = match turn.status {
                hunk_codex::state::TurnStatus::InProgress => "in-progress",
                hunk_codex::state::TurnStatus::Completed => "completed",
            };
            format!("{}:{status}", turn.id)
        })
        .collect::<Vec<_>>();
    if turn_states.is_empty() {
        return None;
    }

    turn_states.sort();
    Some(turn_states.join("|"))
}

fn next_thread_metadata_refresh_attempt(
    refresh_state_by_thread: &mut BTreeMap<String, AiThreadTitleRefreshState>,
    state: &hunk_codex::state::AiState,
    thread_id: &str,
    now: Instant,
) -> Option<(String, u8)> {
    let Some(thread) = state.threads.get(thread_id) else {
        refresh_state_by_thread.remove(thread_id);
        return None;
    };
    if thread
        .title
        .as_deref()
        .is_some_and(|title| !title.trim().is_empty())
    {
        if let Some(existing) = refresh_state_by_thread.remove(thread_id) {
            tracing::debug!(
                thread_id,
                attempts = existing.attempts,
                "AI thread title polling finished: title populated"
            );
        }
        return None;
    }

    let Some(refresh_key) = thread_metadata_refresh_key(state, thread_id) else {
        if let Some(existing) = refresh_state_by_thread.remove(thread_id) {
            tracing::debug!(
                thread_id,
                attempts = existing.attempts,
                "AI thread title polling finished: thread no longer eligible"
            );
        }
        return None;
    };

    match refresh_state_by_thread.get(thread_id).cloned() {
        None => Some((refresh_key, 1)),
        Some(existing) if existing.key != refresh_key => {
            refresh_state_by_thread.remove(thread_id);
            Some((refresh_key, 1))
        }
        Some(existing) if existing.in_flight => {
            if let Some(state) = refresh_state_by_thread.get_mut(thread_id) {
                state.in_flight = false;
            }
            tracing::debug!(
                thread_id,
                attempts = existing.attempts,
                "AI thread title poll completed"
            );
            None
        }
        Some(existing)
            if now.duration_since(existing.last_attempt_at) < AI_THREAD_TITLE_REFRESH_RETRY_INTERVAL =>
        {
            None
        }
        Some(existing) if existing.attempts >= AI_THREAD_TITLE_REFRESH_MAX_ATTEMPTS => {
            refresh_state_by_thread.remove(thread_id);
            tracing::debug!(
                thread_id,
                attempts = existing.attempts,
                "AI thread title polling finished: retry budget exhausted"
            );
            None
        }
        Some(existing) => Some((refresh_key, existing.attempts.saturating_add(1))),
    }
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
        .find(|candidate| {
            #[cfg(target_os = "windows")]
            {
                windows_path_is_spawnable(candidate)
            }
            #[cfg(not(target_os = "windows"))]
            {
                candidate.is_file()
            }
        })
}

#[cfg(target_os = "windows")]
fn resolve_windows_command_path(command_name: &std::path::Path) -> Option<std::path::PathBuf> {
    if is_command_name_without_path(command_name) {
        return resolve_windows_command_path_from_env(
            command_name,
            std::env::var_os("PATH"),
            std::env::var_os("PATHEXT"),
        );
    }

    resolve_windows_explicit_command_path(
        command_name,
        std::env::var_os("PATHEXT"),
    )
}

#[cfg(target_os = "windows")]
fn resolve_windows_command_path_from_env(
    command_name: &std::path::Path,
    path_var: Option<OsString>,
    pathext_var: Option<OsString>,
) -> Option<std::path::PathBuf> {
    let command_name = command_name.as_os_str();
    let path_var = path_var?;
    let candidate_names = windows_command_candidate_names(command_name, pathext_var.as_deref());
    std::env::split_paths(&path_var).find_map(|directory| {
        candidate_names
            .iter()
            .map(|candidate| directory.join(candidate))
            .find(|candidate| windows_path_is_spawnable(candidate))
    })
}

#[cfg(target_os = "windows")]
fn resolve_windows_explicit_command_path(
    command_path: &std::path::Path,
    pathext_var: Option<OsString>,
) -> Option<std::path::PathBuf> {
    if windows_path_is_spawnable(command_path) {
        return Some(command_path.to_path_buf());
    }

    let parent = command_path.parent()?;
    let file_name = command_path.file_name()?;
    let candidate_names = windows_command_candidate_names(file_name, pathext_var.as_deref());
    candidate_names
        .iter()
        .map(|candidate| parent.join(candidate))
        .find(|candidate| windows_path_is_spawnable(candidate))
}

#[cfg(target_os = "windows")]
fn windows_command_candidate_names(
    command_name: &OsStr,
    pathext_var: Option<&OsStr>,
) -> Vec<OsString> {
    let command_path = std::path::Path::new(command_name);
    if command_path.extension().is_some() {
        return vec![command_name.to_os_string()];
    }

    let mut candidates = Vec::new();
    let pathext_var = pathext_var
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsStr::new(".COM;.EXE;.BAT;.CMD"));
    for extension in pathext_var
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
    {
        let normalized = if extension.starts_with('.') {
            extension.to_string()
        } else {
            format!(".{extension}")
        };
        candidates.push(OsString::from(format!(
            "{}{}",
            command_name.to_string_lossy(),
            normalized
        )));
    }
    candidates.push(command_name.to_os_string());
    candidates
}

#[cfg(target_os = "windows")]
fn windows_path_is_spawnable(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match extension.as_deref() {
        Some("cmd" | "bat" | "com") => true,
        Some("exe") => windows_file_has_mz_header(path),
        Some(_) => false,
        None => windows_file_has_mz_header(path),
    }
}

#[cfg(target_os = "windows")]
fn windows_file_has_mz_header(path: &std::path::Path) -> bool {
    use std::io::Read as _;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 2];
    file.read_exact(&mut header).is_ok() && header == *b"MZ"
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
    let AiThreadSessionState {
        model,
        effort,
        collaboration_mode,
        service_tier,
    } = session;
    let service_tier = normalized_ai_service_tier_selection(service_tier.unwrap_or_default());
    let is_empty = model.is_none()
        && effort.is_none()
        && collaboration_mode == AiCollaborationModeSelection::Default
        && service_tier.is_none();
    if is_empty {
        return None;
    }
    Some(AiThreadSessionState {
        model,
        effort,
        collaboration_mode,
        service_tier,
    })
}

fn normalized_ai_service_tier_selection(
    selection: AiServiceTierSelection,
) -> Option<AiServiceTierSelection> {
    match selection {
        AiServiceTierSelection::Standard => None,
        other => Some(other),
    }
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
