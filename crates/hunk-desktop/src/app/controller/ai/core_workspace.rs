impl DiffViewer {
    fn resolve_ai_current_state(&self) -> AiResolvedCurrentState {
        let selected_thread_workspace_root =
            self.ai_selected_thread_id.as_deref().and_then(|thread_id| {
                self.ai_state_snapshot
                    .threads
                    .get(thread_id)
                    .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
                    .map(|thread| std::path::PathBuf::from(thread.cwd.as_str()))
                    .or_else(|| self.ai_thread_workspace_root(thread_id))
            });
        let fallback_workspace_key = current_visible_thread_fallback_workspace_key(
            self.ai_worker_workspace_key.as_deref(),
            selected_thread_workspace_root.as_deref(),
            self.ai_workspace_key_for_draft().as_deref(),
        );
        let current_thread_id = current_visible_thread_id_from_snapshot(
            &self.ai_state_snapshot,
            self.ai_selected_thread_id.as_deref(),
            fallback_workspace_key.as_deref(),
            self.ai_new_thread_draft_active || self.ai_pending_new_thread_selection,
        );
        let current_thread_workspace_root = current_thread_id
            .as_deref()
            .and_then(|thread_id| {
                self.ai_state_snapshot
                    .threads
                    .get(thread_id)
                    .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
                    .map(|thread| std::path::PathBuf::from(thread.cwd.as_str()))
            })
            .or_else(|| selected_thread_workspace_root.clone());
        let workspace_root =
            if self.ai_new_thread_draft_active || self.ai_pending_new_thread_selection {
                self.ai_draft_workspace_root()
            } else {
                current_thread_workspace_root
                    .clone()
                    .or_else(|| fallback_workspace_key.as_deref().map(std::path::PathBuf::from))
                    .or_else(|| self.ai_draft_workspace_root())
            };
        let workspace_key = workspace_root
            .as_ref()
            .map(|cwd| cwd.to_string_lossy().to_string());

        AiResolvedCurrentState {
            current_thread_id,
            current_thread_workspace_root,
            workspace_root,
            workspace_key,
        }
    }

    pub(super) fn current_ai_thread_id(&self) -> Option<String> {
        self.resolve_ai_current_state().current_thread_id
    }

    pub(crate) fn ai_pending_thread_start_for_timeline_with_context(
        &self,
        current_thread_id: Option<&str>,
        workspace_key: Option<&str>,
    ) -> Option<AiPendingThreadStart> {
        let pending = self.ai_pending_thread_start.clone()?;
        if workspace_key != Some(pending.workspace_key.as_str()) {
            return None;
        }
        if let Some(thread_id) = pending.thread_id.as_deref() {
            if ai_state_has_user_message_for_thread(&self.ai_state_snapshot, thread_id) {
                return None;
            }
            if current_thread_id.is_some_and(|selected_thread_id| selected_thread_id != thread_id) {
                return None;
            }
        } else if current_thread_id.is_some() {
            return None;
        }
        Some(pending)
    }

    pub(super) fn current_ai_in_progress_turn_id(&self, thread_id: &str) -> Option<String> {
        self.ai_state_snapshot
            .turns
            .values()
            .filter(|turn| turn.thread_id == thread_id && turn.status == TurnStatus::InProgress)
            .max_by_key(|turn| turn.last_sequence)
            .map(|turn| turn.id.clone())
    }

    fn ai_draft_workspace_root(&self) -> Option<std::path::PathBuf> {
        if let Some(workspace_root) = self.ai_draft_workspace_root_override.clone() {
            return Some(workspace_root);
        }

        if let Some(target_id) = self.ai_draft_workspace_target_id.as_deref()
            && let Some(target) = self
                .workspace_targets
                .iter()
                .find(|target| target.id == target_id)
        {
            return Some(target.root.clone());
        }

        self.primary_repo_root().or_else(|| self.ai_chats_root_path())
    }

    fn resolve_ai_default_worktree_base_branch_name(&self) -> Option<String> {
        let repo_root = self.primary_repo_root()?;
        let resolved_default_base_branch = resolve_default_base_branch_name(repo_root.as_path())
            .ok()
            .flatten();
        preferred_ai_worktree_base_branch_name(
            &self.branches,
            resolved_default_base_branch.as_deref(),
            self.primary_checked_out_branch_name()
                .or(Some(self.branch_name.as_str())),
        )
    }

    fn sync_ai_worktree_base_branch_from_repo(&mut self) {
        if self.ai_new_thread_start_mode != AiNewThreadStartMode::Worktree {
            self.ai_worktree_base_branch_name = None;
            return;
        }

        let resolved_default_base_branch = self.resolve_ai_default_worktree_base_branch_name();
        self.ai_worktree_base_branch_name = preferred_ai_worktree_base_branch_name(
            &self.branches,
            self.ai_worktree_base_branch_name
                .as_deref()
                .or(resolved_default_base_branch.as_deref()),
            self.primary_checked_out_branch_name()
                .or(Some(self.branch_name.as_str())),
        );
    }

    fn ai_select_worktree_base_branch(&mut self, branch_name: String, cx: &mut Context<Self>) {
        if self.ai_new_thread_start_mode != AiNewThreadStartMode::Worktree {
            return;
        }
        let branch_name = branch_name.trim().to_string();
        if branch_name.is_empty() {
            return;
        }
        self.ai_worktree_base_branch_name = Some(branch_name);
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.invalidate_ai_visible_frame_state_with_reason("workspace");
        cx.notify();
    }

    pub(crate) fn ai_selected_worktree_base_branch_name(&self) -> Option<&str> {
        if self.ai_new_thread_start_mode != AiNewThreadStartMode::Worktree {
            return None;
        }

        self.ai_worktree_base_branch_name.as_deref()
    }

    pub(crate) fn ai_show_worktree_base_branch_picker(&self) -> bool {
        self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_new_thread_start_mode == AiNewThreadStartMode::Worktree
    }

    fn ai_workspace_key_for_draft(&self) -> Option<String> {
        self.ai_draft_workspace_root()
            .map(|path| path.to_string_lossy().to_string())
    }

    pub(crate) fn ai_workspace_cwd(&self) -> Option<std::path::PathBuf> {
        self.resolve_ai_current_state().workspace_root
    }

    pub(crate) fn ai_visible_project_root(&self) -> Option<std::path::PathBuf> {
        let resolved = self.resolve_ai_current_state();
        self.ai_visible_project_root_with_context(
            resolved.current_thread_id.as_deref(),
            resolved.workspace_root.as_deref(),
        )
    }

    pub(crate) fn ai_visible_project_root_with_context(
        &self,
        current_thread_id: Option<&str>,
        workspace_root: Option<&std::path::Path>,
    ) -> Option<std::path::PathBuf> {
        if let Some(thread_id) = current_thread_id {
            return self.ai_thread_project_root(thread_id);
        }

        let workspace_root = workspace_root?;
        if self.ai_workspace_kind_for_root(workspace_root) == AiWorkspaceKind::Chats {
            return None;
        }
        let workspace_project_roots = ai_workspace_project_roots(
            self.state.workspace_project_paths.as_slice(),
            self.project_path.as_deref(),
            self.repo_root.as_deref(),
        );
        ai_workspace_project_root_for_thread_root(
            workspace_root,
            workspace_project_roots.as_slice(),
        )
    }

    fn ai_workspace_key(&self) -> Option<String> {
        self.resolve_ai_current_state().workspace_key
    }

    pub(crate) fn ai_chats_root_path(&self) -> Option<std::path::PathBuf> {
        crate::app::ai_paths::resolve_ai_chats_root_path()
    }

    pub(crate) fn ai_workspace_kind_for_root(
        &self,
        workspace_root: &std::path::Path,
    ) -> AiWorkspaceKind {
        resolved_ai_workspace_kind_for_root(
            Some(workspace_root),
            self.ai_chats_root_path().as_deref(),
        )
    }

    pub(crate) fn current_ai_workspace_kind(&self) -> AiWorkspaceKind {
        let resolved = self.resolve_ai_current_state();
        resolved_ai_workspace_kind_for_root(
            resolved.workspace_root.as_deref(),
            self.ai_chats_root_path().as_deref(),
        )
    }

    fn sync_ai_workspace_target_from_catalog(&mut self, _: &mut Context<Self>) {
        if let Some(workspace_root) = self.ai_draft_workspace_root_override.as_deref() {
            let next_target_id = self
                .workspace_targets
                .iter()
                .find(|target| target.root.as_path() == workspace_root)
                .map(|target| target.id.clone());
            if self.ai_draft_workspace_target_id != next_target_id {
                self.ai_draft_workspace_target_id = next_target_id;
            }
            return;
        }

        let next_target_id = self
            .ai_draft_workspace_target_id
            .clone()
            .filter(|target_id| {
                self.workspace_targets
                    .iter()
                    .any(|target| target.id == *target_id)
            })
            .or_else(|| self.primary_workspace_target_id())
            .or_else(|| {
                self.workspace_targets
                    .first()
                    .map(|target| target.id.clone())
            });
        if self.ai_draft_workspace_target_id != next_target_id {
            self.ai_draft_workspace_target_id = next_target_id;
        }
    }

    pub(crate) fn ai_active_workspace_label_with_root(
        &self,
        workspace_root: Option<&std::path::Path>,
    ) -> String {
        if self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_new_thread_start_mode == AiNewThreadStartMode::Worktree
        {
            return "New Worktree".to_string();
        }

        let Some(workspace_root) = workspace_root else {
            return "Primary Checkout".to_string();
        };

        if self.ai_workspace_kind_for_root(workspace_root) == AiWorkspaceKind::Chats {
            return "Chats".to_string();
        }

        self.ai_workspace_label_for_root(workspace_root)
    }

    pub(crate) fn ai_workspace_label_for_root(&self, workspace_root: &std::path::Path) -> String {
        if self.ai_workspace_kind_for_root(workspace_root) == AiWorkspaceKind::Chats {
            return "Chats".to_string();
        }

        workspace_target_summary_for_root(
            workspace_root,
            &self.workspace_targets,
            self.workspace_project_states
                .values()
                .map(|state| state.workspace_targets.as_slice()),
        )
        .map(|target| target.display_name.clone())
        .or_else(|| {
            workspace_root
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| workspace_root.display().to_string())
    }

    pub(crate) fn ai_thread_start_mode(&self, thread_id: &str) -> Option<AiNewThreadStartMode> {
        let thread = self.ai_thread_summary(thread_id)?;
        ai_thread_start_mode_for_workspace(
            self.repo_root.as_deref(),
            &self.workspace_targets,
            std::path::Path::new(thread.cwd.as_str()),
        )
    }

    pub(crate) fn ai_thread_mode_picker_state(
        &self,
        selected_thread_start_mode: Option<AiNewThreadStartMode>,
    ) -> (AiNewThreadStartMode, bool) {
        resolved_ai_thread_mode_picker_state(
            selected_thread_start_mode,
            self.ai_new_thread_start_mode,
            self.ai_new_thread_draft_active,
            self.ai_pending_new_thread_selection,
        )
    }

    pub(crate) fn ai_active_workspace_branch_name_with_root(
        &self,
        workspace_root: Option<&std::path::Path>,
    ) -> String {
        if self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_new_thread_start_mode == AiNewThreadStartMode::Worktree
        {
            let default_base_branch_name = self
                .ai_draft_workspace_root()
                .as_deref()
                .and_then(|root| resolve_default_base_branch_name(root).ok().flatten());
            return self
                .ai_selected_worktree_base_branch_name()
                .or(default_base_branch_name.as_deref())
                .or_else(|| self.primary_checked_out_branch_name())
                .unwrap_or(self.branch_name.as_str())
                .to_string();
        }

        let Some(workspace_root) = workspace_root else {
            return self
                .primary_checked_out_branch_name()
                .unwrap_or(self.branch_name.as_str())
                .to_string();
        };

        workspace_target_summary_for_root(
            workspace_root,
            &self.workspace_targets,
            self.workspace_project_states
                .values()
                .map(|state| state.workspace_targets.as_slice()),
        )
        .map(|target| target.branch_name.clone())
        .or_else(|| {
            cached_workspace_branch_name_for_root(
                workspace_root,
                &self.state.git_workflow_cache_by_repo,
            )
        })
        .unwrap_or_else(|| {
            self.primary_checked_out_branch_name()
                .unwrap_or(self.branch_name.as_str())
                .to_string()
        })
    }

    pub(super) fn ai_sync_workspace_preferences(&mut self, cx: &mut Context<Self>) {
        let previous_mad_max = self.ai_mad_max_mode;
        let previous_include_hidden = self.ai_include_hidden_models;
        self.sync_ai_workspace_preferences_from_state();
        if previous_mad_max != self.ai_mad_max_mode {
            self.send_ai_worker_command_if_running(
                AiWorkerCommand::SetMadMaxMode {
                    enabled: self.ai_mad_max_mode,
                },
                cx,
            );
        }
        if previous_include_hidden != self.ai_include_hidden_models {
            self.send_ai_worker_command_if_running(
                AiWorkerCommand::SetIncludeHiddenModels {
                    enabled: self.ai_include_hidden_models,
                },
                cx,
            );
        }
        self.sync_ai_session_selection_from_state();
        cx.notify();
    }

    fn sync_ai_workspace_preferences_from_state(&mut self) {
        self.ai_mad_max_mode =
            workspace_mad_max_mode(&self.state, self.ai_workspace_key().as_deref());
        self.ai_include_hidden_models =
            workspace_include_hidden_models(&self.state, self.ai_workspace_key().as_deref());
    }

    fn resolve_codex_executable_path() -> std::path::PathBuf {
        std::env::var_os("HUNK_CODEX_EXECUTABLE")
            .map(std::path::PathBuf::from)
            .map(Self::resolve_windows_codex_command_path)
            .or_else(|| {
                let current_exe = std::env::current_exe().ok()?;
                resolve_workspace_codex_executable_from_exe(current_exe.as_path()).or_else(|| {
                    resolve_bundled_codex_executable_from_exe(current_exe.as_path()).or_else(|| {
                        running_from_packaged_bundle().then(|| {
                            expected_bundled_codex_executable_from_exe(current_exe.as_path())
                        })?
                    })
                })
            })
            .or({
                #[cfg(target_os = "windows")]
                {
                    resolve_windows_command_path(std::path::Path::new("codex"))
                }
                #[cfg(not(target_os = "windows"))]
                {
                    None
                }
            })
            .unwrap_or_else(|| std::path::PathBuf::from("codex"))
    }

    fn validate_codex_executable_path(path: &std::path::Path) -> Result<(), String> {
        if is_command_name_without_path(path) {
            #[cfg(target_os = "windows")]
            {
                return Err(format!(
                    "Unable to find a spawnable Codex executable for '{}'. Install Codex so that 'codex.cmd' or 'codex.exe' is on PATH, or set HUNK_CODEX_EXECUTABLE to the full launcher path.",
                    path.display()
                ));
            }
            #[cfg(not(target_os = "windows"))]
            {
                if running_from_packaged_bundle() {
                    return Err(format!(
                        "Bundled Codex executable was not found for this packaged build; refusing to fall back to PATH for '{}'.",
                        path.display()
                    ));
                }
                return Ok(());
            }
        }
        if !path.exists() {
            return Err(format!(
                "Bundled Codex executable not found at {}",
                path.display()
            ));
        }
        if !path.is_file() {
            return Err(format!(
                "Bundled Codex executable path is not a file: {}",
                path.display()
            ));
        }
        #[cfg(target_os = "windows")]
        {
            if !windows_path_is_spawnable(path) {
                return Err(format!(
                    "Codex executable is not spawnable on Windows: {}. Point HUNK_CODEX_EXECUTABLE at a real '.cmd' or '.exe' launcher, not the Unix shim.",
                    path.display()
                ));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(path)
                .map_err(|error| format!("Unable to inspect Codex executable: {error}"))?;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(format!(
                    "Bundled Codex executable is not marked executable: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    fn resolve_windows_codex_command_path(path: std::path::PathBuf) -> std::path::PathBuf {
        #[cfg(target_os = "windows")]
        {
            resolve_windows_command_path(path.as_path()).unwrap_or(path)
        }
        #[cfg(not(target_os = "windows"))]
        {
            path
        }
    }

    fn default_ai_workspace_state_for_workspace_key(
        &self,
        workspace_key: Option<&str>,
    ) -> AiWorkspaceState {
        let mut next = AiWorkspaceState {
            include_hidden_models: workspace_include_hidden_models(&self.state, workspace_key),
            mad_max_mode: workspace_mad_max_mode(&self.state, workspace_key),
            ..AiWorkspaceState::default()
        };
        next.draft_workspace_root_override = workspace_key.map(PathBuf::from);
        let persisted = resolved_ai_thread_session_state(&self.state, None, workspace_key);
        next.selected_model = persisted.model;
        next.selected_effort = persisted.effort;
        next.selected_collaboration_mode = persisted.collaboration_mode;
        next.selected_service_tier = persisted.service_tier.unwrap_or_default();
        next
    }

    fn capture_current_ai_workspace_state(&self) -> AiWorkspaceState {
        AiWorkspaceState {
            connection_state: self.ai_connection_state,
            bootstrap_loading: self.ai_bootstrap_loading,
            status_message: self.ai_status_message.clone(),
            error_message: self.ai_error_message.clone(),
            state_snapshot: self.ai_state_snapshot.clone(),
            selected_thread_id: self.ai_selected_thread_id.clone(),
            new_thread_draft_active: self.ai_new_thread_draft_active,
            new_thread_start_mode: self.ai_new_thread_start_mode,
            worktree_base_branch_name: self.ai_worktree_base_branch_name.clone(),
            pending_new_thread_selection: self.ai_pending_new_thread_selection,
            pending_thread_start: self.ai_pending_thread_start.clone(),
            pending_steers: self.ai_pending_steers.clone(),
            queued_messages: self.ai_queued_messages.clone(),
            interrupt_restore_queued_thread_ids: self
                .ai_interrupt_restore_queued_thread_ids
                .clone(),
            timeline_follow_output: self.ai_timeline_follow_output,
            inline_review_selected_row_id_by_thread: self
                .ai_inline_review_selected_row_id_by_thread
                .clone(),
            inline_review_mode_by_thread: self.ai_inline_review_mode_by_thread.clone(),
            browser_open_thread_ids: self.ai_browser_open_thread_ids.clone(),
            right_pane_mode_by_thread: self.ai_right_pane_mode_by_thread.clone(),
            thread_title_refresh_state_by_thread: self
                .ai_thread_title_refresh_state_by_thread
                .clone(),
            timeline_visible_turn_limit_by_thread: self
                .ai_timeline_visible_turn_limit_by_thread
                .clone(),
            in_progress_turn_started_at: self.ai_in_progress_turn_started_at.clone(),
            expanded_timeline_row_ids: self.ai_expanded_timeline_row_ids.clone(),
            pending_approvals: self.ai_pending_approvals.clone(),
            pending_user_inputs: self.ai_pending_user_inputs.clone(),
            pending_user_input_answers: self.ai_pending_user_input_answers.clone(),
            account: self.ai_account.clone(),
            requires_openai_auth: self.ai_requires_openai_auth,
            pending_chatgpt_login_id: self.ai_pending_chatgpt_login_id.clone(),
            pending_chatgpt_auth_url: self.ai_pending_chatgpt_auth_url.clone(),
            rate_limits: self.ai_rate_limits.clone(),
            models: self.ai_models.clone(),
            experimental_features: self.ai_experimental_features.clone(),
            collaboration_modes: self.ai_collaboration_modes.clone(),
            skills: self.ai_skills.clone(),
            include_hidden_models: self.ai_include_hidden_models,
            selected_model: self.ai_selected_model.clone(),
            selected_effort: self.ai_selected_effort.clone(),
            selected_collaboration_mode: self.ai_selected_collaboration_mode,
            selected_service_tier: self.ai_selected_service_tier,
            review_mode_thread_ids: self.ai_review_mode_thread_ids.clone(),
            followup_prompt_state_by_thread: self.ai_followup_prompt_state_by_thread.clone(),
            mad_max_mode: self.ai_mad_max_mode,
            draft_workspace_root_override: self.ai_draft_workspace_root_override.clone(),
            terminal_open: self.ai_terminal_open,
            terminal_follow_output: self.ai_terminal_follow_output,
            terminal_height_px: self.ai_terminal_height_px,
            terminal_input_draft: self.ai_terminal_input_draft.clone(),
            terminal_session: self.ai_terminal_session.clone(),
        }
    }

    fn apply_ai_workspace_state(&mut self, mut state: AiWorkspaceState) {
        reconcile_ai_pending_steers(&mut state.pending_steers, &state.state_snapshot);
        let restored_pending_steers =
            take_restorable_ai_pending_steers(&mut state.pending_steers, &state.state_snapshot);
        let restored_queued_messages = reconcile_ai_queued_messages_after_snapshot(
            &mut state.queued_messages,
            &mut state.interrupt_restore_queued_thread_ids,
            &state.state_snapshot,
        );
        self.ai_connection_state = state.connection_state;
        self.ai_bootstrap_loading = state.bootstrap_loading;
        self.ai_status_message = state.status_message;
        self.ai_error_message = state.error_message;
        self.ai_state_snapshot = state.state_snapshot;
        self.ai_selected_thread_id = state.selected_thread_id;
        self.ai_new_thread_draft_active = state.new_thread_draft_active;
        self.ai_new_thread_start_mode = state.new_thread_start_mode;
        self.ai_worktree_base_branch_name = state.worktree_base_branch_name;
        self.ai_pending_new_thread_selection = state.pending_new_thread_selection;
        self.ai_pending_thread_start = state.pending_thread_start;
        self.ai_pending_steers = state.pending_steers;
        self.ai_queued_messages = state.queued_messages;
        self.ai_interrupt_restore_queued_thread_ids = state.interrupt_restore_queued_thread_ids;
        self.ai_scroll_timeline_to_bottom = false;
        self.ai_timeline_follow_output = state.timeline_follow_output;
        self.ai_inline_review_selected_row_id_by_thread =
            state.inline_review_selected_row_id_by_thread;
        self.ai_inline_review_mode_by_thread = state.inline_review_mode_by_thread;
        self.ai_browser_open_thread_ids = state.browser_open_thread_ids;
        self.ai_right_pane_mode_by_thread = state.right_pane_mode_by_thread;
        self.ai_inline_review_session = None;
        self.ai_inline_review_loaded_state = None;
        self.ai_inline_review_error = None;
        self.ai_inline_review_status_message = None;
        self.ai_inline_review_surface.clear_runtime_state();
        self.ai_thread_title_refresh_state_by_thread = state.thread_title_refresh_state_by_thread;
        self.ai_timeline_visible_turn_limit_by_thread = state.timeline_visible_turn_limit_by_thread;
        self.ai_in_progress_turn_started_at = state.in_progress_turn_started_at;
        self.ai_expanded_timeline_row_ids = state.expanded_timeline_row_ids;
        self.ai_pending_approvals = state.pending_approvals;
        self.ai_pending_user_inputs = state.pending_user_inputs;
        self.ai_pending_user_input_answers = state.pending_user_input_answers;
        self.ai_account = state.account;
        self.ai_requires_openai_auth = state.requires_openai_auth;
        self.ai_pending_chatgpt_login_id = state.pending_chatgpt_login_id;
        self.ai_pending_chatgpt_auth_url = state.pending_chatgpt_auth_url;
        self.ai_rate_limits = state.rate_limits;
        self.ai_models = state.models;
        self.ai_experimental_features = state.experimental_features;
        self.ai_collaboration_modes = state.collaboration_modes;
        if self.ai_skills != state.skills {
            self.ai_skills_generation = self.ai_skills_generation.saturating_add(1);
            self.ai_composer_completion_sync_key = None;
        }
        self.ai_skills = state.skills;
        self.ai_include_hidden_models = state.include_hidden_models;
        self.ai_selected_model = state.selected_model;
        self.ai_selected_effort = state.selected_effort;
        self.ai_selected_collaboration_mode = state.selected_collaboration_mode;
        self.ai_selected_service_tier = state.selected_service_tier;
        self.ai_review_mode_thread_ids = state.review_mode_thread_ids;
        self.ai_followup_prompt_state_by_thread = state.followup_prompt_state_by_thread;
        self.ai_mad_max_mode = state.mad_max_mode;
        self.ai_draft_workspace_root_override = state.draft_workspace_root_override;
        self.ai_terminal_open = state.terminal_open;
        self.ai_terminal_follow_output = state.terminal_follow_output;
        self.ai_terminal_height_px = state.terminal_height_px;
        self.ai_terminal_input_draft = state.terminal_input_draft;
        self.ai_terminal_session = state.terminal_session;
        self.invalidate_ai_visible_frame_state_with_reason("workspace");
        self.ai_text_selection = None;
        self.ai_text_selection_drag_pointer = None;
        self.ai_text_selection_auto_scroll_task = Task::ready(());
        self.rebuild_ai_timeline_indexes();
        self.sync_ai_in_progress_turn_started_at();
        self.ai_composer_activity_elapsed_second =
            self.current_ai_composer_activity_elapsed_second();
        self.ai_thread_title_refresh_state_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.ai_timeline_visible_turn_limit_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.ai_inline_review_selected_row_id_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.ai_inline_review_mode_by_thread
            .retain(|thread_id, _| self.ai_state_snapshot.threads.contains_key(thread_id));
        self.ai_review_mode_thread_ids
            .retain(|thread_id| self.ai_state_snapshot.threads.contains_key(thread_id));
        prune_ai_followup_prompt_state(
            &mut self.ai_followup_prompt_state_by_thread,
            &self.ai_state_snapshot,
        );
        sync_ai_review_mode_threads_after_snapshot(
            &mut self.ai_review_mode_thread_ids,
            &self.ai_state_snapshot,
        );
        self.sync_ai_pending_user_input_answers();
        self.ai_expanded_timeline_row_ids
            .retain(|row_id| self.ai_timeline_rows_by_id.contains_key(row_id));
        if self
            .ai_pending_thread_start
            .as_ref()
            .is_some_and(|pending| {
                pending.thread_id.as_ref().is_some_and(|thread_id| {
                    ai_state_has_user_message_for_thread(&self.ai_state_snapshot, thread_id)
                })
            })
        {
            self.ai_pending_thread_start = None;
        }
        if self.ai_selected_thread_id.as_ref().is_some_and(|selected| {
            self.ai_state_snapshot
                .threads
                .get(selected)
                .is_none_or(|thread| thread.status == ThreadLifecycleStatus::Archived)
        }) {
            self.ai_selected_thread_id = None;
        }
        if !self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_selected_thread_id.is_none()
        {
            self.ai_selected_thread_id = self.current_ai_thread_id();
        }
        if !self.ai_new_thread_draft_active
            && !self.ai_pending_new_thread_selection
            && self.ai_selected_thread_id.is_none()
            && let Some(first_thread) = self.ai_threads_for_current_workspace().first()
        {
            self.ai_selected_thread_id = Some(first_thread.id.clone());
        }
        self.ai_review_mode_active = self
            .current_ai_thread_id()
            .as_ref()
            .is_some_and(|thread_id| self.ai_review_mode_thread_ids.contains(thread_id));
        let current_thread_id = self.current_ai_thread_id();
        self.sync_ai_followup_prompt_state_for_selected_thread(current_thread_id.as_deref());
        self.restore_ai_pending_steers_to_drafts(restored_pending_steers);
        self.restore_ai_queued_messages_to_drafts(restored_queued_messages);
        self.prune_ai_composer_drafts();
        self.prune_ai_composer_statuses();
        self.rebuild_ai_thread_sidebar_state();
    }

    fn store_current_ai_workspace_state(&mut self, workspace_key: Option<&str>) {
        let Some(workspace_key) = workspace_key else {
            return;
        };
        self.ai_workspace_states.insert(
            workspace_key.to_string(),
            self.capture_current_ai_workspace_state(),
        );
    }

    fn restore_ai_workspace_state_for_key(&mut self, workspace_key: Option<&str>) {
        let state = workspace_key
            .and_then(|key| self.ai_workspace_states.get(key).cloned())
            .unwrap_or_else(|| self.default_ai_workspace_state_for_workspace_key(workspace_key));
        self.apply_ai_workspace_state(state);
    }

    fn park_visible_ai_runtime(&mut self) {
        let Some(workspace_key) = self.ai_worker_workspace_key.clone() else {
            return;
        };
        self.clear_ai_runtime_start_in_flight_for_workspace(Some(workspace_key.as_str()));
        let Some(command_tx) = self.ai_command_tx.take() else {
            self.ai_worker_workspace_key = None;
            return;
        };
        let Some(worker_thread) = self.ai_worker_thread.take() else {
            self.ai_worker_workspace_key = None;
            return;
        };
        let event_task = std::mem::replace(&mut self.ai_event_task, Task::ready(()));
        self.ai_hidden_runtimes.insert(
            workspace_key,
            AiHiddenRuntimeHandle {
                command_tx,
                worker_thread,
                event_task,
                generation: self.ai_event_epoch,
            },
        );
        self.ai_worker_workspace_key = None;
    }

    fn promote_hidden_ai_runtime(&mut self, workspace_key: &str) -> bool {
        let Some(handle) = self.ai_hidden_runtimes.remove(workspace_key) else {
            return false;
        };
        if handle.worker_thread.is_finished() {
            if let Err(error) = handle.worker_thread.join() {
                error!(
                    "failed to join completed hidden AI worker thread while promoting {workspace_key}: {error:?}"
                );
            }
            return false;
        }
        self.ai_command_tx = Some(handle.command_tx);
        self.ai_worker_thread = Some(handle.worker_thread);
        self.ai_event_task = handle.event_task;
        self.ai_event_epoch = handle.generation;
        self.ai_worker_workspace_key = Some(workspace_key.to_string());
        self.clear_ai_runtime_start_in_flight_for_workspace(Some(workspace_key));
        true
    }

    fn ai_runtime_listener_generation(&self, workspace_key: &str) -> Option<usize> {
        if self.ai_worker_workspace_key.as_deref() == Some(workspace_key) {
            return Some(self.ai_event_epoch);
        }
        self.ai_hidden_runtimes
            .get(workspace_key)
            .map(|handle| handle.generation)
    }

    fn ai_runtime_listener_is_current(&self, workspace_key: &str, generation: usize) -> bool {
        self.ai_runtime_listener_generation(workspace_key) == Some(generation)
    }

    fn ai_runtime_start_is_in_flight_for_workspace(&self, workspace_key: &str) -> bool {
        self.ai_runtime_starting_workspace_key.as_deref() == Some(workspace_key)
    }

    fn mark_ai_runtime_start_in_flight(&mut self, workspace_key: &str) {
        self.ai_runtime_starting_workspace_key = Some(workspace_key.to_string());
    }

    fn clear_ai_runtime_start_in_flight_for_workspace(&mut self, workspace_key: Option<&str>) {
        let should_clear = match (
            self.ai_runtime_starting_workspace_key.as_deref(),
            workspace_key,
        ) {
            (_, None) => true,
            (Some(in_flight), Some(workspace_key)) => in_flight == workspace_key,
            (None, Some(_)) => false,
        };
        if should_clear {
            self.ai_runtime_starting_workspace_key = None;
        }
    }

    fn update_background_ai_workspace_state<F>(&mut self, workspace_key: &str, update: F)
    where
        F: FnOnce(&mut AiWorkspaceState),
    {
        let default_state = self.default_ai_workspace_state_for_workspace_key(Some(workspace_key));
        let state = self
            .ai_workspace_states
            .entry(workspace_key.to_string())
            .or_insert(default_state);
        update(state);
    }

    fn apply_ai_snapshot_to_workspace_state(
        state: &mut AiWorkspaceState,
        snapshot: AiSnapshot,
    ) -> Vec<AiPendingSteer> {
        let AiSnapshot {
            state: next_snapshot,
            active_thread_id,
            pending_approvals,
            pending_user_inputs,
            account,
            requires_openai_auth,
            pending_chatgpt_login_id,
            pending_chatgpt_auth_url,
            rate_limits,
            models,
            experimental_features,
            collaboration_modes,
            skills,
            include_hidden_models,
            mad_max_mode,
        } = snapshot;

        state.state_snapshot = next_snapshot;
        reconcile_ai_pending_steers(&mut state.pending_steers, &state.state_snapshot);
        let restored_pending_steers =
            take_restorable_ai_pending_steers(&mut state.pending_steers, &state.state_snapshot);
        state.pending_approvals = pending_approvals;
        state.pending_user_inputs = pending_user_inputs;
        state.account = account;
        state.requires_openai_auth = requires_openai_auth;
        state.pending_chatgpt_login_id = pending_chatgpt_login_id;
        state.pending_chatgpt_auth_url = pending_chatgpt_auth_url;
        state.rate_limits = rate_limits;
        state.models = models;
        state.experimental_features = experimental_features;
        state.collaboration_modes = collaboration_modes;
        state.skills = skills;
        state.include_hidden_models = include_hidden_models;
        state.mad_max_mode = mad_max_mode;
        state.connection_state = AiConnectionState::Ready;
        state.error_message = ai_auth_required_message(
            state.account.as_ref(),
            state.requires_openai_auth,
            state.pending_chatgpt_login_id.as_deref(),
        );

        if let Some(thread_id) = pending_new_thread_selection_ready_thread_id(
            state.pending_new_thread_selection,
            state.pending_thread_start.as_ref(),
            active_thread_id.as_deref(),
            &state.state_snapshot,
        ) {
            state.new_thread_draft_active = false;
            state.pending_new_thread_selection = false;
            state.selected_thread_id = Some(thread_id);
        }

        if state.pending_thread_start.as_ref().is_some_and(|pending| {
            pending.thread_id.as_ref().is_some_and(|thread_id| {
                ai_state_has_user_message_for_thread(&state.state_snapshot, thread_id)
            })
        }) {
            state.pending_thread_start = None;
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
            state.selected_thread_id = active_thread_id;
        }

        if !state.new_thread_draft_active
            && !state.pending_new_thread_selection
            && state.selected_thread_id.is_none()
            && let Some(first_thread) = sorted_threads(&state.state_snapshot).first()
        {
            state.selected_thread_id = Some(first_thread.id.clone());
        }

        prune_ai_followup_prompt_state(
            &mut state.followup_prompt_state_by_thread,
            &state.state_snapshot,
        );
        sync_ai_review_mode_threads_after_snapshot(
            &mut state.review_mode_thread_ids,
            &state.state_snapshot,
        );
        sync_ai_followup_prompt_ui_state(
            &mut state.followup_prompt_state_by_thread,
            &state.state_snapshot,
            state.selected_thread_id.as_deref(),
            state.selected_collaboration_mode,
        );
        state
            .thread_title_refresh_state_by_thread
            .retain(|thread_id, _| state.state_snapshot.threads.contains_key(thread_id));
        state
            .timeline_visible_turn_limit_by_thread
            .retain(|thread_id, _| state.state_snapshot.threads.contains_key(thread_id));
        restored_pending_steers
    }

    fn restore_ai_workspace_state_after_failure_for_state(state: &mut AiWorkspaceState) {
        if state.pending_new_thread_selection {
            state.new_thread_draft_active = true;
        }
        state.pending_new_thread_selection = false;
        if let Some(pending) = state.pending_thread_start.as_mut() {
            pending.thread_id = None;
        }
    }

    fn reset_background_ai_workspace_after_failure(state: &mut AiWorkspaceState) {
        state.connection_state = AiConnectionState::Failed;
        state.bootstrap_loading = false;
        state.account = None;
        state.requires_openai_auth = false;
        state.pending_chatgpt_login_id = None;
        state.pending_chatgpt_auth_url = None;
        state.rate_limits = None;
        state.models.clear();
        state.experimental_features.clear();
        state.collaboration_modes.clear();
        state.skills.clear();
        state.pending_approvals.clear();
        state.pending_user_inputs.clear();
        state.pending_user_input_answers.clear();
        state.pending_steers.clear();
        state.queued_messages.clear();
        state.interrupt_restore_queued_thread_ids.clear();
        Self::restore_ai_workspace_state_after_failure_for_state(state);
    }

    fn apply_background_ai_workspace_fatal(state: &mut AiWorkspaceState, message: String) {
        Self::reset_background_ai_workspace_after_failure(state);
        state.status_message = Some("Codex integration failed".to_string());
        state.error_message = Some(message);
    }

    fn apply_background_ai_workspace_disconnect(state: &mut AiWorkspaceState) {
        Self::reset_background_ai_workspace_after_failure(state);
        if state.error_message.is_none() {
            let message = "Codex worker disconnected.".to_string();
            state.error_message = Some(message);
            state.status_message = Some("Codex integration failed".to_string());
        }
    }

    fn handle_background_ai_worker_event(
        &mut self,
        workspace_key: &str,
        event: AiWorkerEventPayload,
        cx: &mut Context<Self>,
    ) {
        let mut restored_pending_steers = Vec::new();
        let mut restored_queued_messages = Vec::new();
        let mut reconcile_queued_after_snapshot = false;
        let mut emit_desktop_notification = false;
        let mut clear_desktop_notification_state = false;
        self.update_background_ai_workspace_state(workspace_key, |state| match event {
            AiWorkerEventPayload::Snapshot(snapshot) => {
                restored_pending_steers =
                    Self::apply_ai_snapshot_to_workspace_state(state, *snapshot);
                reconcile_queued_after_snapshot = true;
                emit_desktop_notification = true;
            }
            AiWorkerEventPayload::BootstrapCompleted => {
                state.bootstrap_loading = false;
            }
            AiWorkerEventPayload::ThreadStarted { thread_id } => {
                set_pending_thread_start_thread_id(&mut state.pending_thread_start, thread_id);
            }
            AiWorkerEventPayload::SteerAccepted(pending) => {
                state.pending_steers.push(pending);
            }
            AiWorkerEventPayload::Reconnecting(message) => {
                state.connection_state = AiConnectionState::Reconnecting;
                state.bootstrap_loading = false;
                state.error_message = None;
                state.status_message = Some(message);
            }
            AiWorkerEventPayload::Status(message) => {
                state.status_message = Some(message.clone());
                if let Some(error_message) = ai_prominent_worker_status_error(message.as_str()) {
                    state.error_message = Some(error_message);
                }
            }
            AiWorkerEventPayload::Error(message) => {
                Self::restore_ai_workspace_state_after_failure_for_state(state);
                state.error_message = Some(message.clone());
                state.status_message = Some(message);
            }
            AiWorkerEventPayload::Fatal(message) => {
                restored_pending_steers = take_all_ai_pending_steers(&mut state.pending_steers);
                restored_queued_messages = take_all_ai_queued_messages(&mut state.queued_messages);
                state.interrupt_restore_queued_thread_ids.clear();
                Self::apply_background_ai_workspace_fatal(state, message);
                clear_desktop_notification_state = true;
            }
            AiWorkerEventPayload::BrowserToolCall {
                params,
                response_tx,
            } => {
                let response = crate::app::ai_dynamic_tools::browser_unavailable_response(
                    &params,
                    "The embedded browser is only available in the visible AI workspace.",
                );
                if response_tx.send(response).is_err() {
                    state.status_message =
                        Some("Embedded browser tool response receiver disconnected.".to_string());
                }
            }
        });
        if clear_desktop_notification_state {
            self.clear_ai_desktop_notification_state(Some(workspace_key));
        }
        if emit_desktop_notification {
            self.maybe_emit_background_ai_desktop_notification(workspace_key, cx);
        }
        if reconcile_queued_after_snapshot {
            self.ai_prune_terminal_threads("updating background AI workspace snapshot", cx);
        }
        let _ = self.restore_ai_pending_steers_to_drafts(restored_pending_steers);
        if reconcile_queued_after_snapshot {
            let mut ready_thread_ids = Vec::new();
            if let Some(state) = self.ai_workspace_states.get_mut(workspace_key) {
                restored_queued_messages = reconcile_ai_queued_messages_after_snapshot(
                    &mut state.queued_messages,
                    &mut state.interrupt_restore_queued_thread_ids,
                    &state.state_snapshot,
                );
                ready_thread_ids = ready_ai_queued_message_thread_ids(
                    state.queued_messages.as_slice(),
                    &state.interrupt_restore_queued_thread_ids,
                    &state.state_snapshot,
                );
            }
            let _ = self.restore_ai_queued_messages_to_drafts(restored_queued_messages);
            for thread_id in ready_thread_ids {
                let Some((accepted_after_sequence, session_overrides)) =
                    self.ai_workspace_states.get(workspace_key).map(|state| {
                        (
                            thread_latest_timeline_sequence(
                                &state.state_snapshot,
                                thread_id.as_str(),
                            ),
                            resolved_ai_turn_session_overrides(
                                &self.state,
                                state.models.as_slice(),
                                Some(thread_id.as_str()),
                                Some(workspace_key),
                            ),
                        )
                    })
                else {
                    break;
                };
                let Some((queued_ix, queued)) = self
                    .ai_workspace_states
                    .get_mut(workspace_key)
                    .and_then(|state| {
                        mark_next_ai_queued_message_pending_confirmation(
                            state.queued_messages.as_mut_slice(),
                            thread_id.as_str(),
                            accepted_after_sequence,
                        )
                    })
                else {
                    continue;
                };
                let prompt = (!queued.prompt.trim().is_empty()).then_some(queued.prompt.clone());
                let sent = self.send_ai_worker_command_for_workspace(
                    Some(workspace_key),
                    AiWorkerCommand::SendPrompt {
                        thread_id: queued.thread_id.clone(),
                        prompt,
                        local_image_paths: queued.local_images.clone(),
                        selected_skills: queued.selected_skills.clone(),
                        skill_bindings: queued.skill_bindings.clone(),
                        session_overrides,
                    },
                    false,
                    cx,
                );
                if !sent && let Some(state) = self.ai_workspace_states.get_mut(workspace_key) {
                    reset_ai_queued_message_to_queued(
                        state.queued_messages.as_mut_slice(),
                        queued_ix,
                    );
                }
            }
            return;
        }
        let _ = self.restore_ai_queued_messages_to_drafts(restored_queued_messages);
    }

    fn handle_background_ai_worker_disconnect(&mut self, workspace_key: &str) {
        if let Some(hidden) = self.ai_hidden_runtimes.remove(workspace_key) {
            let AiHiddenRuntimeHandle { worker_thread, .. } = hidden;
            let workspace_key = workspace_key.to_string();
            std::thread::spawn(move || {
                if let Err(error) = worker_thread.join() {
                    error!(
                        "failed to join hidden AI worker thread during disconnect for {workspace_key}: {error:?}"
                    );
                }
            });
        }
        let mut restored_pending_steers = Vec::new();
        let mut restored_queued_messages = Vec::new();
        self.update_background_ai_workspace_state(workspace_key, |state| {
            restored_pending_steers = take_all_ai_pending_steers(&mut state.pending_steers);
            restored_queued_messages = take_all_ai_queued_messages(&mut state.queued_messages);
            state.interrupt_restore_queued_thread_ids.clear();
            Self::apply_background_ai_workspace_disconnect(state);
        });
        let _ = self.restore_ai_pending_steers_to_drafts(restored_pending_steers);
        let _ = self.restore_ai_queued_messages_to_drafts(restored_queued_messages);
    }

    pub(super) fn shutdown_ai_worker_blocking(&mut self) {
        if let Some(command_tx) = self.ai_command_tx.take() {
            let _ = command_tx.send(AiWorkerCommand::Shutdown);
        }
        let visible_workspace_key = self.ai_worker_workspace_key.clone();
        self.clear_ai_runtime_start_in_flight_for_workspace(visible_workspace_key.as_deref());
        self.ai_worker_workspace_key = None;
        self.join_ai_worker_thread("dropping DiffViewer");
        for (_, hidden) in std::mem::take(&mut self.ai_hidden_runtimes) {
            let _ = hidden.command_tx.send(AiWorkerCommand::Shutdown);
            if let Err(error) = hidden.worker_thread.join() {
                error!("failed to join hidden AI worker thread during shutdown: {error:?}");
            }
        }
    }

    pub(super) fn shutdown_ai_runtime_for_workspace_blocking(&mut self, workspace_key: &str) {
        if self.ai_worker_workspace_key.as_deref() == Some(workspace_key) {
            if let Some(command_tx) = self.ai_command_tx.take() {
                let _ = command_tx.send(AiWorkerCommand::Shutdown);
            }
            self.clear_ai_runtime_start_in_flight_for_workspace(Some(workspace_key));
            self.ai_worker_workspace_key = None;
            self.ai_connection_state = AiConnectionState::Disconnected;
            self.ai_bootstrap_loading = false;
            self.ai_event_task = Task::ready(());
            self.join_ai_worker_thread("deleting managed AI worktree");
        }

        if let Some(hidden) = self.ai_hidden_runtimes.remove(workspace_key) {
            let _ = hidden.command_tx.send(AiWorkerCommand::Shutdown);
            if let Err(error) = hidden.worker_thread.join() {
                error!(
                    "failed to join hidden AI worker thread during managed worktree deletion for {workspace_key}: {error:?}"
                );
            }
        }
    }

    fn join_ai_worker_thread_if_finished(&mut self, reason: &str) {
        let Some(worker) = self.ai_worker_thread.take() else {
            return;
        };
        if !worker.is_finished() {
            self.ai_worker_thread = Some(worker);
            return;
        }
        if let Err(error) = worker.join() {
            error!("failed to join completed AI worker thread during {reason}: {error:?}");
        }
    }

    fn join_ai_worker_thread(&mut self, reason: &str) {
        let Some(worker) = self.ai_worker_thread.take() else {
            return;
        };
        if let Err(error) = worker.join() {
            error!("failed to join AI worker thread during {reason}: {error:?}");
        }
    }

    fn send_ai_worker_command(&mut self, command: AiWorkerCommand, cx: &mut Context<Self>) -> bool {
        let workspace_key = self.ai_workspace_key();
        self.send_ai_worker_command_for_workspace(workspace_key.as_deref(), command, true, cx)
    }

    fn send_ai_worker_command_if_running(
        &mut self,
        command: AiWorkerCommand,
        cx: &mut Context<Self>,
    ) -> bool {
        let workspace_key = self.ai_workspace_key();
        self.send_ai_worker_command_for_workspace(workspace_key.as_deref(), command, false, cx)
    }

    fn send_ai_worker_command_for_workspace(
        &mut self,
        workspace_key: Option<&str>,
        command: AiWorkerCommand,
        ensure_running: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let current_workspace_key = self.ai_workspace_key();
        let Some(workspace_key) = workspace_key.or(current_workspace_key.as_deref()) else {
            return false;
        };

        if self.ai_worker_workspace_key.as_deref() == Some(workspace_key) {
            if ensure_running && self.ai_command_tx.is_none() {
                self.ensure_ai_runtime_started(cx);
            }

            let Some(command_tx) = self.ai_command_tx.as_ref() else {
                return false;
            };
            if command_tx.send(command).is_ok() {
                return true;
            }

            self.ai_connection_state = AiConnectionState::Failed;
            self.ai_bootstrap_loading = false;
            self.ai_error_message = Some("AI worker channel disconnected.".to_string());
            self.ai_command_tx = None;
            self.clear_ai_runtime_start_in_flight_for_workspace(Some(workspace_key));
            self.ai_worker_workspace_key = None;
            self.join_ai_worker_thread("worker channel disconnect");
            cx.notify();
            return false;
        }

        if let Some(command_tx) = self
            .ai_hidden_runtimes
            .get(workspace_key)
            .map(|runtime| runtime.command_tx.clone())
        {
            if command_tx.send(command).is_ok() {
                return true;
            }

            self.handle_background_ai_worker_disconnect(workspace_key);
            cx.notify();
            return false;
        }

        if ensure_running && current_workspace_key.as_deref() == Some(workspace_key) {
            self.ensure_ai_runtime_started(cx);
            if self.ai_worker_workspace_key.as_deref() == Some(workspace_key) {
                return self.send_ai_worker_command_for_workspace(
                    Some(workspace_key),
                    command,
                    false,
                    cx,
                );
            }
        }

        false
    }

    fn next_ai_event_epoch(&mut self) -> usize {
        self.ai_event_epoch = self.ai_event_epoch.saturating_add(1);
        self.ai_event_epoch
    }

    fn ai_add_composer_local_images<I>(&mut self, paths: I) -> usize
    where
        I: IntoIterator<Item = std::path::PathBuf>,
    {
        let mut added = 0;
        let Some(draft) = self.current_ai_composer_draft_mut() else {
            return 0;
        };

        for path in paths {
            let normalized = std::fs::canonicalize(path.as_path()).unwrap_or(path);
            if !normalized.is_file()
                || !crate::app::ai_attachment_images::is_supported_ai_image_path(
                    normalized.as_path(),
                )
            {
                continue;
            }
            if draft
                .local_images
                .iter()
                .any(|existing| existing == &normalized)
            {
                continue;
            }
            draft.local_images.push(normalized);
            added += 1;
        }

        added
    }
}

fn workspace_target_summary_for_root<'a, I>(
    workspace_root: &std::path::Path,
    current_workspace_targets: &'a [WorkspaceTargetSummary],
    other_workspace_targets: I,
) -> Option<&'a WorkspaceTargetSummary>
where
    I: IntoIterator<Item = &'a [WorkspaceTargetSummary]>,
{
    current_workspace_targets
        .iter()
        .find(|target| target.root.as_path() == workspace_root)
        .or_else(|| {
            other_workspace_targets.into_iter().find_map(|targets| {
                targets
                    .iter()
                    .find(|target| target.root.as_path() == workspace_root)
            })
        })
}

fn workspace_branch_name_for_root<'a, I>(
    workspace_root: &std::path::Path,
    current_workspace_targets: &'a [WorkspaceTargetSummary],
    other_workspace_targets: I,
    workflow_cache_by_repo: &std::collections::BTreeMap<String, CachedWorkflowState>,
) -> Option<String>
where
    I: IntoIterator<Item = &'a [WorkspaceTargetSummary]>,
{
    workspace_target_summary_for_root(
        workspace_root,
        current_workspace_targets,
        other_workspace_targets,
    )
    .map(|target| target.branch_name.clone())
    .or_else(|| cached_workspace_branch_name_for_root(workspace_root, workflow_cache_by_repo))
}

fn cached_workspace_branch_name_for_root(
    workspace_root: &std::path::Path,
    workflow_cache_by_repo: &std::collections::BTreeMap<String, CachedWorkflowState>,
) -> Option<String> {
    workflow_cache_by_repo
        .iter()
        .find_map(|(project_key, workflow)| {
            workflow
                .branches
                .iter()
                .find(|branch| {
                    branch.attached_workspace_target_root.as_deref() == Some(workspace_root)
                })
                .map(|branch| branch.name.clone())
                .or_else(|| {
                    let is_primary_root = workflow.root.as_deref() == Some(workspace_root)
                        || std::path::Path::new(project_key.as_str()) == workspace_root;
                    is_primary_root
                        .then(|| workflow.branch_name.trim())
                        .filter(|branch_name| !branch_name.is_empty())
                        .map(str::to_string)
                })
        })
}

fn ai_in_progress_turn_tracking_key(thread_id: &str, turn_id: &str) -> String {
    format!("{thread_id}::{turn_id}")
}

fn ai_attachment_status_message(file_count: usize, added_count: usize) -> Option<String> {
    if file_count == 0 || added_count == file_count {
        return None;
    }

    if added_count == 0 {
        if file_count == 1 {
            return Some("File is not a supported image or is already attached.".to_string());
        }
        return Some("No files were supported images or were already attached.".to_string());
    }

    let added_suffix = if added_count == 1 { "" } else { "s" };
    let skipped_count = file_count.saturating_sub(added_count);
    let skipped_suffix = if skipped_count == 1 { "" } else { "s" };
    Some(format!(
        "Attached {added_count} image{added_suffix}. Skipped {skipped_count} unsupported or duplicate file{skipped_suffix}."
    ))
}
