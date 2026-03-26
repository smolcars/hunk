impl DiffViewer {
    fn ai_composer_activity_label_for_kind(kind: &str) -> &'static str {
        match kind {
            "reasoning" => "Thinking",
            "commandExecution" => "Running",
            "fileChange" => "Editing",
            "dynamicToolCall" | "mcpToolCall" | "collabAgentToolCall" => "Tool",
            "webSearch" => "Searching",
            "agentMessage" | "plan" => "Writing",
            _ => "Working",
        }
    }

    fn current_ai_composer_feedback_state_for_thread(
        &self,
        current_thread_id: Option<&str>,
    ) -> Option<AiComposerFeedbackState> {
        if let Some(status) = self.current_ai_composer_status_message()
            && let Some(tone) = ai_composer_status_tone(status)
        {
            return Some(AiComposerFeedbackState::Status {
                message: status.to_string(),
                tone,
            });
        }

        self.current_ai_composer_activity_feedback_for_thread(current_thread_id)
            .map(AiComposerFeedbackState::Activity)
    }

    fn current_ai_composer_activity_feedback_for_thread(
        &self,
        current_thread_id: Option<&str>,
    ) -> Option<AiComposerFeedbackActivity> {
        let thread_id = current_thread_id?;
        let turn_id = self.current_ai_in_progress_turn_id(thread_id)?;
        let tracking_key = format!("{thread_id}::{turn_id}");
        let started_at = *self.ai_in_progress_turn_started_at.get(tracking_key.as_str())?;
        let label = self
            .ai_state_snapshot
            .items
            .values()
            .filter(|item| {
                item.thread_id == thread_id
                    && item.turn_id == turn_id
                    && item.status != hunk_codex::state::ItemStatus::Completed
            })
            .max_by_key(|item| item.last_sequence)
            .map(|item| Self::ai_composer_activity_label_for_kind(item.kind.as_str()))
            .unwrap_or("Working");

        Some(AiComposerFeedbackActivity {
            label: label.to_string(),
            started_at,
            animation_key: tracking_key,
        })
    }

    fn ai_review_compare_base_branch_name_for_workspace(
        &self,
        workspace_key: &str,
    ) -> Option<String> {
        if self.ai_workspace_key().as_deref() == Some(workspace_key) {
            return self.ai_selected_worktree_base_branch_name().map(str::to_string);
        }

        self.ai_workspace_states
            .get(workspace_key)
            .and_then(|state| state.worktree_base_branch_name.clone())
    }

    fn ai_selected_thread_review_compare_selection(
        &self,
    ) -> Option<(Option<String>, Option<String>)> {
        let selected_thread_id = self.ai_selected_thread_id.as_deref()?;
        let workspace_root = self.ai_thread_workspace_root(selected_thread_id)?;
        let workspace_key = workspace_root.to_string_lossy().to_string();
        let preferred_base_branch_name = if self.ai_thread_start_mode(selected_thread_id)
            == Some(AiNewThreadStartMode::Worktree)
        {
            self.ai_review_compare_base_branch_name_for_workspace(workspace_key.as_str())
        } else {
            None
        };
        let default_base_branch_name = self
            .project_path
            .as_deref()
            .and_then(|path| resolve_default_base_branch_name(path).ok().flatten());

        review_compare_selection_ids_for_workspace_root(
            &self.review_compare_sources,
            &self.workspace_targets,
            workspace_root.as_path(),
            preferred_base_branch_name.as_deref(),
            default_base_branch_name.as_deref(),
        )
    }

    pub(super) fn ai_open_review_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(selected_thread_id) = self.ai_selected_thread_id.clone() {
            if let Some(project_root) = self.ai_thread_project_root(selected_thread_id.as_str())
                && self.project_path.as_ref() != Some(&project_root)
            {
                self.activate_workspace_project_root(project_root, cx);
            }
            if let Some(workspace_root) = self.ai_thread_workspace_root(selected_thread_id.as_str())
                && let Some(target_id) = self
                    .workspace_targets
                    .iter()
                    .find(|target| target.root == workspace_root)
                    .map(|target| target.id.clone())
                && self.active_workspace_target_id.as_deref() != Some(target_id.as_str())
            {
                self.activate_workspace_target(target_id, cx);
            }
        }

        if let Some((left_source_id, right_source_id)) =
            self.ai_selected_thread_review_compare_selection()
        {
            self.update_review_compare_selection(left_source_id, right_source_id, cx);
        }
        self.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
    }

    pub(crate) fn ai_threads_for_current_workspace(&self) -> Vec<ThreadSummary> {
        let threads = sorted_threads(&self.ai_state_snapshot)
            .into_iter()
            .filter(|thread| {
                thread.status != ThreadLifecycleStatus::Archived
                    && ai_thread_workspace_matches_current_project(
                        std::path::Path::new(thread.cwd.as_str()),
                        self.workspace_targets.as_slice(),
                        self.project_path.as_deref(),
                        self.repo_root.as_deref(),
                    )
            })
            .collect::<Vec<_>>();

        crate::app::ai_bookmarks::bookmark_first_sorted_threads(
            threads,
            &self.state.ai_bookmarked_thread_ids,
        )
    }

    pub(crate) fn ai_visible_thread_sections(&self) -> &[AiVisibleThreadProjectSection] {
        self.ai_thread_sidebar_sections.as_slice()
    }

    pub(crate) fn ai_thread_sidebar_rows(&self) -> &[AiThreadSidebarRow] {
        self.ai_thread_sidebar_rows.as_slice()
    }

    pub(super) fn rebuild_ai_thread_sidebar_state(&mut self) {
        let rebuild_started_at = Instant::now();
        let threads = self.ai_visible_threads();
        let sections = ai_visible_thread_sections(
            threads,
            self.state.workspace_project_paths.as_slice(),
            self.project_path.as_deref(),
            self.repo_root.as_deref(),
            &self.ai_expanded_thread_sidebar_project_roots,
        );
        let rows = self.ai_thread_sidebar_rows_from_sections(sections.as_slice());
        let visible_thread_count = sections
            .iter()
            .map(|section| section.total_thread_count)
            .sum::<usize>();
        if let Some(state) = self.ai_visible_frame_state.as_mut() {
            state.project_count = sections.len();
            state.visible_thread_count = visible_thread_count;
            state.threads_loading = self.ai_bootstrap_loading && visible_thread_count == 0;
        }
        self.ai_thread_sidebar_sections = sections;
        self.ai_thread_sidebar_rows = rows;
        self.record_ai_thread_sidebar_rebuild_timing(rebuild_started_at.elapsed());
    }

    pub(super) fn invalidate_ai_visible_frame_state_with_reason(
        &mut self,
        reason: &'static str,
    ) {
        self.record_ai_visible_frame_invalidation(reason);
        self.ai_visible_frame_state = None;
    }

    pub(super) fn visible_ai_frame_state(&mut self) -> AiVisibleFrameState {
        if let Some(state) = self.ai_visible_frame_state.clone() {
            self.record_ai_visible_frame_cache_hit();
            return state;
        }

        let build_started_at = Instant::now();
        let resolved = self.resolve_ai_current_state();
        let (project_count, visible_thread_count, threads_loading) = {
            let project_count = self.ai_visible_thread_sections().len();
            let visible_thread_count = self
                .ai_visible_thread_sections()
                .iter()
                .map(|section| section.total_thread_count)
                .sum::<usize>();
            let threads_loading = self.ai_bootstrap_loading && visible_thread_count == 0;
            (project_count, visible_thread_count, threads_loading)
        };
        let (toolbar_project_label, toolbar_repo_label, active_branch, active_workspace_label) = {
            let toolbar_project_label = self
                .ai_visible_project_root_with_context(
                    resolved.current_thread_id.as_deref(),
                    resolved.workspace_root.as_deref(),
                )
                .or_else(|| self.repo_root.clone())
                .as_deref()
                .map(crate::app::project_picker::project_display_name)
                .unwrap_or_else(|| {
                    self.repo_root
                        .as_ref()
                        .or(self.project_path.as_ref())
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().to_string())
                        .filter(|label| !label.is_empty())
                        .unwrap_or_else(|| "Hunk".to_string())
                });
            let toolbar_repo_label = resolved
                .workspace_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "No Git repository found".to_string());
            let active_branch =
                self.ai_active_workspace_branch_name_with_root(resolved.workspace_root.as_deref());
            let active_workspace_label =
                self.ai_active_workspace_label_with_root(resolved.workspace_root.as_deref());
            (
                toolbar_project_label,
                toolbar_repo_label,
                active_branch,
                active_workspace_label,
            )
        };
        let (pending_approvals, pending_user_inputs) = {
            (
                self.ai_visible_pending_approvals(),
                self.ai_visible_pending_user_inputs(),
            )
        };
        let (
            selected_thread_id,
            pending_thread_start,
            selected_thread_start_mode,
            selected_workspace_root,
            show_worktree_base_branch_picker,
            selected_worktree_base_branch,
        ) = {
            let selected_thread_id = resolved.current_thread_id.clone();
            let pending_thread_start = self.ai_pending_thread_start_for_timeline_with_context(
                resolved.current_thread_id.as_deref(),
                resolved.workspace_key.as_deref(),
            );
            let selected_thread_start_mode = selected_thread_id
                .as_deref()
                .and_then(|_| {
                    resolved.current_thread_workspace_root.as_deref().map(|thread_root| {
                        ai_thread_start_mode_for_workspace(
                            self.repo_root.as_deref(),
                            &self.workspace_targets,
                            thread_root,
                        )
                    })
                })
                .flatten();
            let selected_workspace_root = resolved.workspace_root.clone();
            let show_worktree_base_branch_picker = self.ai_show_worktree_base_branch_picker();
            let selected_worktree_base_branch = self
                .ai_selected_worktree_base_branch_name()
                .unwrap_or("Choose base branch")
                .to_string();
            (
                selected_thread_id,
                pending_thread_start,
                selected_thread_start_mode,
                selected_workspace_root,
                show_worktree_base_branch_picker,
                selected_worktree_base_branch,
            )
        };
        let timeline_rows_started_at = Instant::now();
        let (
            timeline_total_turn_count,
            timeline_visible_turn_count,
            timeline_hidden_turn_count,
            timeline_visible_row_ids,
        ) = if let Some(thread_id) = selected_thread_id.as_deref() {
            self.ai_timeline_visible_rows_for_thread(thread_id)
        } else {
            (0, 0, 0, Vec::new())
        };
        self.record_ai_visible_frame_timeline_rows_timing(timeline_rows_started_at.elapsed());
        let show_no_turns_empty_state = crate::app::render::ai_should_show_no_turns_empty_state(
            timeline_visible_row_ids.len(),
            pending_thread_start.is_some(),
        );
        let timeline_loading = self.ai_bootstrap_loading
            && selected_thread_id.is_some()
            && timeline_visible_row_ids.is_empty();
        let show_select_thread_empty_state =
            selected_thread_id.is_none() && !timeline_loading && pending_thread_start.is_none();
        let (
            composer_feedback,
            composer_attachment_paths,
            composer_send_waiting_on_connection,
            composer_interrupt_available,
            queued_message_count,
            model_supports_image_inputs,
            selected_thread_in_progress,
        ) = {
            let composer_feedback_started_at = Instant::now();
            let composer_feedback =
                self.current_ai_composer_feedback_state_for_thread(resolved.current_thread_id.as_deref());
            self.record_ai_visible_frame_composer_feedback_timing(
                composer_feedback_started_at.elapsed(),
            );
            let composer_attachment_paths = self
                .current_ai_composer_draft()
                .map(|draft| Arc::<[PathBuf]>::from(draft.local_images.clone()))
                .unwrap_or_else(|| Arc::<[PathBuf]>::from(Vec::<PathBuf>::new()));
            let composer_send_waiting_on_connection =
                crate::app::controller::ai_prompt_send_waiting_on_connection(
                    self.ai_connection_state,
                    self.ai_bootstrap_loading,
                );
            let composer_interrupt_available = selected_thread_id
                .as_deref()
                .and_then(|thread_id| self.current_ai_in_progress_turn_id(thread_id))
                .is_some();
            let queued_message_count = selected_thread_id
                .as_deref()
                .map(|thread_id| self.ai_queued_message_row_ids_for_thread(thread_id).len())
                .unwrap_or(0);
            let model_supports_image_inputs = self.current_ai_model_supports_image_inputs();
            let selected_thread_in_progress = selected_thread_id
                .as_deref()
                .and_then(|thread_id| self.current_ai_in_progress_turn_id(thread_id))
                .is_some();
            (
                composer_feedback,
                composer_attachment_paths,
                composer_send_waiting_on_connection,
                composer_interrupt_available,
                queued_message_count,
                model_supports_image_inputs,
                selected_thread_in_progress,
            )
        };
        let (
            review_action_blocker,
            ai_publish_blocker,
            ai_publish_disabled,
            ai_open_pr_disabled,
            ai_managed_worktree_target,
            ai_delete_worktree_blocker,
            terminal_cwd_label,
        ) = {
            let review_action_blocker = match selected_thread_id.as_deref() {
                Some(_) if selected_thread_in_progress => {
                    Some("Wait for the current run to finish or interrupt it first.".to_string())
                }
                Some(_) => None,
                None => Some("Select a thread before starting review.".to_string()),
            };
            let ai_publish_blocker = match (
                self.git_controls_busy(),
                selected_thread_id.as_deref(),
                selected_thread_start_mode,
                selected_workspace_root.as_deref(),
            ) {
                (true, _, _, _) => Some("Another workspace action is in progress.".to_string()),
                (_, None, _, _) => Some("Select a thread before publishing.".to_string()),
                (_, Some(_), None, _) => {
                    Some("Unable to resolve the selected thread before publishing.".to_string())
                }
                (_, Some(_), Some(_), None) => {
                    Some("Open a workspace before publishing.".to_string())
                }
                (_, Some(thread_id), Some(start_mode), Some(repo_root)) => {
                    let normalized_branch = active_branch.trim();
                    if normalized_branch.is_empty()
                        || matches!(normalized_branch, "detached" | "unknown")
                    {
                        Some("Activate a branch before publishing.".to_string())
                    } else {
                        let _context = AiThreadGitActionContext {
                            repo_root: repo_root.to_path_buf(),
                            thread_id: thread_id.to_string(),
                            branch_name: normalized_branch.to_string(),
                            start_mode,
                        };
                        None
                    }
                }
            };
            let ai_publish_disabled = ai_publish_blocker.is_some();
            let ai_open_pr_disabled = match (
                self.git_controls_busy(),
                selected_thread_id.as_deref(),
                selected_thread_start_mode,
                selected_workspace_root.as_deref(),
            ) {
                (true, _, _, _) => true,
                (_, Some(_), Some(_), Some(_)) => {
                    let normalized_branch = active_branch.trim();
                    normalized_branch.is_empty()
                        || matches!(normalized_branch, "detached" | "unknown")
                }
                _ => true,
            };
            let ai_managed_worktree_target = if selected_thread_start_mode
                == Some(AiNewThreadStartMode::Worktree)
            {
                selected_workspace_root.as_deref().and_then(|workspace_root| {
                    self.workspace_targets
                        .iter()
                        .find(|target| {
                            target.root.as_path() == workspace_root
                                && target.kind
                                    == hunk_git::worktree::WorkspaceTargetKind::LinkedWorktree
                                && target.managed
                        })
                        .cloned()
                })
            } else {
                None
            };
            let ai_delete_worktree_blocker = ai_managed_worktree_target.as_ref().and_then(|_| {
                if self.git_controls_busy() {
                    Some("Another workspace action is in progress.".to_string())
                } else if selected_thread_in_progress {
                    Some("Wait for the current run to finish or interrupt it first.".to_string())
                } else {
                    None
                }
            });
            let terminal_cwd_label = self
                .ai_terminal_session
                .cwd
                .clone()
                .or_else(|| selected_workspace_root.clone())
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "No workspace selected".to_string());
            (
                review_action_blocker,
                ai_publish_blocker,
                ai_publish_disabled,
                ai_open_pr_disabled,
                ai_managed_worktree_target,
                ai_delete_worktree_blocker,
                terminal_cwd_label,
            )
        };

        let state = AiVisibleFrameState {
            project_count,
            visible_thread_count,
            threads_loading,
            toolbar_project_label,
            toolbar_repo_label,
            active_branch,
            active_workspace_label,
            pending_approvals: pending_approvals.into(),
            pending_user_inputs: pending_user_inputs.into(),
            selected_thread_id,
            pending_thread_start,
            selected_thread_start_mode,
            show_worktree_base_branch_picker,
            selected_worktree_base_branch,
            timeline_total_turn_count,
            timeline_visible_turn_count,
            timeline_hidden_turn_count,
            timeline_visible_row_ids: timeline_visible_row_ids.into(),
            timeline_loading,
            show_select_thread_empty_state,
            show_no_turns_empty_state,
            composer_feedback,
            composer_attachment_paths,
            composer_send_waiting_on_connection,
            composer_interrupt_available,
            queued_message_count,
            model_supports_image_inputs,
            review_action_blocker,
            ai_publish_blocker,
            ai_publish_disabled,
            ai_open_pr_disabled,
            ai_managed_worktree_target,
            ai_delete_worktree_blocker,
            terminal_cwd_label,
        };
        self.ai_visible_frame_state = Some(state.clone());
        self.record_ai_visible_frame_build_timing(build_started_at.elapsed());
        state
    }

    fn ai_thread_sidebar_rows_from_sections(
        &self,
        sections: &[AiVisibleThreadProjectSection],
    ) -> Vec<AiThreadSidebarRow> {
        let mut rows = Vec::new();
        for section in sections {
            rows.push(AiThreadSidebarRow {
                kind: AiThreadSidebarRowKind::ProjectHeader {
                    project_root: section.project_root.clone(),
                    project_label: section.project_label.clone(),
                    total_thread_count: section.total_thread_count,
                },
            });
            if section.threads.is_empty() {
                rows.push(AiThreadSidebarRow {
                    kind: AiThreadSidebarRowKind::EmptyProject {
                        project_root: section.project_root.clone(),
                    },
                });
            } else {
                rows.extend(section.threads.iter().cloned().map(|thread| AiThreadSidebarRow {
                    kind: AiThreadSidebarRowKind::Thread {
                        workspace_label: self
                            .ai_workspace_label_for_root(std::path::Path::new(thread.cwd.as_str())),
                        thread,
                    },
                }));
            }
            if section.hidden_thread_count > 0 || section.expanded {
                rows.push(AiThreadSidebarRow {
                    kind: AiThreadSidebarRowKind::ProjectFooter {
                        project_root: section.project_root.clone(),
                        hidden_thread_count: section.hidden_thread_count,
                        expanded: section.expanded,
                    },
                });
            }
        }
        rows
    }

    fn ai_state_snapshot_workspace_key(&self) -> Option<String> {
        let draft_workspace_key = self.ai_workspace_key_for_draft();
        state_snapshot_workspace_key(
            &self.ai_state_snapshot,
            self.ai_selected_thread_id.as_deref(),
            self.ai_worker_workspace_key.as_deref(),
            draft_workspace_key.as_deref(),
            self.ai_new_thread_draft_active,
            self.ai_pending_new_thread_selection,
        )
    }

    fn ai_thread_summary(&self, thread_id: &str) -> Option<ThreadSummary> {
        let workspace_project_roots = ai_workspace_project_roots(
            self.state.workspace_project_paths.as_slice(),
            self.project_path.as_deref(),
            self.repo_root.as_deref(),
        );
        self.ai_state_snapshot
            .threads
            .get(thread_id)
            .filter(|thread| {
                ai_workspace_project_root_for_thread_root(
                    std::path::Path::new(thread.cwd.as_str()),
                    workspace_project_roots.as_slice(),
                )
                .is_some()
            })
            .cloned()
            .or_else(|| {
                self.ai_workspace_states
                    .values()
                    .find_map(|state| {
                        state.state_snapshot.threads.get(thread_id).filter(|thread| {
                            ai_workspace_project_root_for_thread_root(
                                std::path::Path::new(thread.cwd.as_str()),
                                workspace_project_roots.as_slice(),
                            )
                            .is_some()
                        })
                    })
                    .cloned()
            })
    }

    fn ai_thread_workspace_root(&self, thread_id: &str) -> Option<std::path::PathBuf> {
        self.ai_thread_summary(thread_id)
            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
            .map(|thread| std::path::PathBuf::from(thread.cwd))
    }

    fn ai_thread_project_root(&self, thread_id: &str) -> Option<std::path::PathBuf> {
        let workspace_project_roots = ai_workspace_project_roots(
            self.state.workspace_project_paths.as_slice(),
            self.project_path.as_deref(),
            self.repo_root.as_deref(),
        );
        self.ai_thread_workspace_root(thread_id).and_then(|thread_workspace_root| {
            ai_workspace_project_root_for_thread_root(
                thread_workspace_root.as_path(),
                workspace_project_roots.as_slice(),
            )
        })
    }

    fn ai_pending_approval(&self, request_id: &str) -> Option<AiPendingApproval> {
        self.ai_pending_approvals
            .iter()
            .find(|approval| approval.request_id == request_id)
            .cloned()
            .or_else(|| {
                self.ai_workspace_states
                    .values()
                    .find_map(|state| {
                        state
                            .pending_approvals
                            .iter()
                            .find(|approval| approval.request_id == request_id)
                            .cloned()
                    })
            })
    }

    fn ai_pending_user_input_request(
        &self,
        request_id: &str,
    ) -> Option<AiPendingUserInputRequest> {
        self.ai_pending_user_inputs
            .iter()
            .find(|request| request.request_id == request_id)
            .cloned()
            .or_else(|| {
                self.ai_workspace_states
                    .values()
                    .find_map(|state| {
                        state
                            .pending_user_inputs
                            .iter()
                            .find(|request| request.request_id == request_id)
                            .cloned()
                    })
            })
    }

    fn ai_pending_user_input_answers_mut_for_workspace(
        &mut self,
        workspace_key: Option<&str>,
    ) -> Option<&mut BTreeMap<String, BTreeMap<String, Vec<String>>>> {
        let workspace_key = workspace_key?;
        if self.ai_workspace_key().as_deref() == Some(workspace_key) {
            return Some(&mut self.ai_pending_user_input_answers);
        }
        self.ai_workspace_states
            .get_mut(workspace_key)
            .map(|state| &mut state.pending_user_input_answers)
    }

    pub(super) fn ai_visible_threads(&self) -> Vec<ThreadSummary> {
        let state_snapshot_workspace_key = self.ai_state_snapshot_workspace_key();
        let threads = merged_ai_visible_threads(
            &self.ai_state_snapshot,
            state_snapshot_workspace_key.as_deref(),
            &self.ai_workspace_states,
            self.state.workspace_project_paths.as_slice(),
            self.project_path.as_deref(),
            self.repo_root.as_deref(),
        );
        crate::app::ai_bookmarks::bookmark_first_sorted_threads(
            threads,
            &self.state.ai_bookmarked_thread_ids,
        )
    }

    pub(super) fn ai_toggle_thread_sidebar_project_expanded(
        &mut self,
        project_root: String,
        cx: &mut Context<Self>,
    ) {
        if !self
            .ai_expanded_thread_sidebar_project_roots
            .insert(project_root.clone())
        {
            self.ai_expanded_thread_sidebar_project_roots
                .remove(project_root.as_str());
        }
        self.rebuild_ai_thread_sidebar_state();
        cx.notify();
    }

    pub(super) fn ai_timeline_turn_ids(&self, thread_id: &str) -> &[String] {
        self.ai_timeline_turn_ids_by_thread
            .get(thread_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn ai_timeline_row_ids(&self, thread_id: &str) -> &[String] {
        self.ai_timeline_row_ids_by_thread
            .get(thread_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn ai_timeline_row(&self, row_id: &str) -> Option<&AiTimelineRow> {
        self.ai_timeline_rows_by_id.get(row_id)
    }

    pub(super) fn ai_timeline_group(&self, group_id: &str) -> Option<&AiTimelineGroup> {
        self.ai_timeline_groups_by_id.get(group_id)
    }

    fn ai_timeline_container_row_id(&self, row_id: &str) -> Option<String> {
        self.ai_timeline_group_parent_by_child_row_id
            .get(row_id)
            .cloned()
            .or_else(|| self.ai_timeline_rows_by_id.contains_key(row_id).then(|| row_id.to_string()))
    }

    pub(super) fn ai_timeline_visible_rows_for_thread(
        &self,
        thread_id: &str,
    ) -> (usize, usize, usize, Vec<String>) {
        let turn_ids = self.ai_timeline_turn_ids(thread_id);
        let configured_limit = self
            .ai_timeline_visible_turn_limit_by_thread
            .get(thread_id)
            .copied()
            .unwrap_or(AI_TIMELINE_DEFAULT_VISIBLE_TURNS);
        let (total_turn_count, visible_turn_count, hidden_turn_count, visible_turn_ids) =
            timeline_visible_turn_ids(turn_ids, configured_limit);
        let row_ids = self.ai_timeline_row_ids(thread_id);
        let mut visible_row_ids = timeline_visible_row_ids_for_turns(
            row_ids,
            &self.ai_timeline_rows_by_id,
            visible_turn_ids.as_slice(),
        )
        .into_iter()
        .filter(|row_id| {
            self.ai_timeline_row(row_id.as_str())
                .is_some_and(|row| ai_timeline_row_is_renderable_for_controller(self, row))
        })
        .collect::<Vec<_>>();
        visible_row_ids.extend(self.ai_pending_steer_row_ids_for_thread(thread_id));
        visible_row_ids.extend(self.ai_queued_message_row_ids_for_thread(thread_id));
        (
            total_turn_count,
            visible_turn_count,
            hidden_turn_count,
            visible_row_ids,
        )
    }

    fn rebuild_ai_timeline_indexes(&mut self) {
        let rebuild_started_at = Instant::now();
        self.ai_timeline_turn_ids_by_thread = timeline_turn_ids_by_thread(&self.ai_state_snapshot);

        let mut base_rows_by_thread = BTreeMap::<String, Vec<(u64, String)>>::new();
        let mut rows_by_id = BTreeMap::<String, AiTimelineRow>::new();
        let turn_keys_with_file_change_items =
            ai_turn_keys_with_file_change_items(&self.ai_state_snapshot);
        for (item_key, item) in &self.ai_state_snapshot.items {
            let row_id = format!("item:{item_key}");
            base_rows_by_thread
                .entry(item.thread_id.clone())
                .or_default()
                .push((item.last_sequence, row_id.clone()));
            rows_by_id.insert(
                row_id.clone(),
                AiTimelineRow {
                    id: row_id,
                    thread_id: item.thread_id.clone(),
                    turn_id: item.turn_id.clone(),
                    last_sequence: item.last_sequence,
                    source: AiTimelineRowSource::Item {
                        item_key: item_key.clone(),
                    },
                },
            );
        }

        for (turn_key, turn) in &self.ai_state_snapshot.turns {
            let Some(diff) = self.ai_state_snapshot.turn_diffs.get(turn_key.as_str()) else {
                continue;
            };
            if diff.trim().is_empty() {
                continue;
            }
            if turn_keys_with_file_change_items.contains(turn_key.as_str()) {
                continue;
            }
            let diff_row_id = format!("turn-diff:{turn_key}");
            base_rows_by_thread
                .entry(turn.thread_id.clone())
                .or_default()
                .push((turn.last_sequence, diff_row_id.clone()));
            rows_by_id.entry(diff_row_id.clone()).or_insert(AiTimelineRow {
                id: diff_row_id,
                thread_id: turn.thread_id.clone(),
                turn_id: turn.id.clone(),
                last_sequence: turn.last_sequence,
                source: AiTimelineRowSource::TurnDiff {
                    turn_key: turn_key.clone(),
                },
            });
        }

        for (turn_key, plan) in &self.ai_state_snapshot.turn_plans {
            let plan_row_id = format!("turn-plan:{turn_key}");
            base_rows_by_thread
                .entry(plan.thread_id.clone())
                .or_default()
                .push((plan.last_sequence, plan_row_id.clone()));
            rows_by_id.entry(plan_row_id.clone()).or_insert(AiTimelineRow {
                id: plan_row_id,
                thread_id: plan.thread_id.clone(),
                turn_id: plan.turn_id.clone(),
                last_sequence: plan.last_sequence,
                source: AiTimelineRowSource::TurnPlan {
                    turn_key: turn_key.clone(),
                },
            });
        }

        let base_row_ids_by_thread = base_rows_by_thread
            .into_iter()
            .map(|(thread_id, mut entries)| {
                entries.sort_by(|left, right| {
                    left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1))
                });
                entries.dedup_by(|left, right| left.1 == right.1);
                let ids = entries
                    .into_iter()
                    .map(|(_, row_id)| row_id)
                    .collect::<Vec<_>>();
                (thread_id, ids)
            })
            .collect::<BTreeMap<_, _>>();

        let mut grouped_row_ids_by_thread = BTreeMap::new();
        let mut groups_by_id = BTreeMap::new();
        let mut parent_by_child_row_id = BTreeMap::new();
        for (thread_id, row_ids) in &base_row_ids_by_thread {
            let (grouped_row_ids, groups, group_parent_by_child_row_id) =
                group_ai_timeline_rows_for_thread(
                    &self.ai_state_snapshot,
                    row_ids.as_slice(),
                    &rows_by_id,
                );
            for group in groups {
                rows_by_id.insert(
                    group.id.clone(),
                    AiTimelineRow {
                        id: group.id.clone(),
                        thread_id: group.thread_id.clone(),
                        turn_id: group.turn_id.clone(),
                        last_sequence: group.last_sequence,
                        source: AiTimelineRowSource::Group {
                            group_id: group.id.clone(),
                        },
                    },
                );
                groups_by_id.insert(group.id.clone(), group);
            }
            parent_by_child_row_id.extend(group_parent_by_child_row_id);
            grouped_row_ids_by_thread.insert(thread_id.clone(), grouped_row_ids);
        }

        self.ai_timeline_row_ids_by_thread = grouped_row_ids_by_thread;
        self.ai_timeline_rows_by_id = rows_by_id;
        self.ai_timeline_groups_by_id = groups_by_id;
        self.ai_timeline_group_parent_by_child_row_id = parent_by_child_row_id;
        self.record_ai_timeline_index_rebuild_timing(rebuild_started_at.elapsed());
    }

    fn refresh_ai_timeline_follow_output_from_scroll(&mut self) {
        let row_count = self.ai_timeline_list_state.item_count();
        let scroll_offset_y = self
            .ai_timeline_list_state
            .scroll_px_offset_for_scrollbar()
            .y
            .as_f32();
        let max_scroll_offset_y = self
            .ai_timeline_list_state
            .max_offset_for_scrollbar()
            .y
            .as_f32();
        self.ai_timeline_follow_output =
            should_follow_timeline_output(row_count, scroll_offset_y, max_scroll_offset_y);
    }

    fn flush_ai_timeline_scroll_request(&mut self) {
        if self.ai_scroll_timeline_to_bottom && self.ai_timeline_list_state.item_count() > 0 {
            self.scroll_ai_timeline_list_to_bottom();
            self.ai_scroll_timeline_to_bottom = false;
        }
    }

    fn scroll_ai_timeline_list_to_bottom(&self) {
        let row_count = self.ai_timeline_list_state.item_count();
        if row_count == 0 {
            return;
        }
        // Use an end-of-list logical offset instead of reveal-item because reveal-item relies on
        // measured row heights; immediately after a reset, rows are unmeasured (height=0).
        self.ai_timeline_list_state.scroll_to(ListOffset {
            item_ix: row_count,
            offset_in_item: px(0.),
        });
    }

    pub(super) fn ai_visible_pending_approvals(&self) -> Vec<AiPendingApproval> {
        self.ai_pending_approvals.clone()
    }

    pub(super) fn ai_visible_pending_user_inputs(&self) -> Vec<AiPendingUserInputRequest> {
        self.ai_pending_user_inputs.clone()
    }

    pub(super) fn ai_load_older_turns_action(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let total_turn_count = self.ai_timeline_turn_ids(thread_id.as_str()).len();
        if total_turn_count == 0 {
            return;
        }
        let current_limit = self
            .ai_timeline_visible_turn_limit_by_thread
            .get(thread_id.as_str())
            .copied()
            .unwrap_or(AI_TIMELINE_DEFAULT_VISIBLE_TURNS.min(total_turn_count));
        if current_limit == usize::MAX {
            return;
        }
        let next_limit = current_limit
            .saturating_add(AI_TIMELINE_TURN_PAGE_SIZE)
            .min(total_turn_count);
        if next_limit == current_limit {
            return;
        }
        self.ai_timeline_visible_turn_limit_by_thread
            .insert(thread_id.clone(), next_limit);
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        if self.ai_selected_thread_id.as_deref() == Some(thread_id.as_str()) {
            self.ai_text_selection = None;
            let visible_row_ids = current_ai_renderable_visible_row_ids(self, thread_id.as_str());
            reset_ai_timeline_list_measurements(self, visible_row_ids.len());
            self.flush_ai_timeline_scroll_request();
        }
        cx.notify();
    }

    pub(super) fn ai_show_full_timeline_action(&mut self, thread_id: String, cx: &mut Context<Self>) {
        let total_turn_count = self.ai_timeline_turn_ids(thread_id.as_str()).len();
        if total_turn_count == 0 {
            return;
        }
        self.ai_timeline_visible_turn_limit_by_thread
            .insert(thread_id.clone(), usize::MAX);
        self.invalidate_ai_visible_frame_state_with_reason("timeline");
        if self.ai_selected_thread_id.as_deref() == Some(thread_id.as_str()) {
            self.ai_text_selection = None;
            let visible_row_ids = current_ai_renderable_visible_row_ids(self, thread_id.as_str());
            reset_ai_timeline_list_measurements(self, visible_row_ids.len());
            self.flush_ai_timeline_scroll_request();
        }
        cx.notify();
    }

    pub(super) fn ai_select_pending_user_input_option_action(
        &mut self,
        request_id: String,
        question_id: String,
        option: String,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.ai_pending_user_input_request(request_id.as_str()) else {
            return;
        };
        let workspace_key = self
            .ai_thread_workspace_root(request.thread_id.as_str())
            .map(|root| root.to_string_lossy().to_string());

        let Some(answers_by_request) =
            self.ai_pending_user_input_answers_mut_for_workspace(workspace_key.as_deref())
        else {
            return;
        };
        let answers = answers_by_request
            .entry(request_id)
            .or_insert_with(|| normalized_user_input_answers(&request, None));
        answers.insert(question_id, vec![option]);
        cx.notify();
    }

    pub(super) fn ai_submit_pending_user_input_action(
        &mut self,
        request_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.ai_pending_user_input_request(request_id.as_str()) else {
            self.set_current_ai_composer_status("User input request no longer exists.", cx);
            cx.notify();
            return;
        };
        let workspace_key = self
            .ai_thread_workspace_root(request.thread_id.as_str())
            .map(|root| root.to_string_lossy().to_string());

        let answers = if self.ai_workspace_key().as_deref() == workspace_key.as_deref() {
            self.ai_pending_user_input_answers
                .get(request_id.as_str())
                .cloned()
        } else {
            workspace_key
                .as_deref()
                .and_then(|workspace_key| self.ai_workspace_states.get(workspace_key))
                .and_then(|state| state.pending_user_input_answers.get(request_id.as_str()).cloned())
        }
        .unwrap_or_else(|| normalized_user_input_answers(&request, None));
        let request_thread_id = request.thread_id.clone();

        if self.send_ai_worker_command_for_workspace(
            workspace_key.as_deref(),
            AiWorkerCommand::SubmitUserInput {
                request_id: request_id.clone(),
                answers,
            },
            true,
            cx,
        ) {
            self.set_ai_composer_status_for_target(
                Some(AiComposerDraftKey::Thread(request_thread_id)),
                format!("Submitted user input for request {request_id}."),
                cx,
            );
            cx.notify();
        }
    }
}

fn ai_turn_keys_with_file_change_items(
    state: &hunk_codex::state::AiState,
) -> std::collections::BTreeSet<String> {
    state
        .items
        .values()
        .filter(|item| item.kind == "fileChange")
        .map(|item| hunk_codex::state::turn_storage_key(item.thread_id.as_str(), item.turn_id.as_str()))
        .collect()
}
