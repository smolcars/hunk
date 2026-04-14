#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CombinedWorkspaceCommitAndPushBlocker {
    Busy,
    MissingBranch,
    MissingRepo,
    NoChanges,
}

impl CombinedWorkspaceCommitAndPushBlocker {
    const fn message(self) -> &'static str {
        match self {
            Self::Busy => "Another workspace action is in progress.",
            Self::MissingBranch => "Activate a branch before committing and pushing.",
            Self::MissingRepo => "No Git repository available.",
            Self::NoChanges => "No changed files to stage and commit.",
        }
    }
}

impl DiffViewer {
    pub(super) fn git_controls_busy(&self) -> bool {
        self.git_action_loading || self.workspace_target_switch_loading
    }

    fn git_index_action_loading(&self) -> bool {
        self.git_action_label.as_deref().is_some_and(|label| {
            label.eq_ignore_ascii_case("Stage files")
                || label.eq_ignore_ascii_case("Unstage files")
        })
    }

    pub(super) fn git_rail_controls_busy(&self) -> bool {
        self.workspace_target_switch_loading
            || (self.git_action_loading && !self.git_index_action_loading())
    }

    fn set_git_warning_message(
        &mut self,
        message: String,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.git_status_message = Some(message.clone());
        Self::push_warning_notification(message, window, cx);
        cx.notify();
    }

    fn push_success_notification(message: String, cx: &mut Context<Self>) {
        let window_handles = cx.windows().into_iter().collect::<Vec<_>>();
        if window_handles.is_empty() {
            error!("cannot show git action success notification: no windows available");
            return;
        }

        for window_handle in window_handles {
            if let Err(err) = cx.update_window(window_handle, |_, window, cx| {
                gpui_component::WindowExt::push_notification(
                    window,
                    crate::app::notifications::success(message.clone()),
                    cx,
                );
            }) {
                error!("failed to show git action success notification: {err:#}");
            }
        }
    }

    fn push_error_notification(message: String, cx: &mut Context<Self>) {
        let window_handles = cx.windows().into_iter().collect::<Vec<_>>();
        if window_handles.is_empty() {
            error!("cannot show git action error notification: no windows available");
            return;
        }

        for window_handle in window_handles {
            if let Err(err) = cx.update_window(window_handle, |_, window, cx| {
                gpui_component::WindowExt::push_notification(
                    window,
                    crate::app::notifications::error(message.clone()),
                    cx,
                );
            }) {
                error!("failed to show git action error notification: {err:#}");
            }
        }
    }

    fn push_warning_notification(
        message: String,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if let Some(window) = window {
            gpui_component::WindowExt::push_notification(
                window,
                crate::app::notifications::warning(message),
                cx,
            );
            return;
        }

        let window_handles = cx.windows().into_iter().collect::<Vec<_>>();
        if window_handles.is_empty() {
            error!("cannot show git action warning notification: no windows available");
            return;
        }

        for window_handle in window_handles {
            if let Err(err) = cx.update_window(window_handle, |_, window, cx| {
                gpui_component::WindowExt::push_notification(
                    window,
                    crate::app::notifications::warning(message.clone()),
                    cx,
                );
            }) {
                error!("failed to show git action warning notification: {err:#}");
            }
        }
    }

    fn next_git_action_epoch(&mut self) -> usize {
        self.git_action_epoch = self.git_action_epoch.saturating_add(1);
        self.git_action_epoch
    }

    fn begin_git_action(&mut self, action_label: impl Into<String>, cx: &mut Context<Self>) -> usize {
        let epoch = self.next_git_action_epoch();
        self.git_action_loading = true;
        self.git_action_label = Some(action_label.into());
        self.ai_git_progress = None;
        cx.notify();
        epoch
    }

    fn finish_git_action(&mut self) {
        self.git_action_loading = false;
        self.git_action_label = None;
        self.ai_git_progress = None;
    }

    fn refresh_after_git_action(&mut self, action_name: &'static str, cx: &mut Context<Self>) {
        let plan = crate::app::refresh_policy::post_git_action_refresh_plan(
            action_name,
            self.selected_git_workspace_root() == self.repo_root,
        );
        if plan.refresh_primary_snapshot {
            self.request_snapshot_refresh_workflow_only(true, cx);
        }
        if plan.refresh_git_workspace {
            self.request_git_workspace_refresh(false, cx);
        }
        if plan.refresh_recent_commits {
            self.request_recent_commits_refresh(true, cx);
        }
    }

    fn apply_optimistic_commit_success(&mut self, subject: &str) {
        self.last_commit_subject = Some(subject.to_string());

        if self.git_workspace.branch_has_upstream {
            self.git_workspace.branch_ahead_count =
                self.git_workspace.branch_ahead_count.saturating_add(1);
        }
    }

    fn remove_paths_from_git_workspace(&mut self, removed_paths: &BTreeSet<&str>) {
        self.git_workspace
            .files
            .retain(|file| !removed_paths.contains(file.path.as_str()));
        self.git_workspace.file_status_by_path = self
            .git_workspace
            .files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.git_workspace
            .file_line_stats
            .retain(|path, _| !removed_paths.contains(path.as_str()));
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
    }

    fn apply_optimistic_restore_success(&mut self, file_path: &str) {
        let removed_paths = [file_path].into_iter().collect::<BTreeSet<_>>();
        self.remove_paths_from_git_workspace(&removed_paths);
    }

    fn apply_optimistic_publish_success(&mut self) {
        self.git_workspace.branch_has_upstream = true;
        self.git_workspace.branch_ahead_count = 0;
        self.git_workspace.branch_behind_count = 0;
        if self.selected_git_workspace_root() == self.repo_root {
            self.branch_has_upstream = true;
            self.branch_ahead_count = 0;
            self.branch_behind_count = 0;
        }
    }

    fn apply_optimistic_push_success(&mut self) {
        self.git_workspace.branch_ahead_count = 0;
        if self.selected_git_workspace_root() == self.repo_root {
            self.branch_ahead_count = 0;
        }
    }

    fn apply_optimistic_git_action_success(&mut self, action_name: &'static str) {
        match action_name {
            "Publish branch" => self.apply_optimistic_publish_success(),
            "Push branch" => self.apply_optimistic_push_success(),
            _ => {}
        }
    }

    fn run_git_action<F>(
        &mut self,
        action_name: &'static str,
        cx: &mut Context<Self>,
        action: F,
    ) -> bool
    where
        F: FnOnce(std::path::PathBuf) -> anyhow::Result<String> + Send + 'static,
    {
        self.run_git_action_with_refresh(action_name, cx, action)
    }

    fn run_git_index_action<F>(
        &mut self,
        action_name: &'static str,
        cx: &mut Context<Self>,
        action: F,
    ) -> bool
    where
        F: FnOnce(std::path::PathBuf) -> anyhow::Result<String> + Send + 'static,
    {
        if self.git_controls_busy() {
            return false;
        }

        let Some(repo_root) = self.selected_git_workspace_root() else {
            self.git_status_message = Some("No Git repository available.".to_string());
            cx.notify();
            return false;
        };

        let epoch = self.begin_git_action(action_name, cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let refresh_root = repo_root.clone();
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = (|| -> anyhow::Result<(
                        String,
                        anyhow::Result<(RepoSnapshotFingerprint, WorkflowSnapshot)>,
                    )> {
                        let message = action(repo_root.clone())?;
                        let snapshot = load_workflow_snapshot_with_fingerprint_without_refresh(
                            repo_root.as_path(),
                        );
                        Ok((message, snapshot))
                    })();
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok((message, Ok((fingerprint, workflow_snapshot)))) => {
                            debug!(
                                "git action complete: epoch={} action={} exec_elapsed_ms={} total_elapsed_ms={} refresh=index-only",
                                epoch,
                                action_name,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.git_status_message = if message.is_empty() {
                                None
                            } else {
                                Some(message)
                            };
                            this.apply_optimistic_git_action_success(action_name);
                            this.apply_lightweight_git_index_snapshot(
                                refresh_root.clone(),
                                fingerprint,
                                workflow_snapshot,
                            );
                        }
                        Ok((message, Err(err))) => {
                            warn!(
                                "git index snapshot reload failed after action '{}': {err:#}; falling back to standard refresh",
                                action_name
                            );
                            this.git_status_message = if message.is_empty() {
                                None
                            } else {
                                Some(message)
                            };
                            this.apply_optimistic_git_action_success(action_name);
                            this.refresh_after_git_action(action_name, cx);
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action={} exec_elapsed_ms={} total_elapsed_ms={} err={err:#}",
                                epoch,
                                action_name,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("{action_name} failed: {summary}"),
                                cx,
                            );
                        }
                    }

                    cx.notify();
                });
            }
        });

        true
    }

    fn run_git_action_with_refresh<F>(
        &mut self,
        action_name: &'static str,
        cx: &mut Context<Self>,
        action: F,
    ) -> bool
    where
        F: FnOnce(std::path::PathBuf) -> anyhow::Result<String> + Send + 'static,
    {
        if self.git_controls_busy() {
            return false;
        }

        let Some(repo_root) = self.selected_git_workspace_root() else {
            self.git_status_message = Some("No Git repository available.".to_string());
            cx.notify();
            return false;
        };

        let epoch = self.begin_git_action(action_name, cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = action(repo_root);
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(message) => {
                            debug!(
                                "git action complete: epoch={} action={} exec_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                action_name,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.git_status_message = if message.is_empty() {
                                None
                            } else {
                                Some(message)
                            };
                            this.apply_optimistic_git_action_success(action_name);
                            this.refresh_after_git_action(action_name, cx);
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action={} exec_elapsed_ms={} total_elapsed_ms={} err={err:#}",
                                epoch,
                                action_name,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("{action_name} failed: {summary}"),
                                cx,
                            );
                            if action_name == "Activate branch" {
                                this.sync_branch_picker_state(cx);
                            }
                        }
                    }

                    cx.notify();
                });
            }
        });

        true
    }

    fn checkout_or_create_branch_with_options(
        &mut self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.run_git_action("Activate branch", cx, move |repo_root| {
            checkout_or_create_branch_with_change_transfer(&repo_root, &branch_name, false)?;
            Ok(format!("Activated branch {}", branch_name))
        })
    }

    fn activate_or_create_branch(&mut self, branch_name: String, cx: &mut Context<Self>) -> bool {
        let target_branch = branch_name.trim().to_string();
        if target_branch.is_empty() {
            self.set_git_warning_message("Branch name is required.".to_string(), None, cx);
            return false;
        }
        if self.checked_out_branch_name() == Some(target_branch.as_str()) {
            self.set_git_warning_message(
                format!("Branch {} is already active.", target_branch),
                None,
                cx,
            );
            return false;
        }
        self.checkout_or_create_branch_with_options(target_branch, cx)
    }

    pub(super) fn checkout_branch(&mut self, branch_name: String, cx: &mut Context<Self>) {
        self.request_activate_or_create_branch_with_dirty_guard(branch_name, None, cx);
    }

    pub(super) fn toggle_commit_file_staged(
        &mut self,
        file_path: String,
        staged: bool,
        cx: &mut Context<Self>,
    ) {
        if self.git_controls_busy() {
            return;
        }

        let message_path = file_path.clone();
        self.run_git_index_action(
            if staged { "Stage files" } else { "Unstage files" },
            cx,
            move |repo_root| {
                let paths = vec![file_path];
                if staged {
                    stage_paths(&repo_root, &paths)?;
                    Ok(format!("Staged {}", message_path))
                } else {
                    unstage_paths(&repo_root, &paths)?;
                    Ok(format!("Unstaged {}", message_path))
                }
            },
        );
    }

    pub(super) fn stage_all_files_for_commit(&mut self, cx: &mut Context<Self>) {
        if self.git_controls_busy() || !self.git_workspace.files.iter().any(|file| file.unstaged) {
            return;
        }
        let paths = self
            .git_workspace
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        self.run_git_index_action("Stage files", cx, move |repo_root| {
            stage_paths(&repo_root, &paths)?;
            Ok("Staged all changed files".to_string())
        });
    }

    pub(super) fn unstage_all_files_for_commit(&mut self, cx: &mut Context<Self>) {
        if self.git_controls_busy() {
            return;
        }
        let paths = self
            .git_workspace
            .files
            .iter()
            .filter(|file| file.staged)
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return;
        }
        self.run_git_index_action("Unstage files", cx, move |repo_root| {
            unstage_paths(&repo_root, &paths)?;
            Ok("Unstaged all staged files".to_string())
        });
    }

    pub(super) fn staged_commit_file_count(&self) -> usize {
        self.git_workspace
            .files
            .iter()
            .filter(|file| file.staged)
            .count()
    }

    pub(super) fn branch_syncable(&self) -> bool {
        !self.git_workspace.branch_name.is_empty()
            && self.git_workspace.branch_name != "unknown"
            && self.git_workspace.branch_name != "detached"
    }

    pub(super) fn checked_out_branch_name(&self) -> Option<&str> {
        if self
            .git_workspace
            .branches
            .iter()
            .any(|branch| branch.is_current && branch.name == self.git_workspace.branch_name)
        {
            return Some(self.git_workspace.branch_name.as_str());
        }

        self.git_workspace
            .branches
            .iter()
            .find(|branch| branch.is_current)
            .map(|branch| branch.name.as_str())
    }

    pub(super) fn primary_checked_out_branch_name(&self) -> Option<&str> {
        if self
            .branches
            .iter()
            .any(|branch| branch.is_current && branch.name == self.branch_name)
        {
            return Some(self.branch_name.as_str());
        }

        self.branches
            .iter()
            .find(|branch| branch.is_current)
            .map(|branch| branch.name.as_str())
    }

    pub(super) fn active_branch_is_checked_out(&self) -> bool {
        self.git_workspace
            .branches
            .iter()
            .any(|branch| branch.is_current && branch.name == self.git_workspace.branch_name)
    }

    pub(super) fn can_run_active_branch_actions(&self) -> bool {
        self.branch_syncable() && self.active_branch_is_checked_out() && !self.git_controls_busy()
    }

    pub(super) fn can_run_active_branch_actions_for_ui(&self) -> bool {
        self.branch_syncable() && self.active_branch_is_checked_out() && !self.git_rail_controls_busy()
    }

    fn tracking_area_clean(&self) -> bool {
        self.git_workspace.files.is_empty()
    }

    pub(super) fn can_sync_current_branch_for_ui(&self) -> bool {
        self.can_run_active_branch_actions_for_ui()
            && self.git_workspace.branch_has_upstream
            && self.tracking_area_clean()
            && !self.git_rail_controls_busy()
    }

    pub(super) fn can_pull_current_branch_with_rebase_for_ui(&self) -> bool {
        self.can_sync_current_branch_for_ui()
    }

    pub(super) fn can_fetch_remote_branches_for_ui(&self) -> bool {
        self.selected_git_workspace_root().is_some() && !self.git_rail_controls_busy()
    }

    pub(super) fn can_publish_current_branch_for_ui(&self) -> bool {
        self.can_run_active_branch_actions_for_ui()
            && !self.git_workspace.branch_has_upstream
            && self.tracking_area_clean()
            && !self.git_rail_controls_busy()
    }

    pub(super) fn can_push_current_branch_for_ui(&self) -> bool {
        self.can_run_active_branch_actions_for_ui()
            && self.git_workspace.branch_has_upstream
            && self.git_workspace.branch_ahead_count > 0
            && !self.git_rail_controls_busy()
    }

    fn combined_workspace_commit_and_push_blocker(
        &self,
    ) -> Option<CombinedWorkspaceCommitAndPushBlocker> {
        if self.git_rail_controls_busy() {
            return Some(CombinedWorkspaceCommitAndPushBlocker::Busy);
        }
        if !self.branch_syncable() || !self.active_branch_is_checked_out() {
            return Some(CombinedWorkspaceCommitAndPushBlocker::MissingBranch);
        }
        if self.selected_git_workspace_root().is_none() {
            return Some(CombinedWorkspaceCommitAndPushBlocker::MissingRepo);
        }
        if self.git_workspace.files.is_empty() {
            return Some(CombinedWorkspaceCommitAndPushBlocker::NoChanges);
        }
        None
    }

    pub(super) fn combined_workspace_commit_and_push_tooltip(&self) -> String {
        self.combined_workspace_commit_and_push_blocker()
            .map(|blocker| blocker.message().to_string())
            .unwrap_or_else(|| {
                "Stage all changed files, generate a commit message, create a commit, and push or publish this branch."
                    .to_string()
            })
    }

    pub(super) fn can_run_combined_workspace_commit_and_push_for_ui(&self) -> bool {
        self.combined_workspace_commit_and_push_blocker().is_none()
    }

    pub(super) fn confirm_combined_workspace_commit_and_push(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(blocker) = self.combined_workspace_commit_and_push_blocker() {
            self.set_git_warning_message(blocker.message().to_string(), Some(window), cx);
            return;
        }

        let branch_name = self.git_workspace.branch_name.clone();
        let changed_count = self.git_workspace.files.len();
        let view = cx.entity();

        gpui_component::WindowExt::open_alert_dialog(window, cx, move |alert, _, _| {
            alert
                .width(px(460.0))
                .title("Commit And Push?")
                .description(format!(
                    "Stage all {changed_count} changed file(s), generate a commit message, create a commit, and push branch '{branch_name}'?"
                ))
                .button_props(
                    gpui_component::dialog::DialogButtonProps::default()
                        .ok_text("Yes")
                        .cancel_text("No")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.run_combined_workspace_commit_and_push(cx);
                        });
                        true
                    }
                })
        });
    }

    fn run_combined_workspace_commit_and_push(&mut self, cx: &mut Context<Self>) {
        if let Some(blocker) = self.combined_workspace_commit_and_push_blocker() {
            let message = blocker.message().to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, None, cx);
            cx.notify();
            return;
        }

        let Some(repo_root) = self.selected_git_workspace_root() else {
            let message = CombinedWorkspaceCommitAndPushBlocker::MissingRepo
                .message()
                .to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, None, cx);
            cx.notify();
            return;
        };

        let changed_paths = self
            .git_workspace
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let branch_name = self.git_workspace.branch_name.clone();
        let codex_executable = Self::resolve_codex_executable_path();
        let epoch = self.begin_git_action("Commit and Push", cx);
        self.begin_ai_git_progress(
            epoch,
            AiGitProgressAction::WorkspaceCommitAndPush,
            crate::app::ai_git_progress::workspace_commit_and_push_progress_steps(),
            AiGitProgressStep::StagingFiles,
            Some(format!("Files: {}", changed_paths.len())),
            cx,
        );

        self.spawn_ai_git_action_with_progress(
            epoch,
            cx,
            move |progress_tx| {
                (|| -> anyhow::Result<(hunk_git::mutation::CreatedCommit, String)> {
                    stage_paths(repo_root.as_path(), &changed_paths)?;

                    send_ai_git_progress(
                        &progress_tx,
                        AiGitProgressStep::GeneratingCommitMessage,
                        Some(ai_branch_progress_detail("Branch", branch_name.as_str())),
                    );
                    let commit_message = try_ai_commit_message_for_staged_index(
                        AiCodexGenerationConfig {
                            codex_executable: codex_executable.as_path(),
                            repo_root: repo_root.as_path(),
                        },
                        repo_root.as_path(),
                        branch_name.as_str(),
                    )?;

                    send_ai_git_progress(
                        &progress_tx,
                        AiGitProgressStep::CreatingCommit,
                        Some(ai_commit_progress_detail(commit_message.subject.as_str())),
                    );
                    let created_commit = commit_index_with_details(
                        repo_root.as_path(),
                        commit_message.as_git_message().as_str(),
                    )?;

                    send_ai_git_progress(
                        &progress_tx,
                        AiGitProgressStep::PushingBranch,
                        Some(ai_branch_progress_detail("Branch", branch_name.as_str())),
                    );
                    push_current_branch_with_publish_fallback(
                        repo_root.as_path(),
                        branch_name.as_str(),
                    )?;

                    Ok((created_commit, branch_name))
                })()
            },
            move |this, result, execution_elapsed, total_elapsed, cx| {
                if epoch != this.git_action_epoch {
                    return;
                }

                this.finish_git_action();
                match result {
                    Ok((created_commit, branch_name)) => {
                        debug!(
                            "git action complete: epoch={} action=Commit and Push exec_elapsed_ms={} total_elapsed_ms={} branch={}",
                            epoch,
                            execution_elapsed.as_millis(),
                            total_elapsed.as_millis(),
                            branch_name
                        );
                        this.apply_optimistic_commit_success(created_commit.subject.as_str());
                        this.apply_optimistic_recent_commit(&created_commit);
                        this.request_snapshot_refresh_workflow_only(true, cx);
                        this.request_git_workspace_refresh(false, cx);
                        this.request_recent_commits_refresh(true, cx);

                        let commit_input_state = this.commit_input_state.clone();
                        if let Some(window_handle) = cx.windows().into_iter().next()
                            && let Err(err) = cx.update_window(window_handle, |_, window, cx| {
                                commit_input_state.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                            })
                        {
                            error!(
                                "failed to clear commit input after combined commit and push: {err:#}"
                            );
                        }

                        let message = format!("Committed and pushed {}", branch_name);
                        this.git_status_message = Some(message.clone());
                        Self::push_success_notification(message, cx);
                    }
                    Err(err) => {
                        error!(
                            "git action failed: epoch={} action=Commit and Push exec_elapsed_ms={} total_elapsed_ms={} err={err:#}",
                            epoch,
                            execution_elapsed.as_millis(),
                            total_elapsed.as_millis()
                        );
                        let summary = err.to_string();
                        this.git_status_message = Some(format!("Git error: {err:#}"));
                        Self::push_error_notification(
                            format!("Commit and Push failed: {summary}"),
                            cx,
                        );
                    }
                }

                cx.notify();
            },
        );
    }

    pub(super) fn create_or_switch_branch_from_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let raw_name = self.branch_input_state.read(cx).value().to_string();
        if raw_name.trim().is_empty() {
            self.set_git_warning_message("Branch name is required.".to_string(), Some(window), cx);
            return;
        }

        let sanitized = sanitize_branch_name(&raw_name);
        let started =
            self.request_activate_or_create_branch_with_dirty_guard(sanitized, Some(window), cx);
        if started {
            self.branch_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }
    }

    pub(super) fn publish_current_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_run_active_branch_actions() {
            self.set_git_warning_message(
                "Activate a branch before publishing.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if !self.tracking_area_clean() {
            self.set_git_warning_message(
                "Commit or discard working tree changes before publishing.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if self.git_workspace.branch_has_upstream {
            self.set_git_warning_message(
                "Branch is already published.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if self.git_controls_busy() {
            return;
        }

        let branch_name = self.git_workspace.branch_name.clone();
        self.run_git_action("Publish branch", cx, move |repo_root| {
            push_current_branch(&repo_root, &branch_name, false)?;
            Ok(format!("Published branch {}", branch_name))
        });
    }

    pub(super) fn push_current_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_run_active_branch_actions() {
            self.set_git_warning_message(
                "Activate a branch before pushing.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if !self.git_workspace.branch_has_upstream {
            self.set_git_warning_message(
                "Publish this branch before pushing.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if self.git_workspace.branch_ahead_count == 0 {
            self.set_git_warning_message("No commits to push.".to_string(), Some(window), cx);
            return;
        }
        if self.git_controls_busy() {
            return;
        }

        let branch_name = self.git_workspace.branch_name.clone();
        self.run_git_action("Push branch", cx, move |repo_root| {
            push_current_branch(&repo_root, &branch_name, true)?;
            Ok(format!("Pushed branch {}", branch_name))
        });
    }

    pub(super) fn sync_current_branch_from_remote(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_run_active_branch_actions() {
            self.set_git_warning_message(
                "Activate a branch before syncing.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if !self.git_workspace.branch_has_upstream {
            self.set_git_warning_message(
                "No upstream branch to sync from.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if !self.tracking_area_clean() {
            self.set_git_warning_message(
                "Commit or discard working tree changes before syncing.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if self.git_controls_busy() {
            return;
        }

        let branch_name = self.git_workspace.branch_name.clone();

        self.run_git_action("Sync branch", cx, move |repo_root| {
            sync_current_branch(&repo_root, &branch_name)?;
            Ok(format!("Synced branch {}", branch_name))
        });
    }

    pub(super) fn pull_current_branch_with_rebase(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_run_active_branch_actions() {
            self.set_git_warning_message(
                "Activate a branch before pulling with rebase.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if !self.git_workspace.branch_has_upstream {
            self.set_git_warning_message(
                "No upstream branch to pull from.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if !self.tracking_area_clean() {
            self.set_git_warning_message(
                "Commit or discard working tree changes before pulling with rebase.".to_string(),
                Some(window),
                cx,
            );
            return;
        }
        if self.git_controls_busy() {
            return;
        }

        let branch_name = self.git_workspace.branch_name.clone();

        self.run_git_action("Pull branch --rebase", cx, move |repo_root| {
            pull_branch_with_rebase(&repo_root, &branch_name)?;
            Ok(format!("Rebased branch {} onto upstream", branch_name))
        });
    }

    pub(super) fn fetch_remote_branches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_git_workspace_root().is_none() {
            self.set_git_warning_message("No Git repository available.".to_string(), Some(window), cx);
            return;
        }
        if self.git_controls_busy() {
            return;
        }

        self.run_git_action("Fetch remote branches", cx, move |repo_root| {
            let fetched_remote_count = fetch_remote_branches(&repo_root)?;
            Ok(match fetched_remote_count {
                1 => "Fetched remote branches from 1 remote".to_string(),
                count => format!("Fetched remote branches from {count} remotes"),
            })
        });
    }

    pub(super) fn open_current_branch_review_url(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(reason) = self.active_review_action_blocker() {
            self.set_git_warning_message(
                format!("Open PR/MR unavailable: {reason}"),
                Some(window),
                cx,
            );
            return;
        }
        let Some(repo_root) = self.selected_git_workspace_root() else {
            self.set_git_warning_message("No Git repository available.".to_string(), Some(window), cx);
            return;
        };
        let branch_name = self.git_workspace.branch_name.clone();
        let review_title = self.preferred_review_title_for_branch(branch_name.as_str());
        match self.open_github_review_dialog_for_branch(
            GitHubReviewOpenDialogRequest {
                repo_root,
                branch_name: branch_name.clone(),
                title: review_title,
                body: None,
                action_label: "Open PR/MR".to_string(),
            },
            window,
            cx,
        ) {
            Ok(()) => {}
            Err(err) if err.contains("GitHub only") => {
                self.run_review_url_action_for_branch(branch_name, ReviewUrlAction::Open, cx);
            }
            Err(err) => {
                self.set_git_warning_message(err, Some(window), cx);
            }
        }
    }

    pub(super) fn copy_current_branch_review_url(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(reason) = self.active_review_action_blocker() {
            self.set_git_warning_message(
                format!("Copy review URL unavailable: {reason}"),
                Some(window),
                cx,
            );
            return;
        }
        if let Some(repo_root) = self.selected_git_workspace_root()
            && let Some(review) =
                self.cached_review_summary_for_branch(repo_root.as_path(), self.git_workspace.branch_name.as_str())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(review.url.clone()));
            let message = format!("Copied PR URL for {}", self.git_workspace.branch_name);
            self.git_status_message = Some(message.clone());
            Self::push_success_notification(message, cx);
            cx.notify();
            return;
        }
        self.run_review_url_action_for_branch(
            self.git_workspace.branch_name.clone(),
            ReviewUrlAction::Copy,
            cx,
        );
    }

    fn run_review_url_action_for_branch(
        &mut self,
        branch_name: String,
        action: ReviewUrlAction,
        cx: &mut Context<Self>,
    ) {
        if self.git_controls_busy() {
            return;
        }

        let Some(repo_root) = self.selected_git_workspace_root() else {
            self.git_status_message = Some("No Git repository available.".to_string());
            cx.notify();
            return;
        };
        let review_title = self.preferred_review_title_for_branch(branch_name.as_str());
        let provider_mappings = self.config.review_provider_mappings.clone();
        let branch_for_task = branch_name.clone();
        let review_title_for_task = review_title.clone();

        let epoch = self.begin_git_action(match action {
            ReviewUrlAction::Open => "Open PR/MR",
            ReviewUrlAction::Copy => "Copy PR/MR URL",
        }, cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = review_url_for_branch_with_provider_map(
                        &repo_root,
                        &branch_for_task,
                        &provider_mappings,
                    );
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(Some(url)) => {
                            debug!(
                                "git action complete: epoch={} action={} lookup_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                match action {
                                    ReviewUrlAction::Open => "Open PR/MR",
                                    ReviewUrlAction::Copy => "Copy PR/MR URL",
                                },
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let url = with_review_title_prefill(url, review_title_for_task.as_str());
                            match action {
                                ReviewUrlAction::Copy => {
                                    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                                    let message = format!("Copied review URL for {}", branch_name);
                                    this.git_status_message = Some(message.clone());
                                    Self::push_success_notification(message, cx);
                                }
                                ReviewUrlAction::Open => match open_url_in_browser(url.as_str()) {
                                    Ok(()) => {
                                        this.git_status_message = Some(format!(
                                            "Opened PR/MR in browser for {}",
                                            branch_name
                                        ));
                                    }
                                    Err(err) => {
                                        error!("Open review URL failed: {err:#}");
                                        let summary = err.to_string();
                                        this.git_status_message = Some(format!("Open URL failed: {summary}"));
                                        Self::push_error_notification(
                                            format!("Open review URL failed: {summary}"),
                                            cx,
                                        );
                                    }
                                },
                            }
                        }
                        Ok(None) => {
                            let message = format!(
                                "No review URL found for {}. Add review_provider_mappings in ~/.hunkdiff/config.toml for self-hosted remotes.",
                                branch_name
                            );
                            debug!(
                                "git action complete: epoch={} action={} lookup_elapsed_ms={} total_elapsed_ms={} result=missing_url",
                                epoch,
                                match action {
                                    ReviewUrlAction::Open => "Open PR/MR",
                                    ReviewUrlAction::Copy => "Copy PR/MR URL",
                                },
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.git_status_message = Some(message.clone());
                            Self::push_warning_notification(message, None, cx);
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action={} lookup_elapsed_ms={} total_elapsed_ms={} err={err:#}",
                                epoch,
                                match action {
                                    ReviewUrlAction::Open => "Open PR/MR",
                                    ReviewUrlAction::Copy => "Copy PR/MR URL",
                                },
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("Build review URL failed: {summary}"),
                                cx,
                            );
                        }
                    }

                    cx.notify();
                });
            }
        });
    }

    fn preferred_review_title_for_branch(&self, branch_name: &str) -> String {
        if self.git_workspace.branch_name == branch_name
            && let Some(subject) = self
                .last_commit_subject
                .as_deref()
                .and_then(normalized_review_title_subject)
        {
            return subject;
        }

        branch_name.to_string()
    }

    pub(super) fn commit_from_input(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.git_controls_busy() {
            return;
        }

        let message = self.commit_input_state.read(cx).value().to_string();
        if message.trim().is_empty() {
            self.git_status_message = Some("Commit message cannot be empty.".to_string());
            cx.notify();
            return;
        }

        let Some(repo_root) = self.selected_git_workspace_root() else {
            self.git_status_message = Some("No Git repository available.".to_string());
            cx.notify();
            return;
        };
        if self.staged_commit_file_count() == 0 {
            self.git_status_message =
                Some("Stage at least one file before creating a commit.".to_string());
            cx.notify();
            return;
        }

        let epoch = self.begin_git_action("Create commit", cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = commit_index_with_details(&repo_root, &message);
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(created_commit) => {
                            debug!(
                                "git action complete: epoch={} action=Create commit exec_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.git_status_message = Some("Created commit".to_string());
                            this.apply_optimistic_commit_success(created_commit.subject.as_str());
                            this.apply_optimistic_recent_commit(&created_commit);

                            let commit_input_state = this.commit_input_state.clone();
                            if let Some(window_handle) = cx.windows().into_iter().next()
                                && let Err(err) = cx.update_window(window_handle, |_, window, cx| {
                                    commit_input_state.update(cx, |state, cx| {
                                        state.set_value("", window, cx);
                                    });
                                })
                            {
                                error!("failed to clear commit input after commit: {err:#}");
                            }

                            this.refresh_after_git_action("Create commit", cx);
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action=Create commit exec_elapsed_ms={} total_elapsed_ms={} err={err:#}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("Commit failed: {}", err),
                                cx,
                            );
                        }
                    }

                    cx.notify();
                });
            }
        });
    }

    pub(super) fn generate_commit_message_from_staged(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git_controls_busy() {
            return;
        }

        let Some(repo_root) = self.selected_git_workspace_root() else {
            self.git_status_message = Some("No Git repository available.".to_string());
            cx.notify();
            return;
        };
        if self.staged_commit_file_count() == 0 {
            self.git_status_message =
                Some("Stage at least one file before generating a commit message.".to_string());
            cx.notify();
            return;
        }

        let codex_executable = Self::resolve_codex_executable_path();
        let branch_name = self.git_workspace.branch_name.clone();
        let commit_input_state = self.commit_input_state.clone();
        let window_handle = window.window_handle();
        let epoch = self.begin_git_action("Generate commit message", cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = try_ai_commit_message_for_staged_index(
                        AiCodexGenerationConfig {
                            codex_executable: codex_executable.as_path(),
                            repo_root: repo_root.as_path(),
                        },
                        repo_root.as_path(),
                        branch_name.as_str(),
                    );
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(commit_message) => {
                            debug!(
                                "git action complete: epoch={} action=Generate commit message exec_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let commit_message_text = commit_message.as_git_message();
                            this.git_status_message = Some("Generated commit message".to_string());
                            if let Err(err) = cx.update_window(window_handle, |_, window, cx| {
                                commit_input_state.update(cx, |state, cx| {
                                    state.set_value(commit_message_text.clone(), window, cx);
                                });
                            }) {
                                error!("failed to populate generated commit message: {err:#}");
                                this.git_status_message =
                                    Some(format!("Set commit message failed: {err:#}"));
                                Self::push_error_notification(
                                    "Generate commit message failed: could not update the commit input.".to_string(),
                                    cx,
                                );
                            }
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action=Generate commit message exec_elapsed_ms={} total_elapsed_ms={} err={err:#}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("Generate commit message failed: {summary}"),
                                cx,
                            );
                        }
                    }

                    cx.notify();
                });
            }
        });
    }

    pub(super) fn undo_working_copy_file(
        &mut self,
        file_path: String,
        is_tracked: bool,
        cx: &mut Context<Self>,
    ) {
        let file_path = file_path.trim().to_string();
        if file_path.is_empty() {
            self.git_status_message = Some("File path is required.".to_string());
            cx.notify();
            return;
        }

        if self.git_controls_busy() {
            return;
        }

        let Some(repo_root) = self.selected_git_workspace_root() else {
            self.git_status_message = Some("No Git repository available.".to_string());
            cx.notify();
            return;
        };

        if self.prevent_file_editor_tab_discard_for_path(file_path.as_str(), "restoring", cx) {
            return;
        }

        self.close_file_editor_tabs_for_path(file_path.as_str());

        let epoch = self.begin_git_action("Undo file changes", cx);
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let file_path_for_action = file_path.clone();
            let (execution_elapsed, result) = cx
                .background_executor()
                .spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = restore_working_copy_paths(
                        &repo_root,
                        std::slice::from_ref(&file_path_for_action),
                    )
                    .map(|_| {
                        if is_tracked {
                            format!("Restored {}", file_path_for_action)
                        } else {
                            format!("Removed untracked {}", file_path_for_action)
                        }
                    });
                    (execution_started_at.elapsed(), result)
                })
                .await;

            if let Some(this) = this.upgrade() {
                let restored_file_path = file_path.clone();
                this.update(cx, move |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(message) => {
                            debug!(
                                "git action complete: epoch={} action=Undo file changes exec_elapsed_ms={} total_elapsed_ms={}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.git_status_message = Some(message);
                            this.apply_optimistic_restore_success(restored_file_path.as_str());
                            this.refresh_after_git_action("Undo file changes", cx);
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action=Undo file changes exec_elapsed_ms={} total_elapsed_ms={} err={err:#}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis()
                            );
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(
                                format!("Undo file changes failed: {}", err),
                                cx,
                            );
                        }
                    }

                    cx.notify();
                });
            }
        });
    }

}
