impl DiffViewer {
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
        if let Some((left_source_id, right_source_id)) =
            self.ai_selected_thread_review_compare_selection()
        {
            self.update_review_compare_selection(left_source_id, right_source_id, cx);
        }
        self.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
    }

    pub(crate) fn ai_threads_for_current_workspace(&self) -> Vec<ThreadSummary> {
        sorted_threads(&self.ai_state_snapshot)
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
            .collect()
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
        self.ai_state_snapshot
            .threads
            .get(thread_id)
            .filter(|thread| {
                ai_thread_workspace_matches_current_project(
                    std::path::Path::new(thread.cwd.as_str()),
                    self.workspace_targets.as_slice(),
                    self.project_path.as_deref(),
                    self.repo_root.as_deref(),
                )
            })
            .cloned()
            .or_else(|| {
                self.ai_workspace_states
                    .iter()
                    .filter(|(workspace_key, _)| {
                        ai_thread_workspace_matches_current_project(
                            std::path::Path::new(workspace_key.as_str()),
                            self.workspace_targets.as_slice(),
                            self.project_path.as_deref(),
                            self.repo_root.as_deref(),
                        )
                    })
                    .find_map(|(_, state)| state.state_snapshot.threads.get(thread_id).cloned())
            })
    }

    fn ai_thread_workspace_root(&self, thread_id: &str) -> Option<std::path::PathBuf> {
        self.ai_thread_summary(thread_id)
            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
            .map(|thread| std::path::PathBuf::from(thread.cwd))
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
        merged_ai_visible_threads(
            &self.ai_state_snapshot,
            state_snapshot_workspace_key.as_deref(),
            &self.ai_workspace_states,
            self.workspace_targets.as_slice(),
            self.project_path.as_deref(),
            self.repo_root.as_deref(),
        )
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
        self.ai_timeline_turn_ids_by_thread = timeline_turn_ids_by_thread(&self.ai_state_snapshot);

        let mut base_rows_by_thread = BTreeMap::<String, Vec<(u64, String)>>::new();
        let mut rows_by_id = BTreeMap::<String, AiTimelineRow>::new();
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
            if ai_turn_has_file_change_items(
                &self.ai_state_snapshot,
                turn.thread_id.as_str(),
                turn.id.as_str(),
            ) {
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
            .height
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
        let mut approvals_by_id = BTreeMap::<String, AiPendingApproval>::new();

        for approval in &self.ai_pending_approvals {
            if self.ai_thread_workspace_root(approval.thread_id.as_str()).is_some() {
                approvals_by_id.insert(approval.request_id.clone(), approval.clone());
            }
        }

        for (workspace_key, state) in &self.ai_workspace_states {
            if !ai_thread_workspace_matches_current_project(
                std::path::Path::new(workspace_key.as_str()),
                self.workspace_targets.as_slice(),
                self.project_path.as_deref(),
                self.repo_root.as_deref(),
            ) {
                continue;
            }
            for approval in &state.pending_approvals {
                approvals_by_id
                    .entry(approval.request_id.clone())
                    .or_insert_with(|| approval.clone());
            }
        }

        approvals_by_id.into_values().collect()
    }

    pub(super) fn ai_visible_pending_user_inputs(&self) -> Vec<AiPendingUserInputRequest> {
        let mut requests_by_id = BTreeMap::<String, AiPendingUserInputRequest>::new();

        for request in &self.ai_pending_user_inputs {
            if self.ai_thread_workspace_root(request.thread_id.as_str()).is_some() {
                requests_by_id.insert(request.request_id.clone(), request.clone());
            }
        }

        for (workspace_key, state) in &self.ai_workspace_states {
            if !ai_thread_workspace_matches_current_project(
                std::path::Path::new(workspace_key.as_str()),
                self.workspace_targets.as_slice(),
                self.project_path.as_deref(),
                self.repo_root.as_deref(),
            ) {
                continue;
            }
            for request in &state.pending_user_inputs {
                requests_by_id
                    .entry(request.request_id.clone())
                    .or_insert_with(|| request.clone());
            }
        }

        requests_by_id.into_values().collect()
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
            self.set_current_ai_composer_status("User input request no longer exists.");
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
            );
            cx.notify();
        }
    }
}

fn ai_turn_has_file_change_items(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
    turn_id: &str,
) -> bool {
    state
        .items
        .values()
        .any(|item| item.thread_id == thread_id && item.turn_id == turn_id && item.kind == "fileChange")
}
