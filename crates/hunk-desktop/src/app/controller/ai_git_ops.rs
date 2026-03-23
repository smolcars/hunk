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

    fn ai_validate_managed_worktree_delete(
        &self,
        context: &AiManagedWorktreeDeleteContext,
    ) -> Result<(), String> {
        hunk_git::worktree::validate_managed_worktree_removal(context.worktree_root.as_path())
            .map_err(|err| err.to_string())
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
        if let Err(reason) = self.ai_validate_managed_worktree_delete(&context) {
            let message = format!("Delete worktree unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, Some(window), cx);
            cx.notify();
            return;
        }
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
        if let Err(reason) = self.ai_validate_managed_worktree_delete(&context) {
            let message = format!("Delete worktree unavailable: {reason}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, None, cx);
            cx.notify();
            return;
        }
        let Some(codex_home) = resolve_codex_home_path() else {
            let message =
                "Delete worktree unavailable: unable to resolve Codex home for thread archive."
                    .to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, None, cx);
            cx.notify();
            return;
        };
        let codex_executable = Self::resolve_codex_executable_path();
        if let Err(error) = Self::validate_codex_executable_path(codex_executable.as_path()) {
            let message = format!("Delete worktree unavailable: {error}");
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, None, cx);
            cx.notify();
            return;
        }

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

        let epoch = self.begin_git_action("Delete Worktree", cx);
        self.begin_ai_git_progress(
            epoch,
            AiGitProgressAction::DeleteWorktree,
            ai_delete_worktree_progress_steps(),
            AiGitProgressStep::ArchivingThread,
            Some(ai_thread_progress_detail(
                "Thread",
                context.thread_id.as_str(),
            )),
            cx,
        );
        let workspace_key = context.workspace_key.clone();
        let worktree_root = context.worktree_root.clone();
        let worktree_name = context.worktree_name.clone();
        let thread_id = context.thread_id.clone();
        let thread_id_for_delete = thread_id.clone();
        let worktree_progress_detail = format!(
            "Removing {} at {}",
            worktree_name,
            worktree_root.display()
        );
        self.git_status_message =
            Some(format!("Archiving thread and deleting worktree {}...", worktree_name));
        self.spawn_ai_git_action_with_progress(
            epoch,
            cx,
            move |progress_tx| {
                let background_thread_id = thread_id.clone();
                let mut archived_thread = false;
                let result = (|| -> anyhow::Result<()> {
                    crate::app::ai_runtime::archive_ai_thread_for_workspace(
                        worktree_root.as_path(),
                        background_thread_id.as_str(),
                        codex_executable.as_path(),
                        codex_home.as_path(),
                    )
                    .with_context(|| {
                        format!("failed to archive thread {}", background_thread_id)
                    })?;
                    archived_thread = true;

                    send_ai_git_progress(
                        &progress_tx,
                        AiGitProgressStep::RemovingWorktree,
                        Some(worktree_progress_detail),
                    );
                    hunk_git::worktree::remove_managed_worktree(worktree_root.as_path())?;
                    Ok(())
                })();

                (archived_thread, result)
            },
            move |this, (archived_thread, result), execution_elapsed, total_elapsed, cx| {
                if epoch != this.git_action_epoch {
                    return;
                }

                this.finish_git_action();
                match result {
                    Ok(()) => {
                        debug!(
                            "git action complete: epoch={} action=Delete Worktree exec_elapsed_ms={} total_elapsed_ms={} worktree={} workspace_key={} archived_thread=true",
                            epoch,
                            execution_elapsed.as_millis(),
                            total_elapsed.as_millis(),
                            worktree_name,
                            workspace_key
                        );
                        this.shutdown_ai_runtime_for_workspace_blocking(workspace_key.as_str());
                        this.ai_forget_deleted_workspace_state(workspace_key.as_str(), cx);
                        this.refresh_workspace_targets_from_git_state(cx);
                        this.refresh_after_git_action("Delete Worktree", cx);
                        let message =
                            format!("Archived thread and deleted worktree {}", worktree_name);
                        this.git_status_message = Some(message.clone());
                        Self::push_success_notification(message, cx);
                    }
                    Err(err) => {
                        let summary = err.to_string();
                        debug!(
                            "git action failed: epoch={} action=Delete Worktree exec_elapsed_ms={} total_elapsed_ms={} worktree={} workspace_key={} archived_thread={} err={err:#}",
                            epoch,
                            execution_elapsed.as_millis(),
                            total_elapsed.as_millis(),
                            worktree_name,
                            workspace_key,
                            archived_thread
                        );
                        if archived_thread {
                            this.ai_mark_thread_archived_for_workspace(
                                workspace_key.as_str(),
                                thread_id_for_delete.as_str(),
                            );
                        }
                        if restore_selection_after_failure {
                            this.ai_restore_workspace_after_failed_delete(workspace_key.as_str(), cx);
                        }
                        let message = format!("Delete worktree failed: {summary}");
                        this.git_status_message = Some(message.clone());
                        Self::push_error_notification(message, cx);
                    }
                }

                cx.notify();
            },
        );
    }

    fn ai_restore_workspace_after_failed_delete(
        &mut self,
        workspace_key: &str,
        cx: &mut Context<Self>,
    ) {
        let current_workspace_key = self.ai_workspace_key();
        self.ai_handle_workspace_change_to(current_workspace_key, Some(workspace_key.to_string()), cx);
    }

    fn ai_mark_thread_archived_for_workspace(&mut self, workspace_key: &str, thread_id: &str) {
        let Some(state) = self.ai_workspace_states.get_mut(workspace_key) else {
            return;
        };
        let Some(thread) = state.state_snapshot.threads.get_mut(thread_id) else {
            return;
        };
        thread.status = ThreadLifecycleStatus::Archived;

        if state.state_snapshot.active_thread_for_cwd(workspace_key) == Some(thread_id) {
            state.state_snapshot.active_thread_by_cwd.remove(workspace_key);
            if let Some(next_thread_id) = state
                .state_snapshot
                .threads
                .values()
                .filter(|thread| {
                    thread.cwd == workspace_key
                        && thread.status != ThreadLifecycleStatus::Archived
                        && thread.id != thread_id
                })
                .max_by(|left, right| {
                    left.created_at
                        .cmp(&right.created_at)
                        .then_with(|| left.id.cmp(&right.id))
                })
                .map(|thread| thread.id.clone())
            {
                state
                    .state_snapshot
                    .set_active_thread_for_cwd(workspace_key.to_string(), next_thread_id);
            }
        }
        if state.selected_thread_id.as_deref() == Some(thread_id) {
            state.selected_thread_id = None;
        }
    }

    fn ai_forget_deleted_workspace_state(&mut self, workspace_key: &str, cx: &mut Context<Self>) {
        let removed_workspace_state = self.ai_workspace_states.remove(workspace_key);
        if let Some(removed_workspace_state) = removed_workspace_state {
            for thread_id in removed_workspace_state.state_snapshot.threads.keys() {
                let thread_key = AiComposerDraftKey::Thread(thread_id.clone());
                self.ai_composer_drafts.remove(&thread_key);
                self.ai_composer_status_by_draft.remove(&thread_key);
                self.state.ai_thread_session_overrides.remove(thread_id);
                self.ai_review_mode_thread_ids.remove(thread_id);
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

        if self.workspace_view_mode == WorkspaceViewMode::Ai {
            self.ai_prune_terminal_threads("forgetting deleted AI workspace state", cx);
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
        self.spawn_ai_git_action_with_progress(
            epoch,
            cx,
            move |progress_tx| {
                (|| -> anyhow::Result<(Option<String>, String)> {
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
                    push_current_branch_with_publish_fallback(
                        repo_root.as_path(),
                        branch_name.as_str(),
                    )?;

                    Ok((committed_subject, branch_name))
                })()
            },
            move |this, result, execution_elapsed, total_elapsed, cx| {
                if epoch != this.git_action_epoch {
                    return;
                }

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
            },
        );
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
        let open_pr_branch_strategy = ai_open_pr_branch_strategy(repo_root.as_path(), &branch_name);
        let create_review_branch =
            open_pr_branch_strategy == AiOpenPrBranchStrategy::CreateReviewBranch;
        let branch_detail_label = if create_review_branch {
            "Review branch"
        } else {
            "Branch"
        };
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
        self.spawn_ai_git_action_with_progress(
            epoch,
            cx,
            move |progress_tx| {
                (|| -> anyhow::Result<(Option<String>, String, String)> {
                    let review_branch_name = if open_pr_branch_strategy
                        == AiOpenPrBranchStrategy::CreateReviewBranch
                    {
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
                            branch_detail_label,
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
                            branch_detail_label,
                            review_branch_name.as_str(),
                        )),
                    );
                    push_current_branch_with_publish_fallback(
                        repo_root.as_path(),
                        review_branch_name.as_str(),
                    )?;

                    send_ai_git_progress(
                        &progress_tx,
                        AiGitProgressStep::PreparingReviewUrl,
                        Some(ai_branch_progress_detail(
                            branch_detail_label,
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
                })()
            },
            move |this, result, execution_elapsed, total_elapsed, cx| {
                if epoch != this.git_action_epoch {
                    return;
                }

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
                                let message =
                                    format!("Opened PR/MR in browser for {}", branch_name);
                                this.git_status_message = Some(message.clone());
                                Self::push_success_notification(message, cx);
                            }
                            Err(err) => {
                                error!("Open review URL failed: {err:#}");
                                let summary = err.to_string();
                                this.git_status_message =
                                    Some(format!("Open URL failed: {summary}"));
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
            },
        );
    }
}
include!("ai_git_ops/helpers.rs");
