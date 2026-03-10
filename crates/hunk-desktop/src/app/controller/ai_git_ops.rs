#[derive(Clone)]
struct AiThreadGitActionContext {
    repo_root: std::path::PathBuf,
    thread_id: String,
    branch_name: String,
    start_mode: AiNewThreadStartMode,
}

#[derive(Clone)]
struct AiManagedWorktreeDeleteContext {
    worktree_root: std::path::PathBuf,
    workspace_key: String,
    thread_id: String,
    worktree_name: String,
    branch_name: String,
}

#[derive(Debug, Clone)]
struct AiGitProgressEvent {
    step: AiGitProgressStep,
    detail: Option<String>,
}

impl DiffViewer {
    fn ai_current_thread_git_action_context(
        &self,
        action_description: &str,
    ) -> Result<AiThreadGitActionContext, String> {
        if self.git_controls_busy() {
            return Err("Another workspace action is in progress.".to_string());
        }

        let Some(thread_id) = self.current_ai_thread_id() else {
            return Err(format!("Select a thread before {action_description}."));
        };
        let Some(repo_root) = self.ai_workspace_cwd() else {
            return Err(format!("Open a workspace before {action_description}."));
        };
        let Some(start_mode) = self.ai_thread_start_mode(thread_id.as_str()) else {
            return Err(format!(
                "Unable to resolve the selected thread before {action_description}."
            ));
        };

        let branch_name = self
            .workspace_targets
            .iter()
            .find(|target| target.root == repo_root)
            .map(|target| target.branch_name.clone())
            .unwrap_or_else(|| {
                self.primary_checked_out_branch_name()
                    .unwrap_or(self.branch_name.as_str())
                    .to_string()
            });
        let normalized_branch = branch_name.trim();
        if normalized_branch.is_empty()
            || matches!(normalized_branch, "detached" | "unknown")
        {
            return Err(format!("Activate a branch before {action_description}."));
        }

        Ok(AiThreadGitActionContext {
            repo_root,
            thread_id,
            branch_name,
            start_mode,
        })
    }

    pub(super) fn ai_publish_blocker(&self) -> Option<String> {
        ai_publish_blocker_reason(self.ai_current_thread_git_action_context("publishing"))
    }

    pub(super) fn ai_open_pr_blocker(&self) -> Option<String> {
        self.ai_current_thread_git_action_context("opening PR").err()
    }

    pub(super) fn ai_current_managed_worktree_target(&self) -> Option<WorkspaceTargetSummary> {
        let thread_id = self.current_ai_thread_id()?;
        if self.ai_thread_start_mode(thread_id.as_str()) != Some(AiNewThreadStartMode::Worktree) {
            return None;
        }

        let workspace_root = self.ai_thread_workspace_root(thread_id.as_str())?;
        self.workspace_targets
            .iter()
            .find(|target| {
                target.root == workspace_root
                    && target.kind == hunk_git::worktree::WorkspaceTargetKind::LinkedWorktree
                    && target.managed
            })
            .cloned()
    }

    fn ai_current_managed_worktree_delete_context(
        &self,
        action_description: &str,
    ) -> Result<AiManagedWorktreeDeleteContext, String> {
        if self.git_controls_busy() {
            return Err("Another workspace action is in progress.".to_string());
        }

        let Some(thread_id) = self.current_ai_thread_id() else {
            return Err(format!("Select a thread before {action_description}."));
        };
        if self.current_ai_in_progress_turn_id(thread_id.as_str()).is_some() {
            return Err("Wait for the current run to finish or interrupt it first.".to_string());
        }

        let Some(target) = self.ai_current_managed_worktree_target() else {
            return Err(format!(
                "Select a Hunk-managed worktree thread before {action_description}."
            ));
        };

        Ok(AiManagedWorktreeDeleteContext {
            worktree_root: target.root.clone(),
            workspace_key: target.root.to_string_lossy().to_string(),
            thread_id,
            worktree_name: target.name,
            branch_name: target.branch_name,
        })
    }

    pub(super) fn ai_delete_worktree_blocker(&self) -> Option<String> {
        self.ai_current_managed_worktree_delete_context("deleting its worktree")
            .err()
    }

    pub(super) fn ai_confirm_delete_current_worktree_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let context =
            match self.ai_current_managed_worktree_delete_context("deleting its worktree") {
                Ok(context) => context,
                Err(reason) => {
                    let message = format!("Delete worktree unavailable: {reason}");
                    self.git_status_message = Some(message.clone());
                    Self::push_warning_notification(message, Some(window), cx);
                    cx.notify();
                    return;
                }
            };
        let worktree_name = context.worktree_name.clone();
        let branch_name = context.branch_name.clone();
        let worktree_path = context.worktree_root.display().to_string();
        let view = cx.entity();

        gpui_component::WindowExt::open_alert_dialog(window, cx, move |alert, _, cx| {
            alert
                .width(px(460.0))
                .title("Delete Worktree?")
                .description(format!(
                    "Remove worktree '{}' for branch '{}'? This deletes the checkout at {}.",
                    worktree_name, branch_name, worktree_path
                ))
                .button_props(
                    gpui_component::dialog::DialogButtonProps::default()
                        .ok_text("Delete Worktree")
                        .ok_variant(gpui_component::button::ButtonVariant::Danger)
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .whitespace_normal()
                        .child(
                            "The branch is kept, but any uncommitted changes in the worktree must be cleared before deletion.",
                        ),
                )
                .on_ok({
                    let view = view.clone();
                    let context = context.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.ai_delete_current_worktree_action(context.clone(), cx);
                        });
                        true
                    }
                })
        });
    }

    fn ai_delete_current_worktree_action(
        &mut self,
        context: AiManagedWorktreeDeleteContext,
        cx: &mut Context<Self>,
    ) {
        let previous_workspace_key = self.ai_workspace_key();
        let restore_selection_after_failure =
            previous_workspace_key.as_deref() == Some(context.workspace_key.as_str());

        if restore_selection_after_failure {
            self.sync_ai_visible_composer_prompt_to_draft(cx);
            self.ai_selected_thread_id = None;
            self.ai_new_thread_draft_active = false;
            self.ai_pending_new_thread_selection = false;
            self.ai_pending_thread_start = None;
            self.ai_draft_workspace_target_id = self
                .primary_workspace_target_id()
                .or_else(|| self.workspace_targets.first().map(|target| target.id.clone()));
            let next_workspace_key = self.ai_workspace_key();
            self.ai_handle_workspace_change_to(previous_workspace_key, next_workspace_key, cx);
        }

        self.invalidate_ai_thread_catalog_refresh();
        self.shutdown_ai_runtime_for_workspace_blocking(context.workspace_key.as_str());

        let epoch = self.begin_git_action("Delete Worktree", cx);
        self.begin_ai_git_progress(
            epoch,
            AiGitProgressAction::DeleteWorktree,
            ai_delete_worktree_progress_steps(),
            AiGitProgressStep::RemovingWorktree,
            Some(format!(
                "Removing {} at {}",
                context.worktree_name,
                context.worktree_root.display()
            )),
            cx,
        );
        let started_at = Instant::now();
        let workspace_key = context.workspace_key.clone();
        let worktree_root = context.worktree_root.clone();
        let worktree_name = context.worktree_name.clone();
        let thread_id = context.thread_id.clone();
        self.git_status_message = Some(format!("Deleting worktree {}...", worktree_name));
        self.git_action_task = cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                hunk_git::worktree::remove_managed_worktree(worktree_root.as_path())
            });
            let result = result.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok(()) => {
                            debug!(
                                "git action complete: epoch={} action=Delete Worktree total_elapsed_ms={} worktree={} workspace_key={}",
                                epoch,
                                total_elapsed.as_millis(),
                                worktree_name,
                                workspace_key
                            );
                            this.ai_forget_deleted_workspace_state(workspace_key.as_str());
                            this.refresh_workspace_targets_from_git_state(cx);
                            this.refresh_after_git_action("Delete Worktree", cx);
                            let message = format!("Deleted worktree {}", worktree_name);
                            this.git_status_message = Some(message.clone());
                            Self::push_success_notification(message, cx);
                            cx.notify();
                        }
                        Err(err) => {
                            let summary = err.to_string();
                            debug!(
                                "git action failed: epoch={} action=Delete Worktree total_elapsed_ms={} worktree={} workspace_key={} err={err:#}",
                                epoch,
                                total_elapsed.as_millis(),
                                worktree_name,
                                workspace_key
                            );
                            if restore_selection_after_failure {
                                this.ai_restore_workspace_after_failed_delete(
                                    workspace_key.as_str(),
                                    thread_id.as_str(),
                                    cx,
                                );
                            }
                            let message = format!("Delete worktree failed: {summary}");
                            this.git_status_message = Some(message.clone());
                            Self::push_error_notification(message, cx);
                            cx.notify();
                        }
                    }
                });
            }
        });
    }

    fn ai_restore_workspace_after_failed_delete(
        &mut self,
        workspace_key: &str,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) {
        let current_workspace_key = self.ai_workspace_key();
        self.ai_selected_thread_id = Some(thread_id.to_string());
        self.ai_handle_workspace_change_to(
            current_workspace_key,
            Some(workspace_key.to_string()),
            cx,
        );
    }

    fn ai_forget_deleted_workspace_state(&mut self, workspace_key: &str) {
        let removed_workspace_state = self.ai_workspace_states.remove(workspace_key);
        if let Some(removed_workspace_state) = removed_workspace_state {
            for thread_id in removed_workspace_state.state_snapshot.threads.keys() {
                let thread_key = AiComposerDraftKey::Thread(thread_id.clone());
                self.ai_composer_drafts.remove(&thread_key);
                self.ai_composer_status_by_draft.remove(&thread_key);
            }
        }

        let workspace_draft_key = AiComposerDraftKey::Workspace(workspace_key.to_string());
        self.ai_composer_drafts.remove(&workspace_draft_key);
        self.ai_composer_status_by_draft.remove(&workspace_draft_key);

        let mut state_changed = false;
        state_changed |= self.state.ai_workspace_mad_max.remove(workspace_key).is_some();
        state_changed |= self
            .state
            .ai_workspace_include_hidden_models
            .remove(workspace_key)
            .is_some();
        state_changed |= self
            .state
            .ai_workspace_session_overrides
            .remove(workspace_key)
            .is_some();
        if state_changed {
            self.persist_state();
        }
    }

    fn begin_ai_git_progress(
        &mut self,
        epoch: usize,
        action: AiGitProgressAction,
        steps: Vec<AiGitProgressStep>,
        step: AiGitProgressStep,
        detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.ai_git_progress = Some(AiGitProgressState::new(epoch, action, steps, step, detail));
        cx.notify();
    }

    fn apply_ai_git_progress(
        &mut self,
        epoch: usize,
        update: AiGitProgressEvent,
        cx: &mut Context<Self>,
    ) {
        if epoch != self.git_action_epoch {
            return;
        }
        let Some(progress) = self.ai_git_progress.as_mut() else {
            return;
        };
        if progress.epoch != epoch {
            return;
        }
        progress.apply(update.step, update.detail);
        cx.notify();
    }

    pub(super) fn ai_commit_and_push_for_current_thread(&mut self, cx: &mut Context<Self>) {
        if let Some(reason) = self.ai_publish_blocker().filter(|reason| !reason.is_empty()) {
            let message = format!("Publish unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, None, cx);
            cx.notify();
            return;
        }

        let context = match self.ai_current_thread_git_action_context("publishing") {
            Ok(context) => context,
            Err(reason) => {
                let message = format!("Publish unavailable: {reason}");
                self.git_status_message = Some(message.clone());
                Self::push_warning_notification(message, None, cx);
                cx.notify();
                return;
            }
        };
        let fallback_commit_message = ai_commit_message_for_thread(
            &self.ai_state_snapshot,
            context.thread_id.as_str(),
            context.branch_name.as_str(),
        );
        let prompt_seed =
            ai_first_prompt_seed_for_thread(&self.ai_state_snapshot, context.thread_id.as_str());
        let latest_agent_message =
            ai_latest_agent_message_for_thread(&self.ai_state_snapshot, context.thread_id.as_str());
        let codex_executable = Self::resolve_codex_executable_path();
        let branch_name = context.branch_name.clone();
        let repo_root = context.repo_root.clone();
        let epoch = self.begin_git_action("Commit and Push", cx);
        self.begin_ai_git_progress(
            epoch,
            AiGitProgressAction::CommitAndPush,
            ai_commit_and_push_progress_steps(),
            AiGitProgressStep::GeneratingCommitMessage,
            Some(ai_branch_progress_detail("Branch", branch_name.as_str())),
            cx,
        );
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (progress_tx, mut progress_rx) = mpsc::unbounded::<AiGitProgressEvent>();
            let git_task = cx.background_executor().spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = (|| -> anyhow::Result<(Option<String>, String)> {
                        let commit_message = resolve_ai_commit_message_for_working_copy(
                            AiCodexGenerationConfig {
                                codex_executable: codex_executable.as_path(),
                                repo_root: repo_root.as_path(),
                            },
                            repo_root.as_path(),
                            branch_name.as_str(),
                            prompt_seed.as_deref(),
                            latest_agent_message.as_deref(),
                            &fallback_commit_message,
                        );
                        send_ai_git_progress(
                            &progress_tx,
                            AiGitProgressStep::CreatingCommit,
                            Some(ai_commit_progress_detail(commit_message.subject.as_str())),
                        );
                        let commit_message_text = commit_message.as_git_message();
                        let committed_subject = match commit_staged_with_details(
                            repo_root.as_path(),
                            commit_message_text.as_str(),
                        ) {
                            Ok(created) => Some(created.subject),
                            Err(err) if err.to_string().contains("no changes to commit") => None,
                            Err(err) => return Err(err),
                        };

                        send_ai_git_progress(
                            &progress_tx,
                            AiGitProgressStep::PushingBranch,
                            Some(ai_branch_progress_detail("Branch", branch_name.as_str())),
                        );
                        let push_result = match push_current_branch(
                            repo_root.as_path(),
                            branch_name.as_str(),
                            true,
                        ) {
                            Ok(()) => Ok(()),
                            Err(err)
                                if err
                                    .to_string()
                                    .contains("publish this branch before pushing") =>
                            {
                                push_current_branch(repo_root.as_path(), branch_name.as_str(), false)
                            }
                            Err(err) if err.to_string().contains("already published") => {
                                push_current_branch(repo_root.as_path(), branch_name.as_str(), true)
                            }
                            Err(err) => Err(err),
                        };
                        push_result?;

                        Ok((committed_subject, branch_name))
                    })();

                    (execution_started_at.elapsed(), result)
                });

            while let Some(update) = progress_rx.next().await {
                let Some(this) = this.upgrade() else {
                    break;
                };
                this.update(cx, move |this, cx| {
                    this.apply_ai_git_progress(epoch, update, cx);
                });
            }
            let (execution_elapsed, result) = git_task.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok((committed_subject, branch_name)) => {
                            debug!(
                                "git action complete: epoch={} action=Commit and Push exec_elapsed_ms={} total_elapsed_ms={} branch={}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis(),
                                branch_name
                            );
                            let committed = committed_subject.is_some();
                            if let Some(subject) = committed_subject {
                                this.last_commit_subject = Some(subject);
                            }
                            this.request_snapshot_refresh_workflow_only(true, cx);
                            this.request_recent_commits_refresh(true, cx);
                            let message = if committed {
                                format!("Committed and pushed {}", branch_name)
                            } else {
                                format!("Pushed {}", branch_name)
                            };
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
                });
            }
        });
    }

    pub(super) fn ai_open_pr_for_current_thread(&mut self, cx: &mut Context<Self>) {
        if let Some(reason) = self.ai_open_pr_blocker().filter(|reason| !reason.is_empty()) {
            let message = format!("Open PR unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, None, cx);
            cx.notify();
            return;
        }

        let context = match self.ai_current_thread_git_action_context("opening PR") {
            Ok(context) => context,
            Err(reason) => {
                let message = format!("Open PR unavailable: {reason}");
                self.git_status_message = Some(message.clone());
                Self::push_warning_notification(message, None, cx);
                cx.notify();
                return;
            }
        };
        let fallback_commit_message = ai_commit_message_for_thread(
            &self.ai_state_snapshot,
            context.thread_id.as_str(),
            context.branch_name.as_str(),
        );
        let fallback_review_title = fallback_commit_message.subject.clone();
        let prompt_seed =
            ai_first_prompt_seed_for_thread(&self.ai_state_snapshot, context.thread_id.as_str());
        let latest_agent_message =
            ai_latest_agent_message_for_thread(&self.ai_state_snapshot, context.thread_id.as_str());
        let codex_executable = Self::resolve_codex_executable_path();
        let provider_mappings = self.config.review_provider_mappings.clone();
        let fallback_review_branch_name = ai_branch_name_for_thread(
            &self.ai_state_snapshot,
            context.thread_id.as_str(),
            context.branch_name.as_str(),
            false,
        );
        let review_branch_generation_seed = ai_branch_generation_seed_for_thread(
            &self.ai_state_snapshot,
            context.thread_id.as_str(),
            context.branch_name.as_str(),
        );
        let repo_root = context.repo_root.clone();
        let branch_name = context.branch_name.clone();
        let start_mode = context.start_mode;
        let epoch = self.begin_git_action("Open PR", cx);
        let create_review_branch = start_mode == AiNewThreadStartMode::Local;
        let initial_step = if create_review_branch {
            AiGitProgressStep::GeneratingBranchName
        } else {
            AiGitProgressStep::GeneratingCommitMessage
        };
        let initial_detail = if create_review_branch {
            Some(ai_branch_progress_detail("Current branch", branch_name.as_str()))
        } else {
            Some(ai_branch_progress_detail("Branch", branch_name.as_str()))
        };
        self.begin_ai_git_progress(
            epoch,
            AiGitProgressAction::OpenPr,
            ai_open_pr_progress_steps(create_review_branch),
            initial_step,
            initial_detail,
            cx,
        );
        let started_at = Instant::now();

        self.git_action_task = cx.spawn(async move |this, cx| {
            let (progress_tx, mut progress_rx) = mpsc::unbounded::<AiGitProgressEvent>();
            let git_task = cx.background_executor().spawn(async move {
                    let execution_started_at = Instant::now();
                    let result = (|| -> anyhow::Result<(Option<String>, String, String)> {
                        let review_branch_name = if start_mode == AiNewThreadStartMode::Local {
                            let requested_branch_name = try_ai_branch_name_for_prompt(
                                codex_executable.as_path(),
                                repo_root.as_path(),
                                review_branch_generation_seed.as_str(),
                                &[],
                                false,
                            )
                            .unwrap_or_else(|| fallback_review_branch_name.clone());
                            send_ai_git_progress(
                                &progress_tx,
                                AiGitProgressStep::CreatingReviewBranch,
                                Some(ai_branch_progress_detail(
                                    "Review branch",
                                    requested_branch_name.as_str(),
                                )),
                            );
                            activate_new_ai_review_branch(
                                repo_root.as_path(),
                                requested_branch_name.as_str(),
                            )?
                        } else {
                            branch_name.clone()
                        };

                        send_ai_git_progress(
                            &progress_tx,
                            AiGitProgressStep::GeneratingCommitMessage,
                            Some(ai_branch_progress_detail(
                                "Review branch",
                                review_branch_name.as_str(),
                            )),
                        );
                        let commit_message = resolve_ai_commit_message_for_working_copy(
                            AiCodexGenerationConfig {
                                codex_executable: codex_executable.as_path(),
                                repo_root: repo_root.as_path(),
                            },
                            repo_root.as_path(),
                            review_branch_name.as_str(),
                            prompt_seed.as_deref(),
                            latest_agent_message.as_deref(),
                            &fallback_commit_message,
                        );
                        send_ai_git_progress(
                            &progress_tx,
                            AiGitProgressStep::CreatingCommit,
                            Some(ai_commit_progress_detail(commit_message.subject.as_str())),
                        );
                        let commit_message_text = commit_message.as_git_message();
                        let committed_subject = match commit_staged_with_details(
                            repo_root.as_path(),
                            commit_message_text.as_str(),
                        ) {
                            Ok(created) => Some(created.subject),
                            Err(err) if err.to_string().contains("no changes to commit") => None,
                            Err(err) => return Err(err),
                        };

                        send_ai_git_progress(
                            &progress_tx,
                            AiGitProgressStep::PushingBranch,
                            Some(ai_branch_progress_detail(
                                "Review branch",
                                review_branch_name.as_str(),
                            )),
                        );
                        let push_result = match push_current_branch(
                            repo_root.as_path(),
                            review_branch_name.as_str(),
                            true,
                        ) {
                            Ok(()) => Ok(()),
                            Err(err)
                                if err
                                    .to_string()
                                    .contains("publish this branch before pushing") =>
                            {
                                push_current_branch(
                                    repo_root.as_path(),
                                    review_branch_name.as_str(),
                                    false,
                                )
                            }
                            Err(err) if err.to_string().contains("already published") => {
                                push_current_branch(
                                    repo_root.as_path(),
                                    review_branch_name.as_str(),
                                    true,
                                )
                            }
                            Err(err) => Err(err),
                        };
                        push_result?;

                        send_ai_git_progress(
                            &progress_tx,
                            AiGitProgressStep::PreparingReviewUrl,
                            Some(ai_branch_progress_detail(
                                "Review branch",
                                review_branch_name.as_str(),
                            )),
                        );
                        let review_url = review_url_for_branch_with_provider_map(
                            repo_root.as_path(),
                            review_branch_name.as_str(),
                            &provider_mappings,
                        )?
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "no review URL found for {review_branch_name}; configure review_provider_mappings for self-hosted remotes"
                            )
                        })?;
                        let review_title = committed_subject
                            .clone()
                            .unwrap_or_else(|| fallback_review_title.clone());
                        let review_url = with_review_title_prefill(review_url, review_title.as_str());
                        send_ai_git_progress(
                            &progress_tx,
                            AiGitProgressStep::OpeningBrowser,
                            Some(review_title.clone()),
                        );

                        Ok((committed_subject, review_url, review_branch_name))
                    })();

                    (execution_started_at.elapsed(), result)
                });

            while let Some(update) = progress_rx.next().await {
                let Some(this) = this.upgrade() else {
                    break;
                };
                this.update(cx, move |this, cx| {
                    this.apply_ai_git_progress(epoch, update, cx);
                });
            }
            let (execution_elapsed, result) = git_task.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.git_action_epoch {
                        return;
                    }

                    let total_elapsed = started_at.elapsed();
                    this.finish_git_action();
                    match result {
                        Ok((committed_subject, review_url, branch_name)) => {
                            debug!(
                                "git action complete: epoch={} action=Open PR exec_elapsed_ms={} total_elapsed_ms={} branch={} mode={:?}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis(),
                                branch_name,
                                start_mode
                            );
                            if let Some(subject) = committed_subject {
                                this.last_commit_subject = Some(subject);
                            }
                            this.request_snapshot_refresh_workflow_only(true, cx);
                            this.request_recent_commits_refresh(true, cx);
                            match open_url_in_browser(review_url.as_str()) {
                                Ok(()) => {
                                    let message = format!("Opened PR/MR in browser for {}", branch_name);
                                    this.git_status_message = Some(message.clone());
                                    Self::push_success_notification(message, cx);
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
                            }
                        }
                        Err(err) => {
                            error!(
                                "git action failed: epoch={} action=Open PR exec_elapsed_ms={} total_elapsed_ms={} mode={:?} err={err:#}",
                                epoch,
                                execution_elapsed.as_millis(),
                                total_elapsed.as_millis(),
                                start_mode
                            );
                            let summary = err.to_string();
                            this.git_status_message = Some(format!("Git error: {err:#}"));
                            Self::push_error_notification(format!("Open PR failed: {summary}"), cx);
                        }
                    }

                    cx.notify();
                });
            }
        });
    }
}

fn send_ai_git_progress(
    progress_tx: &mpsc::UnboundedSender<AiGitProgressEvent>,
    step: AiGitProgressStep,
    detail: Option<String>,
) {
    if progress_tx
        .unbounded_send(AiGitProgressEvent { step, detail })
        .is_err()
    {
        debug!("dropping AI git progress update because the receiver is gone");
    }
}

fn ai_branch_progress_detail(label: &str, branch_name: &str) -> String {
    format!("{label}: {branch_name}")
}

fn ai_commit_progress_detail(subject: &str) -> String {
    format!("Commit: {subject}")
}

fn ai_publish_blocker_reason(
    context: Result<AiThreadGitActionContext, String>,
) -> Option<String> {
    context.err()
}

fn resolve_ai_commit_message_for_working_copy(
    generation_config: AiCodexGenerationConfig<'_>,
    repo_root: &std::path::Path,
    branch_name: &str,
    prompt_seed: Option<&str>,
    latest_agent_message: Option<&str>,
    fallback_commit_message: &AiCommitMessage,
) -> AiCommitMessage {
    let working_copy_context =
        working_copy_context_for_ai(repo_root, 200, 40_000).ok().flatten();
    let Some(working_copy_context) = working_copy_context else {
        return fallback_commit_message.clone();
    };

    try_ai_commit_message(
        generation_config,
        AiCommitGenerationContext {
            branch_name,
            prompt_seed,
            latest_agent_message,
            changed_files_summary: working_copy_context.changed_files_summary.as_str(),
            diff_patch: working_copy_context.diff_patch.as_str(),
        },
    )
    .unwrap_or_else(|| fallback_commit_message.clone())
}

fn activate_new_ai_review_branch(
    repo_root: &std::path::Path,
    requested_branch_name: &str,
) -> anyhow::Result<String> {
    let mut attempt = 0usize;
    loop {
        attempt = attempt.saturating_add(1);
        let candidate_branch_name = if attempt == 1 {
            requested_branch_name.to_string()
        } else {
            format!("{requested_branch_name}-{attempt}")
        };
        match checkout_or_create_branch_with_change_transfer(
            repo_root,
            candidate_branch_name.as_str(),
            true,
        ) {
            Ok(()) => return Ok(candidate_branch_name),
            Err(err) => {
                if err.to_string().contains("already exists") && attempt < 20 {
                    continue;
                }
                return Err(err);
            }
        }
    }
}

#[cfg(test)]
mod ai_git_ops_tests {
    use super::*;

    fn test_git_action_context(start_mode: AiNewThreadStartMode) -> AiThreadGitActionContext {
        AiThreadGitActionContext {
            repo_root: std::path::PathBuf::from("/repo"),
            thread_id: "thread-1".to_string(),
            branch_name: "feature/ai-thread".to_string(),
            start_mode,
        }
    }

    #[test]
    fn publish_blocker_allows_local_threads() {
        assert_eq!(
            ai_publish_blocker_reason(Ok(test_git_action_context(AiNewThreadStartMode::Local))),
            None
        );
    }

    #[test]
    fn publish_blocker_allows_worktree_threads() {
        assert_eq!(
            ai_publish_blocker_reason(Ok(test_git_action_context(
                AiNewThreadStartMode::Worktree,
            ))),
            None
        );
    }

    #[test]
    fn publish_blocker_preserves_context_errors() {
        assert_eq!(
            ai_publish_blocker_reason(Err("Select a thread before publishing.".to_string())),
            Some("Select a thread before publishing.".to_string())
        );
    }
}
