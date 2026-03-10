use crate::app::ai_runtime::AiWorkspaceThreadCatalog;
use crate::app::ai_runtime::load_ai_workspace_thread_catalogs;

impl DiffViewer {
    pub(super) fn refresh_ai_repo_thread_catalog(&mut self, cx: &mut Context<Self>) {
        let visible_workspace_key = self.ai_workspace_key();
        let known_workspace_keys = ai_known_workspace_keys(self.workspace_targets.as_slice());
        self.prune_ai_workspace_states_for_thread_catalog(
            &known_workspace_keys,
            visible_workspace_key.as_deref(),
        );
        let refresh_epoch = self.next_ai_thread_catalog_refresh_epoch();

        let workspace_roots = ai_thread_catalog_workspace_roots(
            self.workspace_targets.as_slice(),
            visible_workspace_key.as_deref(),
        );
        if workspace_roots.is_empty() {
            self.ai_thread_catalog_task = Task::ready(());
            return;
        }

        let Some(codex_home) = crate::app::ai_paths::resolve_codex_home_path() else {
            self.ai_thread_catalog_task = Task::ready(());
            return;
        };
        let codex_executable = Self::resolve_codex_executable_path();
        if let Err(error) = Self::validate_codex_executable_path(codex_executable.as_path()) {
            debug!("skipping repo-wide AI thread catalog refresh: {error}");
            self.ai_thread_catalog_task = Task::ready(());
            return;
        }

        let expected_workspace_keys = known_workspace_keys.clone();
        self.ai_thread_catalog_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                load_ai_workspace_thread_catalogs(workspace_roots, codex_executable, codex_home)
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if this.ai_thread_catalog_refresh_epoch != refresh_epoch {
                        return;
                    }
                    if ai_known_workspace_keys(this.workspace_targets.as_slice())
                        != expected_workspace_keys
                    {
                        return;
                    }

                    match result {
                        Ok(catalogs) => {
                            this.apply_ai_repo_thread_catalogs(
                                catalogs,
                                visible_workspace_key.as_deref(),
                            );
                            cx.notify();
                        }
                        Err(error) => {
                            debug!("failed to refresh repo-wide AI thread catalog: {error:#}");
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
    ) {
        let hidden_workspace_keys = self
            .ai_hidden_runtimes
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let removable_workspace_keys = self
            .ai_workspace_states
            .keys()
            .filter(|workspace_key| {
                !known_workspace_keys.contains(workspace_key.as_str())
                    && visible_workspace_key != Some(workspace_key.as_str())
                    && !hidden_workspace_keys.contains(workspace_key.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        for workspace_key in removable_workspace_keys {
            self.ai_forget_deleted_workspace_state(workspace_key.as_str());
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

fn ai_thread_catalog_workspace_roots(
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    visible_workspace_key: Option<&str>,
) -> Vec<std::path::PathBuf> {
    let mut seen_workspace_keys = std::collections::BTreeSet::new();
    workspace_targets
        .iter()
        .filter_map(|target| {
            let workspace_key = target.root.to_string_lossy().to_string();
            if visible_workspace_key == Some(workspace_key.as_str()) {
                return None;
            }
            if !seen_workspace_keys.insert(workspace_key) {
                return None;
            }
            Some(target.root.clone())
        })
        .collect()
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
