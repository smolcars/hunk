impl DiffViewer {
    pub(super) fn open_project_action(
        &mut self,
        _: &OpenProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_project_picker(cx);
    }

    pub(super) fn confirm_remove_workspace_project_action(
        &mut self,
        project_path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.state.contains_workspace_project(project_path.as_path()) {
            self.git_status_message = Some("Project is not part of the workspace.".to_string());
            cx.notify();
            return;
        }

        let project_name =
            crate::app::project_picker::project_display_name(project_path.as_path());
        let project_path_label = project_path.display().to_string();
        let view = cx.entity();

        gpui_component::WindowExt::open_alert_dialog(window, cx, move |alert, _, cx| {
            alert
                .width(px(460.0))
                .title("Remove Project?")
                .description(format!(
                    "Remove project '{}' from the workspace? The repository stays on disk and can be added again later.",
                    project_name
                ))
                .button_props(
                    gpui_component::dialog::DialogButtonProps::default()
                        .ok_text("Remove Project")
                        .ok_variant(gpui_component::button::ButtonVariant::Danger)
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .whitespace_normal()
                        .child(project_path_label.clone()),
                )
                .on_ok({
                    let view = view.clone();
                    let project_path = project_path.clone();
                    move |_, window, cx| {
                        view.update(cx, |this, cx| {
                            this.remove_workspace_project_by_path(
                                project_path.as_path(),
                                window,
                                cx,
                            );
                        });
                        true
                    }
                })
        });
    }

    fn remove_workspace_project_by_path(
        &mut self,
        project_path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let removed_project_name =
            crate::app::project_picker::project_display_name(project_path);
        let was_active_project = self.project_path.as_deref() == Some(project_path);

        if !self.state.remove_workspace_project(project_path) {
            self.git_status_message = Some("Project is not part of the workspace.".to_string());
            cx.notify();
            return;
        }

        let next_active_project = self.state.active_project_path().cloned();
        let removed_last_project = next_active_project.is_none();
        self.persist_state();
        self.discard_workspace_project_state(project_path);
        self.discard_files_terminal_state_for_project(
            project_path,
            "removing project from workspace",
        );

        if was_active_project {
            if let Some(next_active_project) = next_active_project {
                self.activate_workspace_project_root(next_active_project, cx);
            } else {
                self.reset_to_empty_workspace_state(false, cx);
            }
        }

        self.discard_workspace_project_state(project_path);
        self.update_project_picker_state(window, cx);
        self.remove_ai_workspace_states_for_project(project_path, cx);
        self.rebuild_ai_thread_sidebar_state();
        self.invalidate_ai_visible_frame_state_with_reason("refresh");
        self.git_status_message = Some(if removed_last_project {
            "Removed the last project from the workspace.".to_string()
        } else {
            format!("Removed '{}' from the workspace.", removed_project_name)
        });
        cx.notify();
    }

    fn remove_ai_workspace_states_for_project(
        &mut self,
        project_path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        for workspace_key in removed_project_workspace_keys(project_path) {
            self.shutdown_ai_runtime_for_workspace_blocking(workspace_key.as_str());
            self.ai_forget_deleted_workspace_state(workspace_key.as_str(), cx);
        }
    }

    pub(super) fn status_for_path(&self, path: &str) -> Option<FileStatus> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.review_file_status_by_path.get(path).copied()
        } else {
            self.file_status_by_path.get(path).copied()
        }
    }

    pub(super) fn request_snapshot_refresh(&mut self, cx: &mut Context<Self>) {
        self.request_snapshot_refresh_internal(SnapshotRefreshRequest::user(false), cx);
    }

    pub(super) fn request_snapshot_refresh_workflow_only(
        &mut self,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let request = if force {
            SnapshotRefreshRequest::user(true)
        } else {
            SnapshotRefreshRequest::background()
        };
        self.request_snapshot_refresh_internal(request, cx);
    }

    fn enqueue_snapshot_refresh(&mut self, request: SnapshotRefreshRequest) {
        self.pending_snapshot_refresh = Some(
            self.pending_snapshot_refresh
                .map_or(request, |pending| pending.merge(request)),
        );
    }

    fn active_snapshot_refresh_request(&self) -> SnapshotRefreshRequest {
        self.snapshot_active_request
            .unwrap_or(SnapshotRefreshRequest::background())
    }

    fn should_preempt_active_snapshot_refresh(&self, request: SnapshotRefreshRequest) -> bool {
        self.snapshot_loading && request.is_more_urgent_than(self.active_snapshot_refresh_request())
    }

    fn finish_snapshot_refresh_loading(&mut self) {
        self.snapshot_loading = false;
        self.snapshot_active_request = None;
        self.workspace_target_switch_loading = false;
    }

    fn enqueue_line_stats_refresh(&mut self, refresh: PendingLineStatsRefresh) {
        self.pending_line_stats_refresh = Some(
            self.pending_line_stats_refresh
                .take()
                .map_or(refresh.clone(), |pending| pending.merge(refresh)),
        );
    }

    fn maybe_run_pending_line_stats_refresh(&mut self, cx: &mut Context<Self>) {
        if self.line_stats_loading {
            return;
        }
        let Some(refresh) = self.pending_line_stats_refresh.take() else {
            return;
        };
        self.schedule_line_stats_refresh(
            refresh.repo_root,
            refresh.request,
            refresh.scope,
            refresh.snapshot_epoch,
            refresh.cold_start,
            cx,
        );
    }

    fn schedule_line_stats_refresh(
        &mut self,
        repo_root: PathBuf,
        request: SnapshotRefreshRequest,
        scope: LineStatsRefreshScope,
        snapshot_epoch: usize,
        cold_start: bool,
        cx: &mut Context<Self>,
    ) {
        let refresh_root = repo_root.display().to_string();
        let scope_label = scope.label();
        let path_count = scope.path_count();
        let queued_refresh = PendingLineStatsRefresh {
            repo_root: repo_root.clone(),
            request,
            scope: scope.clone(),
            snapshot_epoch,
            cold_start,
        };
        if self.line_stats_loading {
            self.enqueue_line_stats_refresh(queued_refresh);
            debug!(
                "git workspace line stats refresh deferred: snapshot_epoch={} force={} priority={} scope={} path_count={} cold_start={}",
                snapshot_epoch,
                request.force,
                request.priority.as_str(),
                scope_label,
                path_count,
                cold_start
            );
            return;
        }
        let epoch = self.next_line_stats_epoch();
        let scope_for_load = scope.clone();
        let debounce = (request.priority == SnapshotRefreshPriority::Background)
            .then_some(Self::LINE_STATS_BACKGROUND_DEBOUNCE);
        self.line_stats_loading = true;
        debug!(
            "git workspace line stats refresh start: epoch={} snapshot_epoch={} force={} priority={} scope={} path_count={} cold_start={} root={}",
            epoch,
            snapshot_epoch,
            request.force,
            request.priority.as_str(),
            scope_label,
            path_count,
            cold_start,
            refresh_root
        );

        self.line_stats_task = cx.spawn(async move |this, cx| {
            let started_at = Instant::now();
            if let Some(delay) = debounce {
                cx.background_executor().timer(delay).await;
            }

            let (result_tx, result_rx) = oneshot::channel();
            let spawn_result = std::thread::Builder::new()
                .name("hunk-line-stats".to_string())
                .spawn(move || {
                    let result = match &scope_for_load {
                        LineStatsRefreshScope::Full => {
                            load_repo_file_line_stats_without_refresh(&repo_root)
                        }
                        LineStatsRefreshScope::Paths(paths) => {
                            load_repo_file_line_stats_for_paths_without_refresh(&repo_root, paths)
                        }
                    };
                    let _ = result_tx.send(result);
                });
            let result = match spawn_result {
                Ok(_) => match result_rx.await {
                    Ok(result) => result,
                    Err(err) => Err(anyhow::anyhow!(
                        "line stats worker exited before reporting a result: {err}"
                    )),
                },
                Err(err) => Err(anyhow::anyhow!("failed to spawn line stats worker: {err}")),
            };

            match &result {
                Ok(file_line_stats) => {
                    let line_stats = Self::sum_line_stats(file_line_stats.values().copied());
                    debug!(
                        "git workspace line stats ready: epoch={} snapshot_epoch={} force={} priority={} scope={} path_count={} elapsed_ms={} files={} added={} removed={} changed={} cold_start={}",
                        epoch,
                        snapshot_epoch,
                        request.force,
                        request.priority.as_str(),
                        scope_label,
                        path_count,
                        started_at.elapsed().as_millis(),
                        file_line_stats.len(),
                        line_stats.added,
                        line_stats.removed,
                        line_stats.changed(),
                        cold_start
                    );
                }
                Err(err) => {
                    error!(
                        "git workspace line stats load failed: epoch={} snapshot_epoch={} force={} priority={} scope={} path_count={} elapsed_ms={} cold_start={} err={err:#}",
                        epoch,
                        snapshot_epoch,
                        request.force,
                        request.priority.as_str(),
                        scope_label,
                        path_count,
                        started_at.elapsed().as_millis(),
                        cold_start
                    );
                }
            }

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.line_stats_epoch {
                        return;
                    }

                    this.line_stats_loading = false;
                    this.line_stats_task = Task::ready(());
                    if let Ok(file_line_stats) = result {
                        match scope {
                            LineStatsRefreshScope::Full => {
                                this.file_line_stats = file_line_stats;
                            }
                            LineStatsRefreshScope::Paths(paths) => {
                                for path in paths {
                                    this.file_line_stats.remove(path.as_str());
                                }
                                this.file_line_stats.extend(file_line_stats);
                            }
                        }
                        this.recompute_overall_line_stats_from_file_stats();
                        this.sync_git_workspace_with_primary_state();
                    }
                    cx.notify();
                    this.maybe_run_pending_line_stats_refresh(cx);
                });
            }
        });
    }

    fn take_line_stats_refresh_scope(
        &mut self,
        request: SnapshotRefreshRequest,
        diff_changed: bool,
    ) -> Option<LineStatsRefreshScope> {
        if self.files.is_empty() {
            self.pending_dirty_paths.clear();
            return None;
        }

        if request.priority == SnapshotRefreshPriority::Background
            && matches!(request.behavior, SnapshotRefreshBehavior::ReadOnly)
        {
            self.pending_dirty_paths.clear();
            return None;
        }

        if !diff_changed {
            self.pending_dirty_paths.clear();
            let missing_paths = missing_line_stat_paths(&self.files, &self.file_line_stats);
            return (!missing_paths.is_empty()).then_some(LineStatsRefreshScope::Paths(missing_paths));
        }

        if request.priority == SnapshotRefreshPriority::Background {
            let pending_dirty_paths = std::mem::take(&mut self.pending_dirty_paths);
            if !pending_dirty_paths.is_empty() {
                let dirty_paths =
                    line_stats_paths_from_dirty_paths(&self.files, &pending_dirty_paths);
                if !dirty_paths.is_empty() {
                    return Some(LineStatsRefreshScope::Paths(dirty_paths));
                }
                return Some(LineStatsRefreshScope::Full);
            }
        } else {
            self.pending_dirty_paths.clear();
        }

        Some(LineStatsRefreshScope::Full)
    }

    fn sum_line_stats<I>(stats: I) -> LineStats
    where
        I: IntoIterator<Item = LineStats>,
    {
        let mut total = LineStats::default();
        for line_stats in stats {
            total.added = total.added.saturating_add(line_stats.added);
            total.removed = total.removed.saturating_add(line_stats.removed);
        }
        total
    }

    fn recompute_overall_line_stats_from_file_stats(&mut self) {
        self.overall_line_stats = Self::sum_line_stats(
            self.files
                .iter()
                .filter_map(|file| self.file_line_stats.get(file.path.as_str()).copied()),
        );
    }

    fn merged_git_workspace_branches(
        mut local_branches: Vec<LocalBranch>,
        remote_branches: &[LocalBranch],
    ) -> Vec<LocalBranch> {
        local_branches.extend(remote_branches.iter().cloned());
        local_branches
    }

    fn remote_branches_without_local_duplicates(
        local_branches: &[LocalBranch],
        remote_branches: &[LocalBranch],
    ) -> Vec<LocalBranch> {
        remote_branches
            .iter()
            .filter(|remote_branch| {
                let Some((_, remote_branch_name)) = remote_branch.name.split_once('/') else {
                    return true;
                };
                !local_branches
                    .iter()
                    .any(|local_branch| local_branch.name == remote_branch_name)
            })
            .cloned()
            .collect()
    }

    fn current_git_workspace_local_branches(&self) -> Vec<LocalBranch> {
        self.git_workspace
            .branches
            .iter()
            .filter(|branch| !branch.is_remote_tracking)
            .cloned()
            .collect()
    }

    fn update_git_workspace_remote_branches(
        &mut self,
        remote_branches: Vec<LocalBranch>,
        cx: &mut Context<Self>,
    ) {
        let local_branches = if self.selected_git_workspace_root() == self.repo_root {
            self.branches.clone()
        } else {
            self.current_git_workspace_local_branches()
        };
        self.git_workspace.remote_branches =
            Self::remote_branches_without_local_duplicates(&local_branches, &remote_branches);
        self.git_workspace.branches = Self::merged_git_workspace_branches(
            local_branches,
            &self.git_workspace.remote_branches,
        );
        self.sync_branch_picker_state(cx);
    }

    fn sync_git_workspace_with_primary_state(&mut self) {
        let Some(repo_root) = self.repo_root.clone() else {
            return;
        };
        if self.selected_git_workspace_root().as_ref() != Some(&repo_root) {
            return;
        }

        self.git_workspace.root = Some(repo_root);
        self.git_workspace.working_copy_commit_id = self.working_copy_commit_id.clone();
        self.git_workspace.branch_name = self.branch_name.clone();
        self.git_workspace.branch_has_upstream = self.branch_has_upstream;
        self.git_workspace.branch_ahead_count = self.branch_ahead_count;
        self.git_workspace.branch_behind_count = self.branch_behind_count;
        self.git_workspace.remote_branches =
            Self::remote_branches_without_local_duplicates(
                &self.branches,
                &self.git_workspace.remote_branches,
            );
        self.git_workspace.branches = Self::merged_git_workspace_branches(
            self.branches.clone(),
            &self.git_workspace.remote_branches,
        );
        self.git_workspace.files = self.files.clone();
        self.git_workspace.file_status_by_path = self.file_status_by_path.clone();
        self.git_workspace.file_line_stats = self.file_line_stats.clone();
        self.git_workspace.overall_line_stats = self.overall_line_stats;
    }

    fn next_git_workspace_refresh_epoch(&mut self) -> usize {
        self.git_workspace_refresh_epoch = self.git_workspace_refresh_epoch.saturating_add(1);
        self.git_workspace_refresh_epoch
    }

    fn maybe_run_pending_git_workspace_refresh(&mut self, cx: &mut Context<Self>) {
        if self.git_workspace_loading {
            return;
        }
        let Some(request) = self.pending_git_workspace_refresh.take() else {
            return;
        };
        debug!(
            "git workspace refresh running queued refresh: recent_commits={} root={}",
            request.refresh_recent_commits,
            request.root.display()
        );
        self.request_git_workspace_refresh(request.refresh_recent_commits, cx);
    }

    fn clear_git_workspace_state(&mut self) {
        self.git_workspace = GitWorkspaceState::default();
        self.last_commit_subject = None;
        self.recent_commits.clear();
        self.recent_commits_error = None;
        self.last_recent_commits_fingerprint = None;
        self.git_workspace_active_root = None;
        self.git_workspace_loading = false;
        self.pending_git_workspace_refresh = None;
        self.last_git_workspace_fingerprint = None;
        self.workspace_target_switch_loading = false;
    }

    fn apply_git_workspace_snapshot(
        &mut self,
        root: PathBuf,
        snapshot: WorkflowSnapshot,
        remote_branches: Vec<LocalBranch>,
        file_line_stats: BTreeMap<String, LineStats>,
        cx: &mut Context<Self>,
    ) {
        let previous_working_copy_commit_id = self.git_workspace.working_copy_commit_id.clone();
        let previous_files = self.git_workspace.files.clone();
        let WorkflowSnapshot {
            working_copy_commit_id,
            branch_name,
            branch_has_upstream,
            branch_ahead_count,
            branch_behind_count,
            branches,
            files,
            last_commit_subject,
            ..
        } = snapshot;

        self.git_workspace.root = Some(root.clone());
        self.git_workspace.working_copy_commit_id = Some(working_copy_commit_id);
        self.git_workspace.branch_name = branch_name;
        self.git_workspace.branch_has_upstream = branch_has_upstream;
        self.git_workspace.branch_ahead_count = branch_ahead_count;
        self.git_workspace.branch_behind_count = branch_behind_count;
        self.git_workspace.remote_branches =
            Self::remote_branches_without_local_duplicates(&branches, &remote_branches);
        self.git_workspace.branches = Self::merged_git_workspace_branches(
            branches,
            &self.git_workspace.remote_branches,
        );
        self.git_workspace.files = files;
        self.git_workspace.file_status_by_path = self
            .git_workspace
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.git_workspace.file_line_stats = file_line_stats;
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

        let diff_changed = previous_working_copy_commit_id.as_deref()
            != self.git_workspace.working_copy_commit_id.as_deref()
            || previous_files != self.git_workspace.files;
        if diff_changed
            && self.workspace_view_mode == WorkspaceViewMode::Diff
            && self.review_compare_references_workspace_root(root.as_path())
        {
            self.request_selected_diff_reload(cx);
        }
        self.sync_branch_picker_state(cx);
    }

    pub(super) fn request_git_workspace_refresh(&mut self, refresh_recent_commits: bool, cx: &mut Context<Self>) {
        let Some(root) = self.selected_git_workspace_root() else {
            self.clear_git_workspace_state();
            cx.notify();
            return;
        };

        let request = GitWorkspaceRefreshRequest::new(root.clone(), refresh_recent_commits);
        if self.git_workspace_loading {
            if self.git_workspace_active_root.as_ref() == Some(&root) {
                let queued_request = match self.pending_git_workspace_refresh.take() {
                    Some(pending) => pending.merge(request),
                    None => request,
                };
                debug!(
                    "git workspace refresh deferred: epoch={} recent_commits={} root={}",
                    self.git_workspace_refresh_epoch,
                    queued_request.refresh_recent_commits,
                    queued_request.root.display()
                );
                self.pending_git_workspace_refresh = Some(queued_request);
                return;
            }

            debug!(
                "git workspace refresh preempted: epoch={} active_root={} next_root={}",
                self.git_workspace_refresh_epoch,
                self.git_workspace_active_root
                    .as_deref()
                    .map_or_else(|| "<unknown>".to_string(), |path| path.display().to_string()),
                root.display()
            );
            self.next_git_workspace_refresh_epoch();
            self.git_workspace_refresh_task = Task::ready(());
            self.git_workspace_loading = false;
            self.pending_git_workspace_refresh = None;
        }

        let previous_fingerprint = (self.git_workspace.root.as_ref() == Some(&root))
            .then(|| self.last_git_workspace_fingerprint.clone())
            .flatten();
        let epoch = self.next_git_workspace_refresh_epoch();
        self.git_workspace_loading = true;
        self.git_workspace_active_root = Some(root.clone());
        let refresh_root = root.clone();
        debug!(
            "git workspace state refresh start: epoch={} recent_commits={} root={} cached_fingerprint={}",
            epoch,
            refresh_recent_commits,
            refresh_root.display(),
            previous_fingerprint.is_some()
        );

        self.git_workspace_refresh_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let (fingerprint, workflow_snapshot) =
                    load_workflow_snapshot_if_changed_without_refresh(
                        refresh_root.as_path(),
                        previous_fingerprint.as_ref(),
                    )?;
                let remote_branches =
                    load_remote_tracking_branches_without_refresh(refresh_root.as_path())?;
                let file_line_stats = if let Some(workflow_snapshot) = workflow_snapshot.as_ref() {
                    if workflow_snapshot.files.is_empty() {
                        BTreeMap::new()
                    } else {
                        load_repo_file_line_stats_without_refresh(refresh_root.as_path())?
                    }
                } else {
                    BTreeMap::new()
                };
                Ok::<_, anyhow::Error>((
                    fingerprint,
                    workflow_snapshot,
                    remote_branches,
                    file_line_stats,
                ))
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_workspace_refresh_epoch {
                        return;
                    }

                    this.git_workspace_loading = false;
                    this.git_workspace_active_root = None;
                    this.workspace_target_switch_loading = false;
                    match result {
                        Ok((fingerprint, Some(workflow_snapshot), remote_branches, file_line_stats)) => {
                            debug!(
                                "git workspace state refresh complete: epoch={} recent_commits={} root={} files={}",
                                epoch,
                                refresh_recent_commits,
                                root.display(),
                                workflow_snapshot.files.len()
                            );
                            this.last_git_workspace_fingerprint = Some(fingerprint);
                            this.apply_git_workspace_snapshot(
                                root.clone(),
                                workflow_snapshot,
                                remote_branches,
                                file_line_stats,
                                cx,
                            );
                            if refresh_recent_commits {
                                this.request_recent_commits_refresh(true, cx);
                            }
                        }
                        Ok((fingerprint, None, remote_branches, _)) => {
                            debug!(
                                "git workspace state refresh skipped: epoch={} recent_commits={} root={} (no repo changes)",
                                epoch,
                                refresh_recent_commits,
                                root.display()
                            );
                            this.last_git_workspace_fingerprint = Some(fingerprint);
                            this.update_git_workspace_remote_branches(remote_branches, cx);
                            if refresh_recent_commits {
                                this.request_recent_commits_refresh(true, cx);
                            }
                        }
                        Err(err) => {
                            this.git_status_message =
                                Some(format!("Failed to load Git workspace: {err:#}"));
                            this.last_git_workspace_fingerprint = None;
                            if this.git_workspace.root.as_ref() != Some(&root) {
                                this.clear_git_workspace_state();
                            }
                        }
                    }
                    cx.notify();
                    this.maybe_run_pending_git_workspace_refresh(cx);
                });
            }
        });
    }
}

fn removed_project_workspace_keys(project_path: &std::path::Path) -> Vec<String> {
    let mut workspace_keys = std::collections::BTreeSet::from([project_path
        .to_string_lossy()
        .to_string()]);

    if let Ok(targets) = hunk_git::worktree::list_workspace_targets(project_path) {
        for target in targets {
            workspace_keys.insert(target.root.to_string_lossy().to_string());
        }
    }

    workspace_keys.into_iter().collect()
}
