const AI_WORKSPACE_STREAMING_REVEAL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(16);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct AiIncrementalStreamingChangeSet {
    changed_item_keys: std::collections::BTreeSet<String>,
}

impl DiffViewer {
    fn try_apply_incremental_ai_streaming_snapshot(
        &mut self,
        snapshot: AiSnapshot,
        previous_selected_thread: Option<&str>,
        previous_selected_thread_sequence: u64,
        cx: &mut Context<Self>,
    ) -> Option<AiSnapshot> {
        let Some(selected_thread_id) = previous_selected_thread else {
            return Some(snapshot);
        };
        if self.workspace_view_mode != WorkspaceViewMode::Ai
            || self.ai_visible_frame_state.is_none()
            || self.ai_workspace_session.is_none()
            || self.ai_new_thread_draft_active
            || self.ai_pending_new_thread_selection
            || self.ai_pending_thread_start.is_some()
            || snapshot.active_thread_id.as_deref() != self.ai_selected_thread_id.as_deref()
            || !ai_snapshot_supports_incremental_streaming(self, &snapshot)
        {
            return Some(snapshot);
        }

        let Some(change_set) =
            ai_incremental_streaming_change_set(&self.ai_state_snapshot, &snapshot.state)
        else {
            return Some(snapshot);
        };
        if change_set.changed_item_keys.is_empty() {
            return Some(snapshot);
        }

        let Some(visible_row_ids) = self.ai_visible_frame_state.as_ref().and_then(|state| {
            (state.selected_thread_id.as_deref() == Some(selected_thread_id))
                .then(|| state.timeline_visible_row_ids.iter().cloned().collect::<Vec<_>>())
        }) else {
            return Some(snapshot);
        };
        let threads_changed = ai_snapshot_threads_changed(&self.ai_state_snapshot, &snapshot.state);

        let AiSnapshot { state, .. } = snapshot;
        self.ai_state_snapshot = state;
        self.rebuild_ai_timeline_indexes();
        if threads_changed {
            self.rebuild_ai_thread_sidebar_state();
        }

        let next_visible_row_ids = self.ai_timeline_visible_rows_for_thread(selected_thread_id).3;
        if next_visible_row_ids != visible_row_ids {
            self.invalidate_ai_visible_frame_state_with_reason("runtime");
            if self.ai_timeline_follow_output
                && should_scroll_timeline_to_bottom_on_new_activity(
                    thread_latest_timeline_sequence(&self.ai_state_snapshot, selected_thread_id),
                    previous_selected_thread_sequence,
                    self.ai_timeline_follow_output,
                )
            {
                self.ai_scroll_timeline_to_bottom = true;
            }
            self.flush_ai_timeline_scroll_request();
            cx.notify();
            return None;
        }

        let changed_container_row_ids = change_set
            .changed_item_keys
            .iter()
            .filter_map(|item_key| {
                let row_id = format!("item:{item_key}");
                self.ai_timeline_container_row_id(row_id.as_str())
            })
            .collect::<std::collections::BTreeSet<_>>();

        let visible_row_id_set = next_visible_row_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let row_updates = changed_container_row_ids
            .iter()
            .filter(|row_id| visible_row_id_set.contains(row_id.as_str()))
            .filter_map(|row_id| {
                let source_row = self.ai_workspace_source_row(row_id.as_str())?;
                let blocks = self.ai_workspace_blocks_for_row(row_id.as_str());
                Some((source_row, blocks))
            })
            .collect::<Vec<_>>();

        let mut fallback_to_full_rebuild = false;
        let mut revealed_immediately = false;
        if let Some(session) = self.ai_workspace_session.as_mut() {
            for (source_row, blocks) in &row_updates {
                if !session.replace_source_row(source_row.clone(), blocks.clone(), true) {
                    fallback_to_full_rebuild = true;
                    break;
                }
            }
            if !fallback_to_full_rebuild {
                revealed_immediately = session.reveal_pending_streaming_preview_step();
            }
        }

        if fallback_to_full_rebuild {
            self.rebuild_ai_timeline_indexes();
            self.invalidate_ai_visible_frame_state_with_reason("runtime");
            cx.notify();
            return None;
        }

        let changed_visible_row_ids = changed_container_row_ids
            .iter()
            .filter(|row_id| visible_row_id_set.contains(row_id.as_str()))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        self.ai_clear_text_selection_for_rows(&changed_visible_row_ids, cx);

        if self.ai_timeline_follow_output
            && should_scroll_timeline_to_bottom_on_new_activity(
                thread_latest_timeline_sequence(&self.ai_state_snapshot, selected_thread_id),
                previous_selected_thread_sequence,
                self.ai_timeline_follow_output,
            )
        {
            self.ai_scroll_timeline_to_bottom = true;
        }
        if revealed_immediately {
            self.ai_scroll_timeline_to_bottom |= self.ai_timeline_follow_output;
        }

        self.ensure_ai_workspace_streaming_reveal_task(cx);
        self.flush_ai_timeline_scroll_request();
        cx.notify();
        None
    }

    fn ensure_ai_workspace_streaming_reveal_task(&mut self, cx: &mut Context<Self>) {
        let has_pending_preview = self
            .ai_workspace_session
            .as_ref()
            .is_some_and(ai_workspace_session::AiWorkspaceSession::has_pending_streaming_preview);
        if self.ai_workspace_streaming_reveal_active || !has_pending_preview {
            return;
        }

        self.ai_workspace_streaming_reveal_active = true;
        self.ai_workspace_streaming_reveal_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(AI_WORKSPACE_STREAMING_REVEAL_INTERVAL)
                    .await;

                let Some(this) = this.upgrade() else {
                    return;
                };

                let mut keep_running = false;
                this.update(cx, |this, cx| {
                    let (revealed, has_pending_preview) = this
                        .ai_workspace_session
                        .as_mut()
                        .map(|session| {
                            (
                                session.reveal_pending_streaming_preview_step(),
                                session.has_pending_streaming_preview(),
                            )
                        })
                        .unwrap_or((false, false));

                    keep_running = has_pending_preview;
                    if !keep_running {
                        this.ai_workspace_streaming_reveal_active = false;
                    }

                    if revealed {
                        if this.ai_timeline_follow_output && this.ai_selected_thread_id.is_some() {
                            this.ai_scroll_timeline_to_bottom = true;
                            this.flush_ai_timeline_scroll_request();
                        }
                        cx.notify();
                    }
                });

                if !keep_running {
                    return;
                }
            }
        });
    }
}

fn ai_snapshot_supports_incremental_streaming(this: &DiffViewer, snapshot: &AiSnapshot) -> bool {
    this.ai_pending_approvals == snapshot.pending_approvals
        && this.ai_pending_user_inputs == snapshot.pending_user_inputs
        && this.ai_account == snapshot.account
        && this.ai_requires_openai_auth == snapshot.requires_openai_auth
        && this.ai_pending_chatgpt_login_id == snapshot.pending_chatgpt_login_id
        && this.ai_pending_chatgpt_auth_url == snapshot.pending_chatgpt_auth_url
        && this.ai_rate_limits == snapshot.rate_limits
        && this.ai_models == snapshot.models
        && this.ai_experimental_features == snapshot.experimental_features
        && this.ai_collaboration_modes == snapshot.collaboration_modes
        && this.ai_skills == snapshot.skills
        && this.ai_include_hidden_models == snapshot.include_hidden_models
        && this.ai_mad_max_mode == snapshot.mad_max_mode
}

fn ai_incremental_streaming_change_set(
    previous_state: &hunk_codex::state::AiState,
    next_state: &hunk_codex::state::AiState,
) -> Option<AiIncrementalStreamingChangeSet> {
    if previous_state.thread_token_usage != next_state.thread_token_usage
        || previous_state.turn_diffs != next_state.turn_diffs
        || previous_state.turn_plans != next_state.turn_plans
        || previous_state.server_requests != next_state.server_requests
        || previous_state.active_thread_by_cwd != next_state.active_thread_by_cwd
        || previous_state.threads.len() != next_state.threads.len()
        || previous_state.turns.len() != next_state.turns.len()
        || previous_state.items.len() != next_state.items.len()
    {
        return None;
    }

    for (thread_id, previous_thread) in &previous_state.threads {
        let next_thread = next_state.threads.get(thread_id.as_str())?;
        if !ai_thread_supports_incremental_streaming(previous_thread, next_thread) {
            return None;
        }
    }

    for (turn_key, previous_turn) in &previous_state.turns {
        let next_turn = next_state.turns.get(turn_key.as_str())?;
        if !ai_turn_supports_incremental_streaming(previous_turn, next_turn) {
            return None;
        }
    }

    let mut changed_item_keys = std::collections::BTreeSet::new();
    for (item_key, previous_item) in &previous_state.items {
        let next_item = next_state.items.get(item_key.as_str())?;
        let changed = ai_item_supports_incremental_streaming(previous_item, next_item)?;
        if changed {
            changed_item_keys.insert(item_key.clone());
        }
    }

    Some(AiIncrementalStreamingChangeSet { changed_item_keys })
}

fn ai_thread_supports_incremental_streaming(
    previous_thread: &hunk_codex::state::ThreadSummary,
    next_thread: &hunk_codex::state::ThreadSummary,
) -> bool {
    previous_thread.id == next_thread.id
        && previous_thread.cwd == next_thread.cwd
        && previous_thread.title == next_thread.title
        && previous_thread.status == next_thread.status
        && previous_thread.created_at == next_thread.created_at
}

fn ai_turn_supports_incremental_streaming(
    previous_turn: &hunk_codex::state::TurnSummary,
    next_turn: &hunk_codex::state::TurnSummary,
) -> bool {
    previous_turn.id == next_turn.id
        && previous_turn.thread_id == next_turn.thread_id
        && previous_turn.collaboration_mode == next_turn.collaboration_mode
        && previous_turn.status == next_turn.status
}

fn ai_item_supports_incremental_streaming(
    previous_item: &hunk_codex::state::ItemSummary,
    next_item: &hunk_codex::state::ItemSummary,
) -> Option<bool> {
    if previous_item.id != next_item.id
        || previous_item.thread_id != next_item.thread_id
        || previous_item.turn_id != next_item.turn_id
        || previous_item.kind != next_item.kind
        || ai_timeline_item_is_renderable_for_layout(previous_item)
            != ai_timeline_item_is_renderable_for_layout(next_item)
    {
        return None;
    }

    Some(
        previous_item.status != next_item.status
            || previous_item.content != next_item.content
            || previous_item.display_metadata != next_item.display_metadata
            || previous_item.last_sequence != next_item.last_sequence,
    )
}
