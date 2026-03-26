use crate::app::ai_runtime::AiWorkspaceThreadCatalog;
use crate::app::ai_runtime::load_ai_workspace_thread_catalogs;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AiWorkspaceCatalogInputs {
    known_workspace_keys: std::collections::BTreeSet<String>,
    workspace_roots: Vec<std::path::PathBuf>,
}

impl DiffViewer {
    pub(super) fn refresh_ai_repo_thread_catalog(&mut self, cx: &mut Context<Self>) {
        let visible_workspace_key = self.ai_workspace_key();
        let refresh_epoch = self.next_ai_thread_catalog_refresh_epoch();
        let workspace_project_paths = ai_workspace_project_roots(
            self.state.workspace_project_paths.as_slice(),
            self.project_path.as_deref(),
            self.repo_root.as_deref(),
        );
        let active_project_path = self.project_path.clone().or_else(|| self.repo_root.clone());
        let active_workspace_targets = self.workspace_targets.clone();
        let expected_workspace_project_paths = workspace_project_paths.clone();
        let expected_active_workspace_keys = ai_known_workspace_keys(active_workspace_targets.as_slice());

        let codex_home = crate::app::ai_paths::resolve_codex_home_path();
        let codex_executable = Self::resolve_codex_executable_path();
        let codex_executable = if let Err(error) =
            Self::validate_codex_executable_path(codex_executable.as_path())
        {
            debug!("skipping workspace-wide AI thread catalog refresh: {error}");
            None
        } else {
            Some(codex_executable)
        };

        let visible_workspace_key_for_task = visible_workspace_key.clone();
        self.ai_thread_catalog_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let catalog_inputs = collect_ai_workspace_catalog_inputs(
                    workspace_project_paths.as_slice(),
                    active_project_path.as_deref(),
                    active_workspace_targets.as_slice(),
                    visible_workspace_key_for_task.as_deref(),
                );
                if catalog_inputs.workspace_roots.is_empty() {
                    return Ok((catalog_inputs, Vec::new()));
                }
                let Some(codex_home) = codex_home else {
                    return Ok((catalog_inputs, Vec::new()));
                };
                let Some(codex_executable) = codex_executable else {
                    return Ok((catalog_inputs, Vec::new()));
                };

                let catalogs = load_ai_workspace_thread_catalogs(
                    catalog_inputs.workspace_roots.clone(),
                    codex_executable,
                    codex_home,
                )?;
                Ok((catalog_inputs, catalogs))
            });
            let result: Result<
                (AiWorkspaceCatalogInputs, Vec<AiWorkspaceThreadCatalog>),
                hunk_codex::errors::CodexIntegrationError,
            > = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if this.ai_thread_catalog_refresh_epoch != refresh_epoch {
                        return;
                    }
                    if ai_workspace_project_roots(
                        this.state.workspace_project_paths.as_slice(),
                        this.project_path.as_deref(),
                        this.repo_root.as_deref(),
                    ) != expected_workspace_project_paths
                    {
                        return;
                    }
                    if ai_known_workspace_keys(this.workspace_targets.as_slice())
                        != expected_active_workspace_keys
                    {
                        return;
                    }
                    if this.ai_workspace_key() != visible_workspace_key
                    {
                        return;
                    }

                    match result {
                        Ok((catalog_inputs, catalogs)) => {
                            this.prune_ai_workspace_states_for_thread_catalog(
                                &catalog_inputs.known_workspace_keys,
                                visible_workspace_key.as_deref(),
                                cx,
                            );
                            this.apply_ai_repo_thread_catalogs(
                                catalogs,
                                visible_workspace_key.as_deref(),
                            );
                            cx.notify();
                        }
                        Err(error) => {
                            debug!("failed to refresh workspace-wide AI thread catalog: {error:#}");
                        }
                    }
                });
            }
        });
    }

    fn next_ai_thread_catalog_refresh_epoch(&mut self) -> usize {
        self.ai_thread_catalog_refresh_epoch =
            self.ai_thread_catalog_refresh_epoch.saturating_add(1);
        self.ai_thread_catalog_refresh_epoch
    }

    pub(super) fn invalidate_ai_thread_catalog_refresh(&mut self) {
        self.ai_thread_catalog_refresh_epoch =
            self.ai_thread_catalog_refresh_epoch.saturating_add(1);
        self.ai_thread_catalog_task = Task::ready(());
    }

    fn apply_ai_repo_thread_catalogs(
        &mut self,
        catalogs: Vec<AiWorkspaceThreadCatalog>,
        visible_workspace_key: Option<&str>,
    ) {
        for catalog in catalogs {
            if visible_workspace_key == Some(catalog.workspace_key.as_str()) {
                continue;
            }
            if self
                .ai_hidden_runtimes
                .contains_key(catalog.workspace_key.as_str())
            {
                continue;
            }

            let workspace_key = catalog.workspace_key.clone();
            let mut state = self
                .ai_workspace_states
                .remove(workspace_key.as_str())
                .unwrap_or_else(|| {
                    self.default_ai_workspace_state_for_workspace_key(Some(workspace_key.as_str()))
                });
            apply_ai_thread_catalog_to_workspace_state(&mut state, catalog);
            self.ai_workspace_states.insert(workspace_key, state);
        }
    }

    fn prune_ai_workspace_states_for_thread_catalog(
        &mut self,
        known_workspace_keys: &std::collections::BTreeSet<String>,
        visible_workspace_key: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let removable_workspace_keys = self
            .ai_workspace_states
            .keys()
            .filter(|workspace_key| {
                !known_workspace_keys.contains(workspace_key.as_str())
                    && visible_workspace_key != Some(workspace_key.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        for workspace_key in removable_workspace_keys {
            self.shutdown_ai_runtime_for_workspace_blocking(workspace_key.as_str());
            self.ai_forget_deleted_workspace_state(workspace_key.as_str(), cx);
        }
    }
}

fn ai_known_workspace_keys(
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
) -> std::collections::BTreeSet<String> {
    workspace_targets
        .iter()
        .map(|target| target.root.to_string_lossy().to_string())
        .collect()
}

#[cfg(test)]
fn ai_thread_catalog_workspace_roots(
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    visible_workspace_key: Option<&str>,
) -> Vec<std::path::PathBuf> {
    ai_workspace_catalog_inputs_from_target_sets(
        &[workspace_targets.to_vec()],
        &[],
        visible_workspace_key,
    )
    .workspace_roots
}

fn collect_ai_workspace_catalog_inputs(
    workspace_project_paths: &[std::path::PathBuf],
    active_project_path: Option<&std::path::Path>,
    active_workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    visible_workspace_key: Option<&str>,
) -> AiWorkspaceCatalogInputs {
    let mut workspace_target_sets = Vec::with_capacity(workspace_project_paths.len());
    let mut fallback_project_roots = Vec::new();

    for project_root in workspace_project_paths {
        if active_project_path == Some(project_root.as_path()) {
            if active_workspace_targets.is_empty() {
                fallback_project_roots.push(project_root.clone());
            } else {
                workspace_target_sets.push(active_workspace_targets.to_vec());
            }
            continue;
        }

        match hunk_git::worktree::list_workspace_targets(project_root.as_path()) {
            Ok(targets) if !targets.is_empty() => workspace_target_sets.push(targets),
            Ok(_) => {
                fallback_project_roots.push(project_root.clone());
            }
            Err(error) => {
                debug!(
                    "failed to list workspace targets for AI catalog refresh on {}: {error:#}",
                    project_root.display()
                );
                fallback_project_roots.push(project_root.clone());
            }
        }
    }

    ai_workspace_catalog_inputs_from_target_sets(
        workspace_target_sets.as_slice(),
        fallback_project_roots.as_slice(),
        visible_workspace_key,
    )
}

fn ai_workspace_catalog_inputs_from_target_sets(
    workspace_target_sets: &[Vec<hunk_git::worktree::WorkspaceTargetSummary>],
    fallback_project_roots: &[std::path::PathBuf],
    visible_workspace_key: Option<&str>,
) -> AiWorkspaceCatalogInputs {
    let mut inputs = AiWorkspaceCatalogInputs::default();

    for workspace_targets in workspace_target_sets {
        for target in workspace_targets {
            register_ai_workspace_root_for_catalog(
                &mut inputs,
                target.root.as_path(),
                visible_workspace_key,
            );
        }
    }

    for project_root in fallback_project_roots {
        register_ai_workspace_root_for_catalog(
            &mut inputs,
            project_root.as_path(),
            visible_workspace_key,
        );
    }

    inputs
}

fn register_ai_workspace_root_for_catalog(
    inputs: &mut AiWorkspaceCatalogInputs,
    workspace_root: &std::path::Path,
    visible_workspace_key: Option<&str>,
) {
    let workspace_key = workspace_root.to_string_lossy().to_string();
    inputs.known_workspace_keys.insert(workspace_key.clone());

    if visible_workspace_key == Some(workspace_key.as_str()) {
        return;
    }
    if inputs.workspace_roots.iter().any(|root| root == workspace_root) {
        return;
    }

    inputs.workspace_roots.push(workspace_root.to_path_buf());
}

fn apply_ai_thread_catalog_to_workspace_state(
    state: &mut AiWorkspaceState,
    catalog: AiWorkspaceThreadCatalog,
) {
    state.connection_state = AiConnectionState::Disconnected;
    state.bootstrap_loading = false;
    state.status_message = None;
    state.error_message = None;
    state.state_snapshot = catalog.state_snapshot;
    state.pending_approvals.clear();
    state.pending_user_inputs.clear();
    state.pending_user_input_answers.clear();
    state.in_progress_turn_started_at.clear();
    state.expanded_timeline_row_ids.clear();

    if state.pending_new_thread_selection
        && let Some(active_thread_id) = catalog.active_thread_id.as_deref()
        && state
            .state_snapshot
            .threads
            .get(active_thread_id)
            .is_some_and(|thread| thread.status != ThreadLifecycleStatus::Archived)
    {
        state.new_thread_draft_active = false;
        state.pending_new_thread_selection = false;
        state.selected_thread_id = Some(active_thread_id.to_string());
    }

    if state.selected_thread_id.as_ref().is_some_and(|selected| {
        state
            .state_snapshot
            .threads
            .get(selected)
            .is_none_or(|thread| thread.status == ThreadLifecycleStatus::Archived)
    }) {
        state.selected_thread_id = None;
    }

    if !state.new_thread_draft_active
        && !state.pending_new_thread_selection
        && state.selected_thread_id.is_none()
    {
        state.selected_thread_id = catalog.active_thread_id;
    }

    if !state.new_thread_draft_active
        && !state.pending_new_thread_selection
        && state.selected_thread_id.is_none()
        && let Some(first_thread) = sorted_threads(&state.state_snapshot).first()
    {
        state.selected_thread_id = Some(first_thread.id.clone());
    }

    state
        .thread_title_refresh_state_by_thread
        .retain(|thread_id, _| state.state_snapshot.threads.contains_key(thread_id));
    state
        .timeline_visible_turn_limit_by_thread
        .retain(|thread_id, _| state.state_snapshot.threads.contains_key(thread_id));
}
