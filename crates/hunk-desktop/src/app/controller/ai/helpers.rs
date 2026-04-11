#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(target_os = "windows")]
use std::ffi::OsString;
use std::collections::BTreeSet;

use codex_app_server_protocol::CollaborationModeMask;
use codex_protocol::config_types::ModeKind;
use hunk_git::git::LocalBranch;

const AI_THREAD_SIDEBAR_DEFAULT_VISIBLE_THREADS_PER_PROJECT: usize = 5;

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

fn state_snapshot_workspace_key(
    state_snapshot: &hunk_codex::state::AiState,
    selected_thread_id: Option<&str>,
    worker_workspace_key: Option<&str>,
    draft_workspace_key: Option<&str>,
    new_thread_draft_active: bool,
    pending_new_thread_selection: bool,
) -> Option<String> {
    if let Some(selected_thread_id) = selected_thread_id
        && let Some(thread) = state_snapshot.threads.get(selected_thread_id)
        && thread.status != ThreadLifecycleStatus::Archived
    {
        return Some(thread.cwd.clone());
    }

    let mut thread_workspace_keys = state_snapshot
        .threads
        .values()
        .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
        .map(|thread| thread.cwd.as_str())
        .collect::<BTreeSet<_>>();
    if thread_workspace_keys.len() == 1 {
        return thread_workspace_keys
            .pop_first()
            .map(std::string::ToString::to_string);
    }

    if new_thread_draft_active || pending_new_thread_selection {
        return draft_workspace_key.map(std::string::ToString::to_string);
    }

    worker_workspace_key.map(std::string::ToString::to_string)
}

fn merged_ai_visible_threads(
    state_snapshot: &hunk_codex::state::AiState,
    state_snapshot_workspace_key: Option<&str>,
    workspace_states: &std::collections::BTreeMap<String, AiWorkspaceState>,
    workspace_project_paths: &[std::path::PathBuf],
    project_path: Option<&std::path::Path>,
    repo_root: Option<&std::path::Path>,
) -> Vec<ThreadSummary> {
    let mut threads_by_id = std::collections::BTreeMap::<String, ThreadSummary>::new();
    let workspace_project_roots =
        ai_workspace_project_roots(workspace_project_paths, project_path, repo_root);
    let chats_root = crate::app::ai_paths::resolve_ai_chats_root_path();

    for thread in state_snapshot
        .threads
        .values()
        .filter(|thread| {
            thread.status != ThreadLifecycleStatus::Archived
                && ai_workspace_section_root_for_thread_root(
                    std::path::Path::new(thread.cwd.as_str()),
                    workspace_project_roots.as_slice(),
                    chats_root.as_deref(),
                )
                .is_some()
        })
    {
        threads_by_id.insert(thread.id.clone(), thread.clone());
    }

    for (workspace_key, state) in workspace_states {
        if state_snapshot_workspace_key == Some(workspace_key.as_str())
            || ai_workspace_section_root_for_thread_root(
                std::path::Path::new(workspace_key.as_str()),
                workspace_project_roots.as_slice(),
                chats_root.as_deref(),
            )
            .is_none()
        {
            continue;
        }
        for thread in state
            .state_snapshot
            .threads
            .values()
            .filter(|thread| {
                thread.status != ThreadLifecycleStatus::Archived
                    && ai_workspace_section_root_for_thread_root(
                        std::path::Path::new(thread.cwd.as_str()),
                        workspace_project_roots.as_slice(),
                        chats_root.as_deref(),
                    )
                    .is_some()
            })
        {
            let replace_existing = threads_by_id
                .get(thread.id.as_str())
                .is_none_or(|existing| {
                    (thread.updated_at, thread.created_at, thread.id.as_str())
                        > (existing.updated_at, existing.created_at, existing.id.as_str())
                });
            if replace_existing {
                threads_by_id.insert(thread.id.clone(), thread.clone());
            }
        }
    }

    let mut threads = threads_by_id.into_values().collect::<Vec<_>>();
    threads.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    threads
}

fn ai_workspace_project_roots(
    workspace_project_paths: &[std::path::PathBuf],
    project_path: Option<&std::path::Path>,
    repo_root: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    let mut project_roots = workspace_project_paths.to_vec();

    if let Some(active_project_root) = project_path
        .or(repo_root)
        .and_then(ai_workspace_project_root_identity)
        .filter(|root| !project_roots.iter().any(|candidate| candidate == root))
    {
        project_roots.push(active_project_root);
    }

    project_roots
}

fn ai_workspace_project_root_for_thread_root(
    thread_workspace_root: &std::path::Path,
    workspace_project_roots: &[std::path::PathBuf],
) -> Option<std::path::PathBuf> {
    if let Some(thread_primary_root) = ai_workspace_project_root_identity(thread_workspace_root)
        && let Some(project_root) = workspace_project_roots
            .iter()
            .find(|project_root| project_root.as_path() == thread_primary_root.as_path())
    {
        return Some(project_root.clone());
    }

    workspace_project_roots
        .iter()
        .filter(|project_root| {
            thread_workspace_root == project_root.as_path()
                || thread_workspace_root.starts_with(project_root.as_path())
        })
        .max_by_key(|project_root| project_root.components().count())
        .cloned()
}

fn ai_workspace_section_root_for_thread_root(
    thread_workspace_root: &std::path::Path,
    workspace_project_roots: &[std::path::PathBuf],
    chats_root: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    if chats_root == Some(thread_workspace_root) {
        return chats_root.map(std::path::Path::to_path_buf);
    }

    ai_workspace_project_root_for_thread_root(thread_workspace_root, workspace_project_roots)
}

fn resolved_ai_workspace_kind_for_root(
    workspace_root: Option<&std::path::Path>,
    chats_root: Option<&std::path::Path>,
) -> AiWorkspaceKind {
    if workspace_root.is_some() && workspace_root == chats_root {
        AiWorkspaceKind::Chats
    } else {
        AiWorkspaceKind::Project
    }
}

fn is_ai_chats_workspace_key(workspace_key: Option<&str>) -> bool {
    let chats_key = crate::app::ai_paths::resolve_ai_chats_root_path()
        .map(|path| path.to_string_lossy().to_string());
    workspace_key.is_some_and(|workspace_key| chats_key.as_deref() == Some(workspace_key))
}

fn ai_visible_thread_sections(
    threads: Vec<ThreadSummary>,
    workspace_project_paths: &[std::path::PathBuf],
    project_path: Option<&std::path::Path>,
    repo_root: Option<&std::path::Path>,
    expanded_project_roots: &BTreeSet<String>,
) -> Vec<AiVisibleThreadProjectSection> {
    let project_roots = ai_workspace_project_roots(workspace_project_paths, project_path, repo_root);
    let chats_root = crate::app::ai_paths::resolve_ai_chats_root_path();
    let mut threads_by_project = std::collections::BTreeMap::<
        std::path::PathBuf,
        Vec<ThreadSummary>,
    >::new();

    for thread in threads {
        if let Some(project_root) = ai_workspace_section_root_for_thread_root(
            std::path::Path::new(thread.cwd.as_str()),
            project_roots.as_slice(),
            chats_root.as_deref(),
        )
        {
            threads_by_project.entry(project_root).or_default().push(thread);
        }
    }

    let mut sections = Vec::new();

    if let Some(chats_root) = chats_root {
        let expanded = expanded_project_roots.contains(chats_root.to_string_lossy().as_ref());
        let all_threads = threads_by_project.remove(&chats_root).unwrap_or_default();
        let total_thread_count = all_threads.len();
        let visible_threads = if expanded {
            all_threads.clone()
        } else {
            all_threads
                .iter()
                .take(AI_THREAD_SIDEBAR_DEFAULT_VISIBLE_THREADS_PER_PROJECT)
                .cloned()
                .collect()
        };

        sections.push(AiVisibleThreadProjectSection {
            workspace_kind: AiWorkspaceKind::Chats,
            project_label: "Chats".to_string(),
            hidden_thread_count: total_thread_count.saturating_sub(visible_threads.len()),
            total_thread_count,
            expanded,
            project_root: chats_root,
            threads: visible_threads,
        });
    }

    sections.extend(project_roots.into_iter().map(|project_root| {
            let expanded = expanded_project_roots
                .contains(project_root.to_string_lossy().as_ref());
            let all_threads = threads_by_project.remove(&project_root).unwrap_or_default();
            let total_thread_count = all_threads.len();
            let visible_threads = if expanded {
                all_threads.clone()
            } else {
                all_threads
                    .iter()
                    .take(AI_THREAD_SIDEBAR_DEFAULT_VISIBLE_THREADS_PER_PROJECT)
                    .cloned()
                    .collect()
            };

            AiVisibleThreadProjectSection {
                workspace_kind: AiWorkspaceKind::Project,
                project_label: crate::app::project_picker::project_display_name(
                    project_root.as_path(),
                ),
                hidden_thread_count: total_thread_count.saturating_sub(visible_threads.len()),
                total_thread_count,
                expanded,
                project_root,
                threads: visible_threads,
            }
        }));

    sections
}

pub(crate) const AI_AUTH_REQUIRED_MESSAGE: &str =
    "Codex sign-in expired. Sign in to talk to the AI Gods.";

pub(crate) fn ai_auth_required_message(
    account: Option<&codex_app_server_protocol::Account>,
    requires_openai_auth: bool,
    pending_chatgpt_login_id: Option<&str>,
) -> Option<String> {
    if pending_chatgpt_login_id.is_some() {
        return None;
    }

    (requires_openai_auth && account.is_none()).then(|| AI_AUTH_REQUIRED_MESSAGE.to_string())
}

pub(crate) fn ai_prominent_worker_status_error(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    if lower.starts_with("chatgpt login failed") {
        return Some(message.to_string());
    }

    if lower.contains("unable to read account state")
        && (lower.contains("401")
            || lower.contains("unauthorized")
            || lower.contains("token")
            || lower.contains("sign in again")
            || lower.contains("refresh"))
    {
        return Some(AI_AUTH_REQUIRED_MESSAGE.to_string());
    }

    None
}

fn ai_workspace_project_root_identity(path: &std::path::Path) -> Option<std::path::PathBuf> {
    hunk_git::worktree::primary_repo_root(path)
        .ok()
        .or_else(|| Some(path.to_path_buf()))
}

fn workspace_mad_max_mode(state: &AppState, workspace_key: Option<&str>) -> bool {
    workspace_key
        .and_then(|workspace| state.ai_workspace_mad_max.get(workspace))
        .copied()
        .unwrap_or(true)
}

fn set_workspace_mad_max_mode(state: &mut AppState, workspace_key: &str, enabled: bool) {
    if enabled {
        state.ai_workspace_mad_max.remove(workspace_key);
    } else {
        state
            .ai_workspace_mad_max
            .insert(workspace_key.to_string(), false);
    }
}

fn workspace_include_hidden_models(state: &AppState, workspace_key: Option<&str>) -> bool {
    workspace_key
        .and_then(|workspace| state.ai_workspace_include_hidden_models.get(workspace))
        .copied()
        .unwrap_or(true)
}

fn set_workspace_include_hidden_models(state: &mut AppState, workspace_key: &str, enabled: bool) {
    if enabled {
        state.ai_workspace_include_hidden_models.remove(workspace_key);
    } else {
        state
            .ai_workspace_include_hidden_models
            .insert(workspace_key.to_string(), false);
    }
}

fn seed_ai_workspace_preferences(
    state: &mut AppState,
    workspace_key: &str,
    mad_max_mode: bool,
    include_hidden_models: bool,
) {
    set_workspace_mad_max_mode(state, workspace_key, mad_max_mode);
    set_workspace_include_hidden_models(state, workspace_key, include_hidden_models);
}

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

fn ai_thread_workspace_matches_current_project(
    thread_workspace_root: &std::path::Path,
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    project_path: Option<&std::path::Path>,
    repo_root: Option<&std::path::Path>,
) -> bool {
    if workspace_targets
        .iter()
        .any(|target| target.root.as_path() == thread_workspace_root)
    {
        return true;
    }

    let visible_workspace_root = resolved_ai_workspace_cwd(project_path, repo_root);
    if visible_workspace_root.as_deref() == Some(thread_workspace_root) {
        return true;
    }

    let current_primary_root = repo_root
        .map(std::path::Path::to_path_buf)
        .or_else(|| {
            visible_workspace_root
                .as_deref()
                .and_then(|root| hunk_git::worktree::primary_repo_root(root).ok())
        });
    let Some(current_primary_root) = current_primary_root else {
        return false;
    };

    hunk_git::worktree::primary_repo_root(thread_workspace_root)
        .is_ok_and(|thread_primary_root| thread_primary_root == current_primary_root)
}

fn ai_thread_start_mode_for_workspace(
    repo_root: Option<&std::path::Path>,
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    thread_cwd: &std::path::Path,
) -> Option<AiNewThreadStartMode> {
    if crate::app::ai_paths::resolve_ai_chats_root_path().as_deref() == Some(thread_cwd) {
        return None;
    }

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

    hunk_git::worktree::primary_repo_root(thread_cwd)
        .ok()
        .map(|primary_root| {
            if primary_root.as_path() == thread_cwd {
                AiNewThreadStartMode::Local
            } else {
                AiNewThreadStartMode::Worktree
            }
        })
        .or_else(|| {
            repo_root.map(|repo_root| {
                if repo_root == thread_cwd {
                    AiNewThreadStartMode::Local
                } else {
                    AiNewThreadStartMode::Worktree
                }
            })
        })
}

fn resolved_ai_thread_mode_picker_state(
    selected_thread_start_mode: Option<AiNewThreadStartMode>,
    draft_start_mode: AiNewThreadStartMode,
    new_thread_draft_active: bool,
    pending_new_thread_selection: bool,
) -> (AiNewThreadStartMode, bool) {
    let editable = new_thread_draft_active && !pending_new_thread_selection;
    let selected = if new_thread_draft_active || pending_new_thread_selection {
        draft_start_mode
    } else {
        selected_thread_start_mode.unwrap_or(draft_start_mode)
    };
    (selected, editable)
}

fn preferred_ai_worktree_base_branch_name(
    branches: &[LocalBranch],
    preferred_branch_name: Option<&str>,
    current_branch_name: Option<&str>,
) -> Option<String> {
    let branch_exists = |candidate: &str| branches.iter().any(|branch| branch.name == candidate);

    preferred_branch_name
        .filter(|candidate| branch_exists(candidate))
        .map(ToOwned::to_owned)
        .or_else(|| {
            ["main", "master"]
                .into_iter()
                .find(|candidate| branch_exists(candidate))
                .map(str::to_string)
        })
        .or_else(|| {
            current_branch_name
                .filter(|candidate| branch_exists(candidate))
                .map(ToOwned::to_owned)
        })
        .or_else(|| branches.first().map(|branch| branch.name.clone()))
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

fn ai_state_has_user_message_for_thread(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
) -> bool {
    state
        .items
        .values()
        .any(|item| item.thread_id == thread_id && item.kind == "userMessage")
}

fn reasoning_effort_key(effort: &codex_protocol::openai_models::ReasoningEffort) -> String {
    serde_json::to_value(effort)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{effort:?}").to_lowercase())
}

fn normalized_ai_session_selection(
    models: &[codex_app_server_protocol::Model],
    selected_model: Option<String>,
    selected_effort: Option<String>,
) -> (Option<String>, Option<String>) {
    if models.is_empty() {
        let selected_effort = selected_model.as_ref().and(selected_effort);
        return (selected_model, selected_effort);
    }

    let selected_model =
        selected_model.filter(|model_id| models.iter().any(|model| model.id == *model_id));
    let selected_effort = selected_model.as_ref().and_then(|model_id| {
        selected_effort.filter(|effort| {
            models
                .iter()
                .find(|model| model.id == *model_id)
                .is_some_and(|model| {
                    model.supported_reasoning_efforts.iter().any(|option| {
                        reasoning_effort_key(&option.reasoning_effort) == *effort
                    })
                })
        })
    });
    (selected_model, selected_effort)
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
        AiTimelineRowSource::TurnPlan { turn_key } => state
            .turn_plans
            .get(turn_key.as_str())
            .is_some_and(|plan| {
                plan.explanation
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                    || !plan.steps.is_empty()
            }),
    }
}

fn ai_timeline_row_is_renderable_for_controller(this: &DiffViewer, row: &AiTimelineRow) -> bool {
    match &row.source {
        AiTimelineRowSource::Item { .. }
        | AiTimelineRowSource::TurnDiff { .. }
        | AiTimelineRowSource::TurnPlan { .. } => {
            ai_timeline_row_is_renderable_for_layout(&this.ai_state_snapshot, row)
        }
        AiTimelineRowSource::Group { group_id } => this
            .ai_timeline_group(group_id.as_str())
            .is_some_and(|group| {
                group.child_row_ids.iter().any(|child_row_id| {
                    this.ai_timeline_row(child_row_id.as_str())
                        .is_some_and(|child_row| {
                            ai_timeline_row_is_renderable_for_controller(this, child_row)
                        })
                })
            }),
    }
}

fn current_ai_renderable_visible_row_ids(this: &DiffViewer, thread_id: &str) -> Vec<String> {
    let (_, _, _, visible_row_ids) = this.ai_timeline_visible_rows_for_thread(thread_id);
    visible_row_ids
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
        if previous_state.turn_plans.get(turn_key.as_str())
            != next_state.turn_plans.get(turn_key.as_str())
        {
            changed_row_ids.insert(format!("turn-plan:{turn_key}"));
        }
    }

    changed_row_ids
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

fn current_visible_thread_fallback_workspace_key(
    visible_workspace_key: Option<&str>,
    selected_thread_workspace_root: Option<&std::path::Path>,
    draft_workspace_key: Option<&str>,
) -> Option<String> {
    visible_workspace_key
        .map(ToOwned::to_owned)
        .or_else(|| {
            selected_thread_workspace_root.map(|workspace_root| {
                workspace_root.to_string_lossy().to_string()
            })
        })
        .or_else(|| draft_workspace_key.map(ToOwned::to_owned))
}

fn pending_new_thread_selection_target_id<'a>(
    pending_thread_start: Option<&'a AiPendingThreadStart>,
    active_thread_id: Option<&'a str>,
) -> Option<&'a str> {
    pending_thread_start
        .and_then(|pending| pending.thread_id.as_deref())
        .or(active_thread_id.filter(|_| pending_thread_start.is_none()))
}

fn pending_new_thread_selection_ready_thread_id(
    pending_new_thread_selection: bool,
    pending_thread_start: Option<&AiPendingThreadStart>,
    active_thread_id: Option<&str>,
    state: &hunk_codex::state::AiState,
) -> Option<String> {
    if !pending_new_thread_selection {
        return None;
    }

    let thread_id =
        pending_new_thread_selection_target_id(pending_thread_start, active_thread_id)?;
    state
        .threads
        .get(thread_id)
        .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
        .map(|_| thread_id.to_string())
}

fn set_pending_thread_start_thread_id(
    pending_thread_start: &mut Option<AiPendingThreadStart>,
    thread_id: String,
) {
    if let Some(pending) = pending_thread_start.as_mut() {
        pending.thread_id = Some(thread_id);
    }
}

fn current_visible_thread_id_from_snapshot(
    state: &hunk_codex::state::AiState,
    selected_thread_id: Option<&str>,
    workspace_key: Option<&str>,
    preserving_workspace_draft: bool,
) -> Option<String> {
    if preserving_workspace_draft {
        return None;
    }

    if let Some(selected_thread_id) = selected_thread_id
        && state
            .threads
            .get(selected_thread_id)
            .is_some_and(|thread| thread.status != ThreadLifecycleStatus::Archived)
    {
        return Some(selected_thread_id.to_string());
    }

    let workspace_key = workspace_key?;
    let active_thread_id = state.active_thread_for_cwd(workspace_key)?;
    state
        .threads
        .get(active_thread_id)
        .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
        .map(|_| active_thread_id.to_string())
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
    let turn_plan_sequences = state
        .turn_plans
        .values()
        .filter(|plan| plan.thread_id == thread_id)
        .map(|plan| plan.last_sequence);

    turn_sequences
        .chain(item_sequences)
        .chain(turn_plan_sequences)
        .max()
        .map_or(thread_sequence, |max_sequence| max_sequence.max(thread_sequence))
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

fn resolved_ai_thread_session_state(
    state: &AppState,
    thread_id: Option<&str>,
    workspace_key: Option<&str>,
) -> AiThreadSessionState {
    let mut resolved = thread_id
        .and_then(|thread_id| state.ai_thread_session_overrides.get(thread_id).cloned())
        .or_else(|| {
            workspace_key.and_then(|workspace| {
                state.ai_workspace_session_overrides.get(workspace).cloned()
            })
        })
        .unwrap_or_else(AiThreadSessionState::preferred_defaults);

    if is_ai_chats_workspace_key(workspace_key) {
        resolved.collaboration_mode = AiCollaborationModeSelection::Default;
    }

    resolved
}

fn resolved_ai_turn_session_overrides(
    state: &AppState,
    models: &[codex_app_server_protocol::Model],
    thread_id: Option<&str>,
    workspace_key: Option<&str>,
) -> AiTurnSessionOverrides {
    let session = resolved_ai_thread_session_state(state, thread_id, workspace_key);
    let (model, effort) = normalized_ai_session_selection(models, session.model, session.effort);

    AiTurnSessionOverrides {
        model,
        effort,
        collaboration_mode: session.collaboration_mode,
        service_tier: session.service_tier.unwrap_or_default(),
    }
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
