impl DiffViewer {
    fn canonical_workspace_project_root(selected_path: &std::path::Path) -> anyhow::Result<PathBuf> {
        hunk_git::worktree::primary_repo_root(selected_path)
    }

    fn activate_workspace_project_root(&mut self, project_root: PathBuf, cx: &mut Context<Self>) {
        let previous_project_key = self.current_workspace_project_key();
        let previous_files_terminal_project_key = self.current_files_terminal_owner_key();
        let previous_ai_workspace_key = self.ai_workspace_key();
        let next_project_key = project_root.to_string_lossy().to_string();
        let switching_projects = previous_project_key.as_deref() != Some(next_project_key.as_str());
        if switching_projects {
            self.store_current_workspace_project_state();
        }
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.project_path = Some(project_root.clone());
        self.set_active_workspace_project_path(Some(project_root));
        let restored_warm_state =
            self.restore_workspace_project_state(std::path::Path::new(next_project_key.as_str()));
        if !restored_warm_state {
            self.apply_workspace_project_state(Self::empty_workspace_project_state());
        }
        self.sync_project_picker_state(cx);
        self.sync_branch_picker_state(cx);
        self.sync_workspace_target_picker_state(cx);
        self.sync_review_compare_picker_states(cx);
        if !restored_warm_state {
            self.hydrate_workflow_cache_if_available(cx);
        }
        self.restore_active_workspace_target_root_from_state(cx);
        self.files_handle_project_change(previous_files_terminal_project_key, cx);
        self.ai_handle_workspace_change(previous_ai_workspace_key, cx);
        self.git_status_message = None;
        self.repo_discovery_failed = false;
        self.error_message = None;
        if !restored_warm_state {
            self.reset_recent_commits_state();
            self.hydrate_recent_commits_cache_if_available(cx);
        }
        self.bootstrap_files_workspace_if_needed(cx);
        self.rebuild_ai_thread_sidebar_state();
        self.start_repo_watch(cx);
        self.request_snapshot_refresh_internal(SnapshotRefreshRequest::user(true), cx);
        self.request_recent_commits_refresh(true, cx);
        self.defer_root_focus(cx);
        cx.notify();
    }

    fn reset_to_empty_workspace_state(
        &mut self,
        clear_active_project_cache: bool,
        cx: &mut Context<Self>,
    ) {
        self.stop_all_files_terminal_runtimes("resetting to empty workspace");
        self.files_terminal_states_by_project.clear();
        if self.state.workspace_project_paths.is_empty() {
            self.workspace_project_states.clear();
        }
        self.cancel_line_stats_refresh();
        self.cancel_patch_reload();
        self.pending_dirty_paths.clear();
        self.last_snapshot_fingerprint = None;
        self.reset_recent_commits_state();
        let previous_ai_workspace_key = self.ai_workspace_key();
        let active_project_cache_key = self.current_workspace_project_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.project_path = None;
        self.repo_root = None;
        self.workspace_targets.clear();
        self.active_workspace_target_id = None;
        self.sync_project_picker_state(cx);
        self.ai_draft_workspace_root_override = None;
        self.ai_draft_workspace_target_id = None;
        self.persist_active_workspace_target_id();
        self.sync_workspace_target_picker_state(cx);
        self.review_compare_sources.clear();
        self.review_default_left_source_id = None;
        self.review_default_right_source_id = None;
        self.review_left_source_id = None;
        self.review_right_source_id = None;
        self.review_loaded_left_source_id = None;
        self.review_loaded_right_source_id = None;
        self.review_loaded_collapsed_files.clear();
        self.sync_review_compare_picker_states(cx);
        self.ai_handle_workspace_change(previous_ai_workspace_key, cx);
        self.request_ai_composer_file_completion_reload(cx);
        self.branch_name = "unknown".to_string();
        self.branch_has_upstream = false;
        self.branch_ahead_count = 0;
        self.branch_behind_count = 0;
        self.working_copy_commit_id = None;
        self.branches.clear();
        self.git_action_label = None;
        self.ai_git_progress = None;
        self.files.clear();
        self.file_status_by_path.clear();
        self.review_files.clear();
        self.review_file_status_by_path.clear();
        self.review_file_line_stats.clear();
        self.review_overall_line_stats = LineStats::default();
        self.review_compare_loading = false;
        self.review_compare_error = None;
        self.review_workspace_session = None;
        self.review_loaded_snapshot_fingerprint = None;
        self.review_surface.clear_workspace_editors();
        self.review_surface.clear_workspace_search_matches();
        self.review_surface.selected_path = None;
        self.review_surface.clear_row_selection();
        self.last_commit_subject = None;
        self.selected_path = None;
        self.selected_status = None;
        self.overall_line_stats = LineStats::default();
        self.comments_cache.clear();
        self.comment_miss_streaks.clear();
        self.reset_comment_row_match_cache();
        self.clear_comment_ui_state();
        self.file_line_stats.clear();
        self.reset_review_surface_runtime_state();
        self.repo_discovery_failed = true;
        self.error_message = None;
        self.repo_tree.nodes.clear();
        self.repo_tree.rows.clear();
        self.repo_tree.file_count = 0;
        self.repo_tree.folder_count = 0;
        self.repo_tree.expanded_dirs.clear();
        self.repo_tree.scroll_anchor_path = None;
        self.repo_tree.row_count = 0;
        self.repo_tree.list_state.reset(0);
        self.rebuild_ai_thread_sidebar_state();
        self.repo_tree.loading = false;
        self.repo_tree.reload_pending = false;
        self.repo_tree.error = None;
        self.repo_tree.changed_only = false;
        self.clear_full_repo_tree_cache();
        self.clear_editor_state(cx);
        if clear_active_project_cache
            && let Some(cache_key) = active_project_cache_key
            && self
                .state
                .git_workflow_cache_by_repo
                .remove(cache_key.as_str())
                .is_some()
        {
            self.persist_state();
        }
        self.clear_recent_commits_cache();
        self.files_terminal_open = false;
        self.files_terminal_follow_output = true;
        self.files_terminal_session = AiTerminalSessionState::default();
        self.files_terminal_restore_target = FilesTerminalRestoreTarget::default();
        self.files_terminal_surface_focused = false;
        self.files_terminal_pending_input = None;
        self.files_terminal_grid_size = None;
        self.ai_worktree_base_branch_name = None;
        self.sync_branch_picker_state(cx);
        self.sync_ai_worktree_base_branch_picker_state(cx);
        cx.notify();
    }

    fn maybe_run_pending_snapshot_refresh(&mut self, cx: &mut Context<Self>) {
        if self.snapshot_loading {
            return;
        }
        let Some(request) = self.pending_snapshot_refresh.take() else {
            return;
        };
        debug!(
            "git workspace running queued refresh: force={} priority={}",
            request.force,
            request.priority.as_str()
        );
        self.request_snapshot_refresh_internal(request, cx);
    }

    pub(super) fn request_snapshot_refresh_internal(
        &mut self,
        request: SnapshotRefreshRequest,
        cx: &mut Context<Self>,
    ) {
        self.request_snapshot_refresh_with_scope(request, cx);
    }

    fn request_snapshot_refresh_with_scope(
        &mut self,
        request: SnapshotRefreshRequest,
        cx: &mut Context<Self>,
    ) {
        let request = self
            .pending_snapshot_refresh
            .take()
            .map_or(request, |pending| request.merge(pending));

        if self.snapshot_loading {
            if self.should_preempt_active_snapshot_refresh(request) {
                let active = self.active_snapshot_refresh_request();
                debug!(
                    "git workspace refresh preempted: epoch={} active_priority={} next_priority={} force={}",
                    self.snapshot_epoch,
                    active.priority.as_str(),
                    request.priority.as_str(),
                    request.force
                );
                self.snapshot_task = Task::ready(());
                self.snapshot_active_request = None;
            } else {
                self.enqueue_snapshot_refresh(request);
                tracing::debug!(
                    "git workspace refresh deferred: queued refresh while epoch={} is still loading (force={} priority={})",
                    self.snapshot_epoch,
                    request.force,
                    request.priority.as_str()
                );
                return;
            }
        }
        if request.force {
            self.auto_refresh_unmodified_streak = 0;
        }
        let cold_start = self.last_snapshot_fingerprint.is_none();

        let source_dir_result = self
            .repo_root
            .clone()
            .or_else(|| self.project_path.clone())
            .map(Ok)
            .unwrap_or_else(|| std::env::current_dir().context("failed to resolve current directory"));
        let previous_fingerprint = if request.force {
            None
        } else {
            self.last_snapshot_fingerprint.clone()
        };
        let prefer_stale_first = cold_start && !request.force;
        let epoch = self.next_snapshot_epoch();
        self.snapshot_loading = true;
        self.snapshot_active_request = Some(request);
        self.workflow_loading = true;
        let refresh_root = self
            .repo_root
            .clone()
            .or_else(|| self.project_path.clone())
            .unwrap_or_else(|| PathBuf::from("."));
        debug!(
            "git workspace refresh start: epoch={} force={} priority={} behavior={} cold_start={} root={}",
            epoch,
            request.force,
            request.priority.as_str(),
            request.behavior.as_str(),
            cold_start,
            refresh_root.display()
        );
        cx.notify();

        self.snapshot_task = cx.spawn(async move |this, cx| {
            let started_at = Instant::now();
            // Stage A: resolve workspace state first so right-pane workflow data can paint early.
            let stage_a_result = match source_dir_result {
                Ok(source_dir) => {
                    cx.background_executor()
                        .spawn(async move {
                            let load_once = || -> Result<SnapshotRefreshStageA> {
                                load_snapshot_stage_a_for_path(
                                    snapshot_stage_a_load_path(
                                        request.behavior,
                                        prefer_stale_first,
                                    ),
                                    &source_dir,
                                    previous_fingerprint.as_ref(),
                                )
                            };

                            match load_once() {
                                Ok(result) => Ok(result),
                                Err(primary_err) => {
                                    if matches!(request.behavior, SnapshotRefreshBehavior::ReadOnly)
                                    {
                                        return Err(primary_err);
                                    }
                                    warn!(
                                        "snapshot stage A stale-first load failed; retrying with working-copy refresh: {primary_err:#}"
                                    );

                                    let fallback = || -> Result<SnapshotRefreshStageA> {
                                        load_snapshot_stage_a_for_path(
                                            snapshot_stage_a_fallback_load_path(
                                                prefer_stale_first,
                                            ),
                                            &source_dir,
                                            previous_fingerprint.as_ref(),
                                        )
                                    };

                                    match fallback() {
                                        Ok(result) => Ok(result),
                                        Err(fallback_err) => Err(primary_err.context(format!(
                                            "snapshot stage A fallback load failed: {fallback_err:#}"
                                        ))),
                                    }
                                }
                            }
                        })
                        .await
                }
                Err(err) => Err(err),
            };

            let (fingerprint, workflow_snapshot, loaded_without_refresh) = match stage_a_result {
                Ok(SnapshotRefreshStageA::Loaded {
                    fingerprint,
                    workflow,
                    loaded_without_refresh,
                }) => (fingerprint, workflow, loaded_without_refresh),
                Ok(SnapshotRefreshStageA::Unchanged(fingerprint)) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            if epoch != this.snapshot_epoch {
                                return;
                            }

                            this.finish_snapshot_refresh_loading();
                            this.workflow_loading = false;
                            let elapsed = started_at.elapsed();
                            debug!(
                                "git workspace refresh skipped: epoch={} force={} priority={} behavior={} cold_start={} elapsed_ms={} (no repo changes)",
                                epoch,
                                request.force,
                                request.priority.as_str(),
                                request.behavior.as_str(),
                                cold_start,
                                elapsed.as_millis()
                            );
                            this.auto_refresh_unmodified_streak =
                                this.auto_refresh_unmodified_streak.saturating_add(1);
                            this.last_snapshot_fingerprint = Some(fingerprint);
                            cx.notify();
                            this.maybe_run_pending_snapshot_refresh(cx);
                        });
                    }
                    return;
                }
                Err(err) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, move |this, cx| {
                            if epoch != this.snapshot_epoch {
                                return;
                            }

                            this.finish_snapshot_refresh_loading();
                            this.workflow_loading = false;
                            let elapsed = started_at.elapsed();
                            error!(
                                "git workspace refresh failed: epoch={} force={} priority={} behavior={} cold_start={} elapsed_ms={} err={err:#}",
                                epoch,
                                request.force,
                                request.priority.as_str(),
                                request.behavior.as_str(),
                                cold_start,
                                elapsed.as_millis()
                            );
                            this.apply_snapshot_error(err, cx);
                            this.maybe_run_pending_snapshot_refresh(cx);
                        });
                    }
                    return;
                }
            };

            let workflow_file_count = workflow_snapshot.files.len();
            let workflow_branch_count = workflow_snapshot.branches.len();
            let workflow_ready_elapsed = started_at.elapsed();
            let should_run_cold_start_reconcile = should_run_cold_start_reconcile(
                cold_start,
                loaded_without_refresh,
                request.behavior,
            );
            debug!(
                "git workspace workflow ready: epoch={} force={} priority={} behavior={} elapsed_ms={} files={} branches={} cold_start={}",
                epoch,
                request.force,
                request.priority.as_str(),
                request.behavior.as_str(),
                workflow_ready_elapsed.as_millis(),
                workflow_file_count,
                workflow_branch_count,
                cold_start
            );

            let repo_root = workflow_snapshot.root.clone();
            let line_stats_repo_root = repo_root.clone();
            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.snapshot_epoch {
                        return;
                    }
                    this.auto_refresh_unmodified_streak = 0;
                    this.last_snapshot_fingerprint = Some(fingerprint);
                    this.workflow_loading = false;
                    let diff_changed = this.apply_workflow_snapshot(*workflow_snapshot, true, cx);
                    if let Some(line_stats_scope) =
                        this.take_line_stats_refresh_scope(request, diff_changed)
                    {
                        this.schedule_line_stats_refresh(
                            line_stats_repo_root.clone(),
                            request,
                            line_stats_scope,
                            epoch,
                            cold_start,
                            cx,
                        );
                    } else if should_refresh_line_stats_after_snapshot(request, diff_changed) {
                        this.cancel_line_stats_refresh();
                    } else {
                        this.pending_dirty_paths.clear();
                    }
                });
            } else {
                return;
            }

            let reconcile_repo_root = repo_root;
            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.snapshot_epoch {
                        return;
                    }

                    this.finish_snapshot_refresh_loading();
                    let elapsed = started_at.elapsed();
                    debug!(
                        "git workspace refresh complete: epoch={} force={} priority={} behavior={} total_elapsed_ms={} cold_start={} line_stats_pending={}",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        request.behavior.as_str(),
                        elapsed.as_millis(),
                        cold_start,
                        this.line_stats_loading
                    );

                    cx.notify();
                    this.maybe_run_pending_snapshot_refresh(cx);
                });
            } else {
                return;
            }

            if !should_run_cold_start_reconcile {
                return;
            }

            let reconcile_started_at = Instant::now();
            let reconcile_result = cx
                .background_executor()
                .spawn(async move { load_snapshot_fingerprint(&reconcile_repo_root) })
                .await;

            match &reconcile_result {
                Ok(_) => {
                    debug!(
                        "git workspace cold-start reconcile probe complete: epoch={} force={} priority={} elapsed_ms={} cold_start={}",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        reconcile_started_at.elapsed().as_millis(),
                        cold_start
                    );
                }
                Err(err) => {
                    warn!(
                        "git workspace cold-start reconcile probe failed: epoch={} force={} priority={} elapsed_ms={} cold_start={} err={err:#}",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        reconcile_started_at.elapsed().as_millis(),
                        cold_start
                    );
                }
            }

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.snapshot_epoch {
                        return;
                    }

                    let Ok(reconciled_fingerprint) = reconcile_result else {
                        return;
                    };
                    if this.last_snapshot_fingerprint.as_ref() == Some(&reconciled_fingerprint) {
                        return;
                    }

                    debug!(
                        "git workspace cold-start reconcile detected drift: epoch={} force={} priority={} behavior={} cold_start={} -> scheduling foreground refresh",
                        epoch,
                        request.force,
                        request.priority.as_str(),
                        request.behavior.as_str(),
                        cold_start
                    );
                    this.request_snapshot_refresh_internal(
                        SnapshotRefreshRequest::user(false),
                        cx,
                    );
                });
            }
        });
    }

    pub(super) fn open_project_picker(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });

        self.open_project_task = cx.spawn(async move |this, cx| {
            let selection = match prompt.await {
                Ok(selection) => selection,
                Err(err) => {
                    error!("project picker prompt channel closed: {err}");
                    return;
                }
            };

            let selected_path = match selection {
                Ok(Some(paths)) => paths.into_iter().next(),
                Ok(None) => None,
                Err(err) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.git_status_message =
                                Some(format!("Failed to open folder picker: {err:#}"));
                            cx.notify();
                        });
                    }
                    return;
                }
            };

            let Some(selected_path) = selected_path else {
                return;
            };
            let canonical_project_root = match cx
                .background_executor()
                .spawn({
                    let selected_path = selected_path.clone();
                    async move { Self::canonical_workspace_project_root(selected_path.as_path()) }
                })
                .await
            {
                Ok(project_root) => project_root,
                Err(err) => {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.git_status_message = Some(format!(
                                "Selected folder is not a Git repository: {err:#}"
                            ));
                            cx.notify();
                        });
                    }
                    return;
                }
            };

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.activate_workspace_project_root(canonical_project_root.clone(), cx);
                });
            }
        });
    }

    fn apply_lightweight_git_index_snapshot(
        &mut self,
        root: PathBuf,
        fingerprint: RepoSnapshotFingerprint,
        snapshot: WorkflowSnapshot,
    ) {
        let root_is_primary = self.repo_root.as_ref() == Some(&root);
        let root_is_selected_workspace = self.selected_git_workspace_root().as_ref() == Some(&root);

        if root_is_primary {
            self.last_snapshot_fingerprint = Some(fingerprint.clone());
            self.apply_primary_git_index_snapshot(snapshot);
            if root_is_selected_workspace {
                self.last_git_workspace_fingerprint = Some(fingerprint);
                self.sync_git_workspace_with_primary_state();
            }
            return;
        }

        if !root_is_selected_workspace {
            return;
        }

        self.last_git_workspace_fingerprint = Some(fingerprint);
        self.apply_git_workspace_index_snapshot(root, snapshot);
    }

    fn apply_primary_git_index_snapshot(&mut self, snapshot: WorkflowSnapshot) {
        let WorkflowSnapshot {
            working_copy_commit_id,
            files,
            last_commit_subject,
            ..
        } = snapshot;

        self.working_copy_commit_id = Some(working_copy_commit_id);
        self.files = files;
        self.file_status_by_path = self
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.file_line_stats
            .retain(|path, _| self.files.iter().any(|file| file.path == *path));
        self.recompute_overall_line_stats_from_file_stats();
        self.collapsed_files
            .retain(|path| self.files.iter().any(|file| file.path == *path));
        if self.workspace_view_mode == WorkspaceViewMode::Files {
            self.selected_path = retained_selection_path(&self.files, self.selected_path.as_deref());
            self.selected_status = self
                .selected_path
                .as_deref()
                .and_then(|selected| self.status_for_path(selected));
        }
        self.last_commit_subject = last_commit_subject;
        self.persist_workflow_cache();
    }

    fn apply_git_workspace_index_snapshot(&mut self, root: PathBuf, snapshot: WorkflowSnapshot) {
        let WorkflowSnapshot {
            working_copy_commit_id,
            files,
            last_commit_subject,
            ..
        } = snapshot;

        self.git_workspace.root = Some(root);
        self.git_workspace.working_copy_commit_id = Some(working_copy_commit_id);
        self.git_workspace.files = files;
        self.git_workspace.file_status_by_path = self
            .git_workspace
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.git_workspace
            .file_line_stats
            .retain(|path, _| self.git_workspace.files.iter().any(|file| file.path == *path));
        self.git_workspace.overall_line_stats = Self::sum_line_stats(
            self.git_workspace
                .files
                .iter()
                .filter_map(|file| {
                    self.git_workspace
                        .file_line_stats
                        .get(file.path.as_str())
                        .copied()
                }),
        );
        self.last_commit_subject = last_commit_subject;
    }

    fn apply_workflow_snapshot(
        &mut self,
        snapshot: WorkflowSnapshot,
        full_refresh: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let WorkflowSnapshot {
            root,
            working_copy_commit_id,
            branch_name,
            branch_has_upstream,
            branch_ahead_count,
            branch_behind_count,
            branches,
            files,
            last_commit_subject,
        } = snapshot;

        debug!("loaded workflow snapshot from {}", root.display());
        let primary_root =
            hunk_git::worktree::primary_repo_root(root.as_path()).unwrap_or_else(|_| root.clone());
        let root_changed = self.repo_root.as_ref() != Some(&primary_root);
        let previous_selected_path = self.selected_path.clone();
        let previous_selected_status = self.selected_status;
        let previous_files = self.files.clone();
        let previous_working_copy_commit_id = self.working_copy_commit_id.clone();

        let previous_ai_workspace_key = self.ai_workspace_key();
        self.sync_ai_visible_composer_prompt_to_draft(cx);
        self.project_path = Some(primary_root.clone());
        self.set_active_workspace_project_path(self.project_path.clone());
        self.repo_root = Some(primary_root.clone());
        self.branches = branches;
        self.working_copy_commit_id = Some(working_copy_commit_id);
        self.branch_name = branch_name;
        self.branch_has_upstream = branch_has_upstream;
        self.branch_ahead_count = branch_ahead_count;
        self.branch_behind_count = branch_behind_count;
        self.files = files;
        self.file_status_by_path = self
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.file_line_stats
            .retain(|path, _| self.files.iter().any(|file| file.path == *path));
        self.recompute_overall_line_stats_from_file_stats();
        self.last_commit_subject = last_commit_subject;
        self.sync_ai_worktree_base_branch_from_repo();
        self.sync_git_workspace_with_primary_state();
        self.sync_branch_picker_state(cx);
        self.sync_ai_worktree_base_branch_picker_state(cx);
        self.refresh_workspace_targets_from_git_state(cx);
        if self.active_workspace_target_id.is_none() {
            self.active_workspace_target_id = self
                .workspace_targets
                .iter()
                .find(|target| target.is_active)
                .map(|target| target.id.clone());
        }
        self.persist_active_workspace_target_id();
        self.ai_handle_workspace_change(previous_ai_workspace_key, cx);
        self.request_ai_composer_file_completion_reload(cx);
        self.repo_discovery_failed = false;
        self.error_message = None;
        if full_refresh {
            self.clear_comment_ui_state();
        }
        if root_changed {
            self.start_repo_watch(cx);
            if full_refresh {
                self.repo_tree.nodes.clear();
                self.repo_tree.rows.clear();
                self.repo_tree.file_count = 0;
                self.repo_tree.folder_count = 0;
                self.repo_tree.expanded_dirs.clear();
                self.repo_tree.scroll_anchor_path = None;
                self.repo_tree.row_count = 0;
                self.repo_tree.list_state.reset(0);
                self.repo_tree.error = None;
                self.repo_tree.changed_only = false;
                self.clear_full_repo_tree_cache();
                self.clear_editor_state(cx);
            }
        }
        self.collapsed_files
            .retain(|path| self.files.iter().any(|file| file.path == *path));
        if self.workspace_view_mode == WorkspaceViewMode::Files {
            let current_selection = self.selected_path.clone();
            self.selected_path = if full_refresh {
                current_selection.or_else(|| self.files.first().map(|file| file.path.clone()))
            } else {
                retained_selection_path(&self.files, current_selection.as_deref())
            };
            self.selected_status = self
                .selected_path
                .as_deref()
                .and_then(|selected| self.status_for_path(selected));
        }

        if full_refresh {
            let selected_changed = self.selected_path != previous_selected_path
                || self.selected_status != previous_selected_status;
            let file_list_changed = previous_files != self.files;
            let diff_changed = diff_state_changed(
                root_changed,
                previous_working_copy_commit_id.as_deref()
                    != self.working_copy_commit_id.as_deref(),
                file_list_changed,
            );

            self.refresh_comments_cache_from_store();

            let should_reload_repo_tree = should_reload_repo_tree_after_snapshot(
                root_changed,
                self.workspace_view_mode.supports_sidebar_tree(),
                file_list_changed,
            );
            if should_reload_repo_tree {
                self.request_repo_tree_reload(cx);
            }

            self.bootstrap_files_workspace_if_needed(cx);

            if !should_reload_diff_after_snapshot(
                self.workspace_view_mode.supports_diff_stream(),
                diff_changed,
                self.active_diff_row_count() == 0,
            ) {
                self.scroll_selected_after_reload = false;
            } else {
                self.scroll_selected_after_reload =
                    should_scroll_selected_after_reload(
                        selected_changed,
                        self.active_diff_row_count() == 0,
                    );
                self.request_selected_diff_reload(cx);
            }

            self.persist_workflow_cache();
            cx.notify();
            return diff_changed;
        }

        self.persist_workflow_cache();
        if self.git_workspace.root.is_none()
            || self
                .selected_git_workspace_root()
                .is_some_and(|selected_root| self.git_workspace.root.as_ref() != Some(&selected_root))
        {
            self.request_git_workspace_refresh(true, cx);
        }
        cx.notify();
        false
    }

    fn apply_snapshot_error(&mut self, err: anyhow::Error, cx: &mut Context<Self>) {
        let missing_repository = Self::is_missing_repository_error(&err);
        let error_message = Self::format_error_chain(&err);
        self.finish_snapshot_refresh_loading();
        self.workflow_loading = false;

        if !missing_repository {
            self.repo_discovery_failed = false;
            self.error_message = Some(error_message);
            cx.notify();
            return;
        }
        self.reset_to_empty_workspace_state(true, cx);
    }

    fn format_error_chain(err: &anyhow::Error) -> String {
        err.chain()
            .enumerate()
            .map(|(index, cause)| {
                if index == 0 {
                    cause.to_string()
                } else {
                    format!("caused by ({index}): {cause}")
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn is_missing_repository_error(err: &anyhow::Error) -> bool {
        err.chain().any(|cause| {
            let message = cause.to_string();
            message.contains("failed to discover git repository")
                || message.contains("could not find repository")
        })
    }
}
