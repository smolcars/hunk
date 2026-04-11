#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AiFollowupPromptKeystrokeAction {
    SelectPrevious,
    SelectNext,
    Accept,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AiComposerModeTarget {
    Code,
    Plan,
    Review,
}

pub(super) fn ai_followup_prompt_action_for_keystroke(
    keystroke: &gpui::Keystroke,
) -> Option<AiFollowupPromptKeystrokeAction> {
    if keystroke.modifiers.modified() {
        return None;
    }

    match keystroke.key.as_str() {
        "left" | "up" => Some(AiFollowupPromptKeystrokeAction::SelectPrevious),
        "right" | "down" => Some(AiFollowupPromptKeystrokeAction::SelectNext),
        "enter" => Some(AiFollowupPromptKeystrokeAction::Accept),
        _ => None,
    }
}

pub(super) fn ai_cycle_composer_mode_target(
    review_mode_active: bool,
    collaboration_mode: AiCollaborationModeSelection,
) -> AiComposerModeTarget {
    if review_mode_active {
        return AiComposerModeTarget::Code;
    }

    match collaboration_mode {
        AiCollaborationModeSelection::Default => AiComposerModeTarget::Plan,
        AiCollaborationModeSelection::Plan => AiComposerModeTarget::Review,
    }
}

fn latest_ai_plan_for_thread(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
) -> Option<AiFollowupPrompt> {
    let turn_plan_prompt = state
        .turn_plans
        .values()
        .filter(|plan| plan.thread_id == thread_id)
        .max_by_key(|plan| plan.last_sequence)
        .map(|plan| AiFollowupPrompt {
            kind: AiFollowupPromptKind::Plan,
            source_sequence: plan.last_sequence,
        });
    let plan_item_prompt = state
        .items
        .values()
        .filter(|item| {
            item.thread_id == thread_id
                && item.kind == "plan"
                && (!item.content.trim().is_empty()
                    || item
                        .display_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.summary.as_deref())
                        .is_some_and(|summary| !summary.trim().is_empty()))
        })
        .max_by_key(|item| item.last_sequence)
        .map(|item| AiFollowupPrompt {
            kind: AiFollowupPromptKind::Plan,
            source_sequence: item.last_sequence,
        });

    match (turn_plan_prompt, plan_item_prompt) {
        (Some(turn_plan), Some(plan_item)) => Some(if turn_plan.source_sequence
            >= plan_item.source_sequence
        {
            turn_plan
        } else {
            plan_item
        }),
        (Some(turn_plan), None) => Some(turn_plan),
        (None, Some(plan_item)) => Some(plan_item),
        (None, None) => None,
    }
}

fn latest_ai_review_for_thread(
    state: &hunk_codex::state::AiState,
    thread_id: &str,
) -> Option<u64> {
    state
        .items
        .values()
        .filter(|item| {
            item.thread_id == thread_id
                && item.kind == "exitedReviewMode"
                && item.status == hunk_codex::state::ItemStatus::Completed
        })
        .max_by_key(|item| item.last_sequence)
        .map(|item| item.last_sequence)
}

fn seed_ai_followup_prompt_state_for_thread<'a>(
    prompt_states: &'a mut BTreeMap<String, AiThreadFollowupPromptState>,
    snapshot: &hunk_codex::state::AiState,
    thread_id: &str,
) -> &'a mut AiThreadFollowupPromptState {
    prompt_states
        .entry(thread_id.to_string())
        .or_insert_with(|| AiThreadFollowupPromptState {
            plan_acknowledged_sequence: latest_ai_plan_for_thread(snapshot, thread_id)
                .map(|prompt| prompt.source_sequence)
                .unwrap_or(0),
            ..AiThreadFollowupPromptState::default()
        })
}

pub(super) fn ai_followup_prompt_for_thread(
    snapshot: &hunk_codex::state::AiState,
    thread_id: &str,
    collaboration_mode: AiCollaborationModeSelection,
    prompt_state: AiThreadFollowupPromptState,
) -> Option<AiFollowupPrompt> {
    if collaboration_mode != AiCollaborationModeSelection::Plan
        || ai_thread_has_in_progress_turn(snapshot, thread_id)
    {
        return None;
    }

    latest_ai_plan_for_thread(snapshot, thread_id)
        .filter(|prompt| prompt.source_sequence > prompt_state.plan_acknowledged_sequence)
}

fn ai_visible_followup_prompt_for_selected_thread(
    snapshot: &hunk_codex::state::AiState,
    selected_thread_id: Option<&str>,
    collaboration_mode: AiCollaborationModeSelection,
    prompt_states: &BTreeMap<String, AiThreadFollowupPromptState>,
) -> Option<AiFollowupPrompt> {
    let thread_id = selected_thread_id?;
    let prompt_state = prompt_states.get(thread_id).copied().unwrap_or_default();
    ai_followup_prompt_for_thread(snapshot, thread_id, collaboration_mode, prompt_state)
}

fn ai_visible_followup_prompt_action_for_selected_thread(
    prompt_states: &BTreeMap<String, AiThreadFollowupPromptState>,
    selected_thread_id: Option<&str>,
) -> AiFollowupPromptAction {
    selected_thread_id
        .and_then(|thread_id| prompt_states.get(thread_id))
        .map(|state| state.selected_action)
        .unwrap_or(AiFollowupPromptAction::Primary)
}

pub(super) fn prune_ai_followup_prompt_state(
    prompt_states: &mut BTreeMap<String, AiThreadFollowupPromptState>,
    snapshot: &hunk_codex::state::AiState,
) {
    prompt_states.retain(|thread_id, _| snapshot.threads.contains_key(thread_id));
}

pub(super) fn sync_ai_review_mode_threads_after_snapshot(
    review_mode_thread_ids: &mut BTreeSet<String>,
    snapshot: &hunk_codex::state::AiState,
) {
    let thread_ids = review_mode_thread_ids.iter().cloned().collect::<Vec<_>>();
    for thread_id in thread_ids {
        if !snapshot.threads.contains_key(thread_id.as_str()) {
            review_mode_thread_ids.remove(thread_id.as_str());
            continue;
        }
        let Some(review_sequence) = latest_ai_review_for_thread(snapshot, thread_id.as_str()) else {
            continue;
        };
        if review_sequence > 0 && !ai_thread_has_in_progress_turn(snapshot, thread_id.as_str()) {
            review_mode_thread_ids.remove(thread_id.as_str());
        }
    }
}

pub(super) fn sync_ai_followup_prompt_ui_state(
    prompt_states: &mut BTreeMap<String, AiThreadFollowupPromptState>,
    snapshot: &hunk_codex::state::AiState,
    thread_id: Option<&str>,
    collaboration_mode: AiCollaborationModeSelection,
) {
    let Some(thread_id) = thread_id else {
        return;
    };
    let prompt_state = seed_ai_followup_prompt_state_for_thread(prompt_states, snapshot, thread_id);
    let next_prompt =
        ai_followup_prompt_for_thread(snapshot, thread_id, collaboration_mode, *prompt_state);
    if let Some(prompt) = next_prompt {
        if prompt_state.prompt_source_sequence != prompt.source_sequence {
            prompt_state.prompt_source_sequence = prompt.source_sequence;
            prompt_state.selected_action = AiFollowupPromptAction::Primary;
        }
    } else {
        prompt_state.prompt_source_sequence = 0;
        prompt_state.selected_action = AiFollowupPromptAction::Primary;
    }
}

impl DiffViewer {
    pub(super) fn current_ai_followup_prompt_for_selected_thread(
        &self,
        selected_thread_id: Option<&str>,
    ) -> Option<AiFollowupPrompt> {
        ai_visible_followup_prompt_for_selected_thread(
            &self.ai_state_snapshot,
            selected_thread_id,
            self.ai_selected_collaboration_mode,
            &self.ai_followup_prompt_state_by_thread,
        )
    }

    pub(super) fn current_ai_followup_prompt_action_for_selected_thread(
        &self,
        selected_thread_id: Option<&str>,
    ) -> AiFollowupPromptAction {
        ai_visible_followup_prompt_action_for_selected_thread(
            &self.ai_followup_prompt_state_by_thread,
            selected_thread_id,
        )
    }

    pub(super) fn sync_ai_followup_prompt_state_for_selected_thread(
        &mut self,
        selected_thread_id: Option<&str>,
    ) {
        sync_ai_followup_prompt_ui_state(
            &mut self.ai_followup_prompt_state_by_thread,
            &self.ai_state_snapshot,
            selected_thread_id,
            self.ai_selected_collaboration_mode,
        );
    }

    fn acknowledge_current_ai_followup_prompt_kind(
        &mut self,
        kind: AiFollowupPromptKind,
    ) -> bool {
        let Some(thread_id) = self.current_ai_thread_id() else {
            return false;
        };
        let prompt_state = seed_ai_followup_prompt_state_for_thread(
            &mut self.ai_followup_prompt_state_by_thread,
            &self.ai_state_snapshot,
            thread_id.as_str(),
        );
        match kind {
            AiFollowupPromptKind::Plan => {
                prompt_state.plan_acknowledged_sequence =
                    latest_ai_plan_for_thread(&self.ai_state_snapshot, thread_id.as_str())
                        .map(|prompt| prompt.source_sequence)
                        .unwrap_or(prompt_state.plan_acknowledged_sequence);
            }
        }
        prompt_state.prompt_source_sequence = 0;
        prompt_state.selected_action = AiFollowupPromptAction::Primary;
        true
    }

    fn set_current_ai_followup_prompt_action(
        &mut self,
        selected_action: AiFollowupPromptAction,
    ) -> bool {
        let Some(thread_id) = self.current_ai_thread_id() else {
            return false;
        };
        let Some(prompt) =
            self.current_ai_followup_prompt_for_selected_thread(Some(thread_id.as_str()))
        else {
            return false;
        };
        let prompt_state = seed_ai_followup_prompt_state_for_thread(
            &mut self.ai_followup_prompt_state_by_thread,
            &self.ai_state_snapshot,
            thread_id.as_str(),
        );
        let changed = prompt_state.selected_action != selected_action
            || prompt_state.prompt_source_sequence != prompt.source_sequence;
        prompt_state.prompt_source_sequence = prompt.source_sequence;
        prompt_state.selected_action = selected_action;
        changed
    }

    fn ai_turn_session_overrides_for_collaboration_mode(
        &self,
        collaboration_mode: AiCollaborationModeSelection,
    ) -> AiTurnSessionOverrides {
        let mut model = self.ai_selected_model.clone();
        let mut effort = self.ai_selected_effort.clone();

        if let Some(mask) = ai_collaboration_mode_mask(&self.ai_collaboration_modes, collaboration_mode)
        {
            if let Some(mask_model) = mask.model.as_ref() {
                model = Some(mask_model.clone());
            }
            if let Some(reasoning_effort) = mask.reasoning_effort.unwrap_or(None) {
                effort = Some(reasoning_effort_key(&reasoning_effort));
            }
        }

        let (model, effort) = normalized_ai_session_selection(self.ai_models.as_slice(), model, effort);
        let effort = model.as_ref().and_then(|model_id| {
            effort
                .clone()
                .filter(|effort_key| self.model_supports_effort(model_id.as_str(), effort_key.as_str()))
        });

        AiTurnSessionOverrides {
            model,
            effort,
            collaboration_mode,
            service_tier: self.ai_selected_service_tier,
        }
    }

    fn send_ai_followup_prompt_to_current_thread(
        &mut self,
        prompt: impl Into<String>,
        session_overrides: AiTurnSessionOverrides,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(thread_id) = self.current_ai_thread_id() else {
            return false;
        };
        if self.ai_command_tx.is_none() {
            self.ensure_ai_runtime_started(cx);
        }
        if self.ai_command_tx.is_none()
            || ai_prompt_send_waiting_on_connection(
                self.ai_connection_state,
                self.ai_bootstrap_loading,
            )
        {
            self.set_current_ai_composer_status("Cannot send until Codex finishes connecting.", cx);
            cx.notify();
            return false;
        }

        let sent = self.send_ai_worker_command(
            AiWorkerCommand::SendPrompt {
                thread_id,
                prompt: Some(prompt.into()),
                local_image_paths: Vec::new(),
                selected_skills: Vec::new(),
                skill_bindings: Vec::new(),
                session_overrides,
            },
            cx,
        );
        if sent {
            self.clear_current_ai_composer_status();
        }
        sent
    }

    pub(super) fn accept_current_ai_followup_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let current_thread_id = self.current_ai_thread_id();
        let Some(prompt) =
            self.current_ai_followup_prompt_for_selected_thread(current_thread_id.as_deref())
        else {
            return false;
        };

        let sent = self.send_ai_followup_prompt_to_current_thread(
            "Implement the latest approved plan from this thread.",
            self.ai_turn_session_overrides_for_collaboration_mode(
                AiCollaborationModeSelection::Default,
            ),
            cx,
        );
        if !sent {
            return false;
        }

        self.ai_select_collaboration_mode_action(AiCollaborationModeSelection::Default, cx);
        self.acknowledge_current_ai_followup_prompt_kind(prompt.kind);
        self.clear_ai_composer_input(window, cx);
        self.ai_timeline_follow_output = true;
        self.ai_scroll_timeline_to_bottom = true;
        self.flush_ai_timeline_scroll_request();
        self.sync_ai_followup_prompt_state_for_selected_thread(current_thread_id.as_deref());
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        cx.notify();
        sent
    }

    pub(super) fn prepare_custom_followup_for_current_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let current_thread_id = self.current_ai_thread_id();
        let Some(prompt) =
            self.current_ai_followup_prompt_for_selected_thread(current_thread_id.as_deref())
        else {
            return false;
        };
        self.acknowledge_current_ai_followup_prompt_kind(prompt.kind);
        self.sync_ai_followup_prompt_state_for_selected_thread(current_thread_id.as_deref());
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        self.focus_ai_composer_input(window, cx);
        cx.notify();
        true
    }

    pub(super) fn ai_handle_followup_prompt_keystroke(
        &mut self,
        action: AiFollowupPromptKeystrokeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return false;
        }

        let composer_focus_handle =
            gpui::Focusable::focus_handle(self.ai_composer_input_state.read(cx), cx);
        let current_thread_id = self.current_ai_thread_id();
        if !composer_focus_handle.is_focused(window)
            || self
                .current_ai_followup_prompt_for_selected_thread(current_thread_id.as_deref())
                .is_none()
        {
            return false;
        }

        window.prevent_default();
        cx.stop_propagation();

        match action {
            AiFollowupPromptKeystrokeAction::SelectPrevious => {
                let changed =
                    self.set_current_ai_followup_prompt_action(AiFollowupPromptAction::Primary);
                if changed {
                    self.invalidate_ai_visible_frame_state_with_reason("thread");
                    cx.notify();
                }
                true
            }
            AiFollowupPromptKeystrokeAction::SelectNext => {
                let changed =
                    self.set_current_ai_followup_prompt_action(AiFollowupPromptAction::Secondary);
                if changed {
                    self.invalidate_ai_visible_frame_state_with_reason("thread");
                    cx.notify();
                }
                true
            }
            AiFollowupPromptKeystrokeAction::Accept => {
                match self.current_ai_followup_prompt_action_for_selected_thread(
                    current_thread_id.as_deref(),
                ) {
                    AiFollowupPromptAction::Primary => {
                        self.accept_current_ai_followup_prompt(window, cx)
                    }
                    AiFollowupPromptAction::Secondary => {
                        self.prepare_custom_followup_for_current_prompt(window, cx)
                    }
                }
            }
        }
    }

    pub(super) fn ai_cycle_composer_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_ai_workspace_kind() == AiWorkspaceKind::Chats {
            return;
        }
        match ai_cycle_composer_mode_target(
            self.ai_review_mode_active,
            self.ai_selected_collaboration_mode,
        ) {
            AiComposerModeTarget::Code => {
                self.ai_select_collaboration_mode_action(AiCollaborationModeSelection::Default, cx);
            }
            AiComposerModeTarget::Plan => {
                self.ai_select_collaboration_mode_action(AiCollaborationModeSelection::Plan, cx);
            }
            AiComposerModeTarget::Review => {
                self.ai_select_review_mode_action(cx);
            }
        }
        let current_thread_id = self.current_ai_thread_id();
        self.sync_ai_followup_prompt_state_for_selected_thread(current_thread_id.as_deref());
        self.invalidate_ai_visible_frame_state_with_reason("thread");
        self.focus_ai_composer_input(window, cx);
        cx.notify();
    }
}
