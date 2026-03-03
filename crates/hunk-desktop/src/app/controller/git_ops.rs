impl DiffViewer {
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
                    gpui_component::notification::Notification::error(message.clone()),
                    cx,
                );
            }) {
                error!("failed to show git action error notification: {err:#}");
            }
        }
    }

    fn push_warning_notification(message: String, cx: &mut Context<Self>) {
        let window_handles = cx.windows().into_iter().collect::<Vec<_>>();
        if window_handles.is_empty() {
            error!("cannot show git action warning notification: no windows available");
            return;
        }

        for window_handle in window_handles {
            if let Err(err) = cx.update_window(window_handle, |_, window, cx| {
                gpui_component::WindowExt::push_notification(
                    window,
                    gpui_component::notification::Notification::warning(message.clone()),
                    cx,
                );
            }) {
                error!("failed to show git action warning notification: {err:#}");
            }
        }
    }

    fn now_unix_seconds() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    }

    fn push_recovery_candidate_for_switch(
        &mut self,
        target_bookmark: &str,
    ) -> Option<WorkingCopyRecoveryCandidate> {
        if self.files.is_empty() {
            return None;
        }
        let source_revision_id = self.graph_working_copy_commit_id.clone()?;
        let source_bookmark = self
            .checked_out_bookmark_name()
            .unwrap_or(self.branch_name.as_str())
            .to_string();
        let switched_to_bookmark = target_bookmark.trim().to_string();
        if source_bookmark == switched_to_bookmark {
            return None;
        }

        let candidate = WorkingCopyRecoveryCandidate {
            source_revision_id,
            source_bookmark,
            switched_to_bookmark,
            changed_file_count: self.files.len(),
            unix_time: Self::now_unix_seconds(),
        };
        self.working_copy_recovery_candidates
            .retain(|existing| existing.source_revision_id != candidate.source_revision_id);
        self.working_copy_recovery_candidates
            .insert(0, candidate.clone());
        self.working_copy_recovery_candidates.truncate(8);
        Some(candidate)
    }

    pub(super) fn latest_working_copy_recovery_candidate_for_active_bookmark(
        &self,
    ) -> Option<WorkingCopyRecoveryCandidate> {
        let active_bookmark = self
            .checked_out_bookmark_name()
            .unwrap_or(self.branch_name.as_str());
        self.working_copy_recovery_candidates
            .iter()
            .find(|candidate| {
                candidate.source_bookmark == active_bookmark
                    || candidate.switched_to_bookmark == active_bookmark
            })
            .cloned()
    }

    fn next_git_action_epoch(&mut self) -> usize {
        self.git_action_epoch = self.git_action_epoch.saturating_add(1);
        self.git_action_epoch
    }

    fn begin_git_action(&mut self, action_label: impl Into<String>, cx: &mut Context<Self>) -> usize {
        let epoch = self.next_git_action_epoch();
        self.git_action_loading = true;
        self.git_action_label = Some(action_label.into());
        cx.notify();
        epoch
    }

    fn finish_git_action(&mut self) {
        self.git_action_loading = false;
        self.git_action_label = None;
    }

    fn run_git_action<F>(&mut self, action_name: &'static str, cx: &mut Context<Self>, action: F)
    where
        F: FnOnce(std::path::PathBuf) -> anyhow::Result<String> + Send + 'static,
    {
        if self.git_action_loading {
            return;
        }

        let Some(repo_root) = self.repo_root.clone() else {
            self.git_status_message = Some("No JJ repository available.".to_string());
            cx.notify();
            return;
        };

        let epoch = self.begin_git_action(action_name, cx);

        self.git_action_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { action(repo_root) }).await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    this.finish_git_action();
                    match result {
                        Ok(message) => {
                            this.git_status_message = if message.is_empty() {
                                None
                            } else {
                                Some(message)
                            };
                            this.request_snapshot_refresh_internal(true, cx);
                        }
                        Err(err) => {
                            error!("{action_name} failed: {err:#}");
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("JJ error: {err:#}"));
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
    }

    fn checkout_or_create_bookmark_with_options(
        &mut self,
        branch_name: String,
        move_changes_to_new_bookmark: bool,
        recovery_candidate: Option<WorkingCopyRecoveryCandidate>,
        cx: &mut Context<Self>,
    ) {
        self.run_git_action("Activate bookmark", cx, move |repo_root| {
            checkout_or_create_bookmark_with_change_transfer(
                &repo_root,
                &branch_name,
                move_changes_to_new_bookmark,
            )?;
            let message = if move_changes_to_new_bookmark {
                format!(
                    "Activated bookmark {} and moved changes",
                    branch_name
                )
            } else {
                format!("Activated bookmark {}", branch_name)
            };
            if let Some(candidate) = recovery_candidate {
                return Ok(format!(
                    "{} · {} files captured from {} -> {}",
                    message,
                    candidate.changed_file_count,
                    candidate.source_bookmark,
                    candidate.switched_to_bookmark
                ));
            }
            Ok(message)
        });
    }

    fn activate_or_create_bookmark(
        &mut self,
        branch_name: String,
        move_changes_to_new_bookmark: bool,
        cx: &mut Context<Self>,
    ) {
        let target_branch = branch_name.trim().to_string();
        if target_branch.is_empty() {
            self.git_status_message = Some("Bookmark name is required.".to_string());
            cx.notify();
            return;
        }
        if self.checked_out_bookmark_name() == Some(target_branch.as_str()) {
            self.git_status_message = Some(format!("Bookmark {} is already active.", target_branch));
            cx.notify();
            return;
        }
        self.pending_bookmark_switch = None;
        let move_changes =
            move_changes_to_new_bookmark && !self.files.is_empty() && !self.branch_name.is_empty();
        let recovery_candidate = if move_changes {
            None
        } else {
            self.push_recovery_candidate_for_switch(target_branch.as_str())
        };
        self.checkout_or_create_bookmark_with_options(
            target_branch,
            move_changes,
            recovery_candidate,
            cx,
        );
    }

    pub(super) fn checkout_bookmark(
        &mut self,
        branch_name: String,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_activate_or_create_bookmark_with_dirty_guard(branch_name, cx);
    }

    pub(super) fn checkout_bookmark_with_change_transfer(
        &mut self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) {
        self.pending_bookmark_switch = None;
        self.activate_or_create_bookmark(branch_name, true, cx);
    }

    pub(super) fn toggle_commit_file_included(
        &mut self,
        file_path: String,
        include: bool,
        cx: &mut Context<Self>,
    ) {
        if include {
            self.commit_excluded_files.remove(file_path.as_str());
        } else {
            self.commit_excluded_files.insert(file_path);
        }
        cx.notify();
    }

    pub(super) fn include_all_files_for_commit(&mut self, cx: &mut Context<Self>) {
        if self.commit_excluded_files.is_empty() {
            return;
        }
        self.commit_excluded_files.clear();
        cx.notify();
    }

    pub(super) fn included_commit_file_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| !self.commit_excluded_files.contains(file.path.as_str()))
            .count()
    }

    pub(super) fn bookmark_syncable(&self) -> bool {
        !self.branch_name.is_empty()
            && self.branch_name != "unknown"
            && self.branch_name != "detached"
    }

    pub(super) fn checked_out_bookmark_name(&self) -> Option<&str> {
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

    pub(super) fn active_bookmark_is_checked_out(&self) -> bool {
        self.branches
            .iter()
            .any(|branch| branch.is_current && branch.name == self.branch_name)
    }

    pub(super) fn can_run_active_bookmark_actions(&self) -> bool {
        self.bookmark_syncable() && self.active_bookmark_is_checked_out()
    }

    fn tracking_area_clean(&self) -> bool {
        self.files.is_empty()
    }

    pub(super) fn can_sync_current_bookmark(&self) -> bool {
        self.can_run_active_bookmark_actions()
            && self.branch_has_upstream
            && self.tracking_area_clean()
            && !self.git_action_loading
    }

    pub(super) fn can_publish_current_bookmark(&self) -> bool {
        self.can_run_active_bookmark_actions()
            && !self.branch_has_upstream
            && self.tracking_area_clean()
            && !self.git_action_loading
    }

    pub(super) fn can_push_current_bookmark_revisions(&self) -> bool {
        self.can_run_active_bookmark_actions()
            && self.branch_has_upstream
            && self.branch_ahead_count > 0
            && self.tracking_area_clean()
            && !self.git_action_loading
    }

    fn selected_commit_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .filter(|file| !self.commit_excluded_files.contains(file.path.as_str()))
            .map(|file| file.path.clone())
            .collect()
    }

    pub(super) fn create_or_switch_bookmark_from_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let raw_name = self.branch_input_state.read(cx).value().to_string();
        if raw_name.trim().is_empty() {
            self.git_status_message = Some("Bookmark name is required.".to_string());
            cx.notify();
            return;
        }

        let sanitized = sanitize_bookmark_name(&raw_name);
        self.branch_input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.request_activate_or_create_bookmark_with_dirty_guard(sanitized, cx);
    }

    pub(super) fn rename_current_bookmark_from_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_run_active_bookmark_actions() {
            self.git_status_message = Some("Activate a bookmark before renaming it.".to_string());
            cx.notify();
            return;
        }

        let raw_name = self.branch_input_state.read(cx).value().to_string();
        if raw_name.trim().is_empty() {
            self.git_status_message = Some("New bookmark name is required.".to_string());
            cx.notify();
            return;
        }

        let current_branch = self.branch_name.clone();
        let sanitized = sanitize_bookmark_name(&raw_name);
        self.branch_input_state.update(cx, |state, cx| {
            state.set_value(sanitized.clone(), window, cx);
        });
        if sanitized == current_branch {
            self.git_status_message =
                Some("New bookmark name must differ from the current bookmark.".to_string());
            cx.notify();
            return;
        }

        self.run_git_action("Rename bookmark", cx, move |repo_root| {
            rename_bookmark(&repo_root, &current_branch, &sanitized)?;
            Ok(format!(
                "Renamed bookmark {} to {}",
                current_branch, sanitized
            ))
        });
    }

    pub(super) fn describe_current_bookmark_from_input(&mut self, cx: &mut Context<Self>) {
        if !self.can_run_active_bookmark_actions() {
            self.git_status_message =
                Some("Cannot edit revision description without an active bookmark.".to_string());
            cx.notify();
            return;
        }

        let message = self.commit_input_state.read(cx).value().to_string();
        if message.trim().is_empty() {
            self.git_status_message = Some("Revision description cannot be empty.".to_string());
            cx.notify();
            return;
        }

        let branch_name = self.branch_name.clone();
        self.run_git_action("Edit revision description", cx, move |repo_root| {
            describe_bookmark_head(&repo_root, &branch_name, &message)?;
            Ok(format!("Updated tip revision on {}", branch_name))
        });
    }

    pub(super) fn abandon_current_bookmark_tip(&mut self, cx: &mut Context<Self>) {
        if !self.can_run_active_bookmark_actions() {
            self.git_status_message =
                Some("Cannot abandon a revision without an active bookmark.".to_string());
            cx.notify();
            return;
        }
        if self.bookmark_revisions.is_empty() {
            self.git_status_message = Some("No revision available to abandon.".to_string());
            cx.notify();
            return;
        }

        let branch_name = self.branch_name.clone();
        self.run_git_action("Abandon tip revision", cx, move |repo_root| {
            abandon_bookmark_head(&repo_root, &branch_name)?;
            Ok(format!("Abandoned tip revision on {}", branch_name))
        });
    }

    pub(super) fn squash_current_bookmark_tip_into_parent(&mut self, cx: &mut Context<Self>) {
        if !self.can_run_active_bookmark_actions() {
            self.git_status_message =
                Some("Cannot squash a revision without an active bookmark.".to_string());
            cx.notify();
            return;
        }
        if self.bookmark_revisions.len() < 2 {
            self.git_status_message = Some(
                "Need at least two revisions in the stack to squash the tip.".to_string(),
            );
            cx.notify();
            return;
        }

        let branch_name = self.branch_name.clone();
        self.run_git_action("Squash tip revision", cx, move |repo_root| {
            squash_bookmark_head_into_parent(&repo_root, &branch_name)?;
            Ok(format!("Squashed tip revision on {}", branch_name))
        });
    }

    pub(super) fn reorder_current_bookmark_tip_older(&mut self, cx: &mut Context<Self>) {
        if !self.can_run_active_bookmark_actions() {
            self.git_status_message =
                Some("Cannot reorder revisions without an active bookmark.".to_string());
            cx.notify();
            return;
        }
        if self.bookmark_revisions.len() < 2 {
            self.git_status_message =
                Some("Need at least two revisions in the stack to reorder.".to_string());
            cx.notify();
            return;
        }

        let branch_name = self.branch_name.clone();
        self.run_git_action("Reorder tip revision", cx, move |repo_root| {
            reorder_bookmark_tip_older(&repo_root, &branch_name)?;
            Ok(format!(
                "Reordered top two revisions on {}",
                branch_name
            ))
        });
    }

    pub(super) fn publish_current_bookmark(&mut self, cx: &mut Context<Self>) {
        if !self.can_run_active_bookmark_actions() {
            let message = "Activate a bookmark before publishing.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if !self.tracking_area_clean() {
            let message = "Commit or discard working-copy changes before publishing.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if self.branch_has_upstream {
            let message = "Bookmark is already published.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if self.git_action_loading {
            return;
        }

        let branch_name = self.branch_name.clone();
        self.run_git_action("Publish bookmark", cx, move |repo_root| {
            push_current_bookmark(&repo_root, &branch_name, false)?;
            Ok(format!("Published bookmark {}", branch_name))
        });
    }

    pub(super) fn push_current_bookmark_revisions(&mut self, cx: &mut Context<Self>) {
        if !self.can_run_active_bookmark_actions() {
            let message = "Activate a bookmark before pushing revisions.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if !self.branch_has_upstream {
            let message = "Publish this bookmark before pushing revisions.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if !self.tracking_area_clean() {
            let message = "Commit or discard working-copy changes before pushing revisions."
                .to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if self.branch_ahead_count == 0 {
            let message = "No revisions to push.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if self.git_action_loading {
            return;
        }

        let branch_name = self.branch_name.clone();
        self.run_git_action("Push revisions", cx, move |repo_root| {
            push_current_bookmark(&repo_root, &branch_name, true)?;
            Ok(format!("Pushed revisions for {}", branch_name))
        });
    }

    pub(super) fn sync_current_bookmark_from_remote(&mut self, cx: &mut Context<Self>) {
        if !self.can_run_active_bookmark_actions() {
            let message = "Activate a bookmark before syncing.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if !self.branch_has_upstream {
            let message = "No upstream bookmark to sync from.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if !self.tracking_area_clean() {
            let message = "Commit or discard working-copy changes before syncing.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        if self.git_action_loading {
            return;
        }

        let branch_name = self.branch_name.clone();

        self.run_git_action("Sync bookmark", cx, move |repo_root| {
            sync_current_bookmark(&repo_root, &branch_name)?;
            Ok(format!("Synced bookmark {}", branch_name))
        });
    }

    pub(super) fn open_current_bookmark_review_url(&mut self, cx: &mut Context<Self>) {
        if let Some(reason) = self.active_review_action_blocker() {
            let message = format!("Open PR/MR unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        self.run_review_url_action_for_bookmark(
            self.branch_name.clone(),
            ReviewUrlAction::Open,
            cx,
        );
    }

    pub(super) fn copy_current_bookmark_review_url(&mut self, cx: &mut Context<Self>) {
        if let Some(reason) = self.active_review_action_blocker() {
            let message = format!("Copy review URL unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        self.run_review_url_action_for_bookmark(
            self.branch_name.clone(),
            ReviewUrlAction::Copy,
            cx,
        );
    }

    pub(super) fn open_selected_graph_bookmark_review_url(&mut self, cx: &mut Context<Self>) {
        if let Some(reason) = self.selected_graph_review_action_blocker() {
            let message = format!("Open PR/MR unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        };
        let Some(bookmark_name) = self.selected_local_graph_bookmark_name() else {
            return;
        };
        self.run_review_url_action_for_bookmark(bookmark_name, ReviewUrlAction::Open, cx);
    }

    pub(super) fn copy_selected_graph_bookmark_review_url(&mut self, cx: &mut Context<Self>) {
        if let Some(reason) = self.selected_graph_review_action_blocker() {
            let message = format!("Copy review URL unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        };
        let Some(bookmark_name) = self.selected_local_graph_bookmark_name() else {
            return;
        };
        self.run_review_url_action_for_bookmark(bookmark_name, ReviewUrlAction::Copy, cx);
    }

    fn selected_local_graph_bookmark_name(&self) -> Option<String> {
        self.graph_selected_bookmark
            .as_ref()
            .filter(|bookmark| bookmark.scope == GraphBookmarkScope::Local)
            .map(|bookmark| bookmark.name.clone())
    }

    fn run_review_url_action_for_bookmark(
        &mut self,
        bookmark_name: String,
        action: ReviewUrlAction,
        cx: &mut Context<Self>,
    ) {
        if self.git_action_loading {
            return;
        }

        let Some(repo_root) = self.repo_root.clone() else {
            self.git_status_message = Some("No JJ repository available.".to_string());
            cx.notify();
            return;
        };
        let review_title = self.preferred_review_title_for_bookmark(bookmark_name.as_str());
        let provider_mappings = self.config.review_provider_mappings.clone();
        let bookmark_for_task = bookmark_name.clone();
        let review_title_for_task = review_title.clone();

        let epoch = self.begin_git_action(match action {
            ReviewUrlAction::Open => "Open PR/MR",
            ReviewUrlAction::Copy => "Copy PR/MR URL",
        }, cx);

        self.git_action_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                review_url_for_bookmark_with_provider_map(
                    &repo_root,
                    &bookmark_for_task,
                    &provider_mappings,
                )
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    this.finish_git_action();
                    match result {
                        Ok(Some(url)) => {
                            let url = with_review_title_prefill(url, review_title_for_task.as_str());
                            match action {
                                ReviewUrlAction::Copy => {
                                    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                                    this.git_status_message =
                                        Some(format!("Copied review URL for {}", bookmark_name));
                                }
                                ReviewUrlAction::Open => match open_url_in_browser(url.as_str()) {
                                    Ok(()) => {
                                        this.git_status_message = Some(format!(
                                            "Opened PR/MR in browser for {}",
                                            bookmark_name
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
                                bookmark_name
                            );
                            this.git_status_message = Some(message.clone());
                            Self::push_warning_notification(message, cx);
                        }
                        Err(err) => {
                            error!("Build review URL failed: {err:#}");
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("JJ error: {err:#}"));
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

    fn preferred_review_title_for_bookmark(&self, bookmark_name: &str) -> String {
        if self.branch_name == bookmark_name
            && let Some(subject) = self
                .bookmark_revisions
                .first()
                .map(|revision| revision.subject.as_str())
                .and_then(normalized_review_title_subject)
        {
            return subject;
        }

        if let Some(subject) = self
            .graph_nodes
            .iter()
            .find(|node| {
                node.bookmarks.iter().any(|bookmark| {
                    bookmark.scope == GraphBookmarkScope::Local && bookmark.name == bookmark_name
                })
            })
            .map(|node| node.subject.as_str())
            .and_then(normalized_review_title_subject)
        {
            return subject;
        }

        bookmark_name.to_string()
    }

    pub(super) fn commit_from_input(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.git_action_loading {
            return;
        }

        let message = self.commit_input_state.read(cx).value().to_string();
        if message.trim().is_empty() {
            self.git_status_message = Some("Commit message cannot be empty.".to_string());
            cx.notify();
            return;
        }

        let Some(repo_root) = self.repo_root.clone() else {
            self.git_status_message = Some("No JJ repository available.".to_string());
            cx.notify();
            return;
        };
        let selected_paths = self.selected_commit_paths();
        if selected_paths.is_empty() {
            self.git_status_message =
                Some("Select at least one file to include in commit.".to_string());
            cx.notify();
            return;
        }
        let partial_commit = selected_paths.len() != self.files.len();

        let epoch = self.begin_git_action("Create revision", cx);

        self.git_action_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                if partial_commit {
                    commit_selected_paths(&repo_root, &message, &selected_paths)?;
                } else {
                    commit_staged(&repo_root, &message)?;
                }
                Ok::<String, anyhow::Error>(message.trim_end().to_string())
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    this.finish_git_action();
                    match result {
                        Ok(subject) => {
                            this.commit_excluded_files.clear();
                            this.git_status_message = Some("Created commit".to_string());
                            this.last_commit_subject = Some(subject);

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

                            this.request_snapshot_refresh(cx);
                        }
                        Err(err) => {
                            error!("Commit failed: {err:#}");
                            this.git_status_message = Some(format!("JJ error: {err:#}"));
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

    pub(super) fn toggle_bookmark_picker(&mut self, cx: &mut Context<Self>) {
        self.branch_picker_open = !self.branch_picker_open;
        cx.notify();
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

        if self.editor_path.as_deref() == Some(file_path.as_str())
            && self.prevent_unsaved_editor_discard(Some(file_path.as_str()), cx)
        {
            return;
        }

        self.run_git_action("Undo file changes", cx, move |repo_root| {
            restore_working_copy_paths(&repo_root, std::slice::from_ref(&file_path))?;
            let message = if is_tracked {
                format!("Restored {}", file_path)
            } else {
                format!("Removed untracked {}", file_path)
            };
            Ok(message)
        });
    }

    pub(super) fn undo_all_working_copy_changes(&mut self, cx: &mut Context<Self>) {
        if self.files.is_empty() {
            self.git_status_message = Some("No working-copy changes to undo.".to_string());
            cx.notify();
            return;
        }

        if self.prevent_unsaved_editor_discard(None, cx) {
            return;
        }

        self.run_git_action("Undo all working-copy changes", cx, move |repo_root| {
            restore_all_working_copy_changes(&repo_root)?;
            Ok("Restored all working-copy changes".to_string())
        });
    }

    pub(super) fn recover_latest_working_copy_for_active_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(candidate) = self.latest_working_copy_recovery_candidate_for_active_bookmark() else {
            let message = "No recoverable working-copy changes were captured for this bookmark."
                .to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        };
        let source_revision_id = candidate.source_revision_id.clone();
        let changed_file_count = candidate.changed_file_count;

        self.run_git_action("Recover working copy", cx, move |repo_root| {
            restore_working_copy_from_revision(&repo_root, &source_revision_id)?;
            let short_revision = source_revision_id.chars().take(12).collect::<String>();
            Ok(format!(
                "Recovered {} files from working-copy revision {}",
                changed_file_count, short_revision
            ))
        });
    }
}

#[derive(Clone, Copy)]
enum ReviewUrlAction {
    Open,
    Copy,
}

fn open_url_in_browser(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .context("failed to launch macOS browser opener")?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("failed to launch Linux browser opener")?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .context("failed to launch Windows browser opener")?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(anyhow::anyhow!(
        "opening review URLs is not supported on this platform"
    ))
}

fn with_review_title_prefill(url: String, title: &str) -> String {
    let normalized_title = normalized_review_title_subject(title);
    let Some(title) = normalized_title else {
        return url;
    };

    if url.contains("/-/merge_requests/new") {
        return append_query_param(url, "merge_request[title]", title.as_str());
    }

    if url.contains("/compare/") {
        let with_quick_pull = append_query_param(url, "quick_pull", "1");
        return append_query_param(with_quick_pull, "title", title.as_str());
    }

    url
}

fn append_query_param(url: String, key: &str, value: &str) -> String {
    let mut out = url;
    let separator = if out.contains('?') {
        if out.ends_with('?') || out.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    out.push_str(separator);
    out.push_str(percent_encode_url_component(key).as_str());
    out.push('=');
    out.push_str(percent_encode_url_component(value).as_str());
    out
}

fn normalized_review_title_subject(raw: &str) -> Option<String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with('(') && normalized.contains("no description") {
        return None;
    }
    Some(normalized.to_string())
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(format!("%{byte:02X}").as_str());
        }
    }
    encoded
}
