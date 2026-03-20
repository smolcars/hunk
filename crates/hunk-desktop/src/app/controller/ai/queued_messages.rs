fn ai_queued_message_row_id(queued: &AiQueuedUserMessage, queued_ix: usize) -> String {
    format!(
        "queued-message\u{1f}{}\u{1f}{}\u{1f}{queued_ix}",
        queued.thread_id,
        queued.prompt.len()
    )
}

fn ai_thread_has_in_progress_turn(state: &hunk_codex::state::AiState, thread_id: &str) -> bool {
    state
        .turns
        .values()
        .any(|turn| turn.thread_id == thread_id && turn.status == TurnStatus::InProgress)
}

fn ai_thread_accepts_queued_messages(status: ThreadLifecycleStatus) -> bool {
    matches!(
        status,
        ThreadLifecycleStatus::Active | ThreadLifecycleStatus::Idle
    )
}

fn ai_queued_message_matching_sequence(
    state: &hunk_codex::state::AiState,
    queued: &AiQueuedUserMessage,
    min_sequence: u64,
) -> Option<u64> {
    let expected_content =
        ai_pending_steer_seed_content(queued.prompt.as_str(), queued.local_images.as_slice())?;

    state
        .items
        .values()
        .filter(|item| {
            item.thread_id == queued.thread_id
                && item.kind == "userMessage"
                && item.content == expected_content
                && item.last_sequence > min_sequence
        })
        .map(|item| item.last_sequence)
        .min()
}

fn reconcile_ai_queued_messages(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
    state: &hunk_codex::state::AiState,
) {
    if queued_messages.is_empty() {
        return;
    }

    let mut matched_sequence_by_thread = BTreeMap::<String, u64>::new();
    let mut blocked_threads = BTreeSet::<String>::new();
    let mut remaining = Vec::with_capacity(queued_messages.len());

    for queued in queued_messages.drain(..) {
        let AiQueuedUserMessageStatus::PendingConfirmation {
            accepted_after_sequence,
        } = queued.status
        else {
            remaining.push(queued);
            continue;
        };

        if blocked_threads.contains(queued.thread_id.as_str()) {
            remaining.push(queued);
            continue;
        }

        let min_sequence = matched_sequence_by_thread
            .get(queued.thread_id.as_str())
            .copied()
            .unwrap_or(accepted_after_sequence);

        if let Some(sequence) = ai_queued_message_matching_sequence(state, &queued, min_sequence) {
            matched_sequence_by_thread.insert(queued.thread_id.clone(), sequence);
        } else {
            blocked_threads.insert(queued.thread_id.clone());
            remaining.push(queued);
        }
    }

    *queued_messages = remaining;
}

fn take_last_editable_ai_queued_message_for_thread(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
    thread_id: &str,
) -> Option<AiQueuedUserMessage> {
    let queued_ix = queued_messages.iter().rposition(|queued| {
        queued.thread_id == thread_id
            && matches!(queued.status, AiQueuedUserMessageStatus::Queued)
    })?;
    Some(queued_messages.remove(queued_ix))
}

fn take_all_ai_queued_messages(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
) -> Vec<AiQueuedUserMessage> {
    std::mem::take(queued_messages)
}

fn take_interrupted_ai_queued_messages(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
    interrupt_restore_thread_ids: &mut BTreeSet<String>,
    state: &hunk_codex::state::AiState,
) -> Vec<AiQueuedUserMessage> {
    if interrupt_restore_thread_ids.is_empty() {
        return Vec::new();
    }

    let restorable_thread_ids = interrupt_restore_thread_ids
        .iter()
        .filter(|thread_id| !ai_thread_has_in_progress_turn(state, thread_id.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    if restorable_thread_ids.is_empty() {
        return Vec::new();
    }

    interrupt_restore_thread_ids.retain(|thread_id| !restorable_thread_ids.contains(thread_id));
    if queued_messages.is_empty() {
        return Vec::new();
    }

    let mut restorable = Vec::new();
    let mut remaining = Vec::with_capacity(queued_messages.len());
    for queued in queued_messages.drain(..) {
        if restorable_thread_ids.contains(queued.thread_id.as_str()) {
            restorable.push(queued);
        } else {
            remaining.push(queued);
        }
    }
    *queued_messages = remaining;
    restorable
}

fn take_restorable_ai_queued_messages(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
    state: &hunk_codex::state::AiState,
) -> Vec<AiQueuedUserMessage> {
    if queued_messages.is_empty() {
        return Vec::new();
    }

    let mut restorable = Vec::new();
    let mut remaining = Vec::with_capacity(queued_messages.len());
    for queued in queued_messages.drain(..) {
        if matches!(
            queued.status,
            AiQueuedUserMessageStatus::PendingConfirmation { .. }
        ) && !ai_thread_has_in_progress_turn(state, queued.thread_id.as_str())
        {
            restorable.push(queued);
        } else {
            remaining.push(queued);
        }
    }
    *queued_messages = remaining;
    restorable
}

fn take_unavailable_ai_queued_messages(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
    interrupt_restore_thread_ids: &mut BTreeSet<String>,
    state: &hunk_codex::state::AiState,
) -> Vec<AiQueuedUserMessage> {
    if queued_messages.is_empty() {
        return Vec::new();
    }

    let unavailable_thread_ids = queued_messages
        .iter()
        .filter_map(|queued| {
            state
                .threads
                .get(queued.thread_id.as_str())
                .filter(|thread| !ai_thread_accepts_queued_messages(thread.status))
                .map(|_| queued.thread_id.clone())
        })
        .collect::<BTreeSet<_>>();
    if unavailable_thread_ids.is_empty() {
        return Vec::new();
    }

    interrupt_restore_thread_ids.retain(|thread_id| !unavailable_thread_ids.contains(thread_id));
    let mut restorable = Vec::new();
    let mut remaining = Vec::with_capacity(queued_messages.len());
    for queued in queued_messages.drain(..) {
        if unavailable_thread_ids.contains(queued.thread_id.as_str()) {
            restorable.push(queued);
        } else {
            remaining.push(queued);
        }
    }
    *queued_messages = remaining;
    restorable
}

fn reconcile_ai_queued_messages_after_snapshot(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
    interrupt_restore_thread_ids: &mut BTreeSet<String>,
    state: &hunk_codex::state::AiState,
) -> Vec<AiQueuedUserMessage> {
    reconcile_ai_queued_messages(queued_messages, state);

    let mut restorable = take_restorable_ai_queued_messages(queued_messages, state);
    restorable.extend(take_interrupted_ai_queued_messages(
        queued_messages,
        interrupt_restore_thread_ids,
        state,
    ));
    restorable.extend(take_unavailable_ai_queued_messages(
        queued_messages,
        interrupt_restore_thread_ids,
        state,
    ));
    restorable
}

fn ready_ai_queued_message_thread_ids(
    queued_messages: &[AiQueuedUserMessage],
    interrupt_restore_thread_ids: &BTreeSet<String>,
    state: &hunk_codex::state::AiState,
) -> Vec<String> {
    let mut thread_ids = Vec::new();
    let mut blocked_threads = BTreeSet::new();

    for queued in queued_messages {
        if blocked_threads.contains(queued.thread_id.as_str()) {
            continue;
        }
        if interrupt_restore_thread_ids.contains(queued.thread_id.as_str()) {
            blocked_threads.insert(queued.thread_id.clone());
            continue;
        }
        let Some(thread) = state.threads.get(queued.thread_id.as_str()) else {
            blocked_threads.insert(queued.thread_id.clone());
            continue;
        };
        if !ai_thread_accepts_queued_messages(thread.status) {
            blocked_threads.insert(queued.thread_id.clone());
            continue;
        }
        if ai_thread_has_in_progress_turn(state, queued.thread_id.as_str()) {
            blocked_threads.insert(queued.thread_id.clone());
            continue;
        }
        if !matches!(queued.status, AiQueuedUserMessageStatus::Queued) {
            blocked_threads.insert(queued.thread_id.clone());
            continue;
        }
        blocked_threads.insert(queued.thread_id.clone());
        thread_ids.push(queued.thread_id.clone());
    }

    thread_ids
}

fn mark_next_ai_queued_message_pending_confirmation(
    queued_messages: &mut [AiQueuedUserMessage],
    thread_id: &str,
    accepted_after_sequence: u64,
) -> Option<(usize, AiQueuedUserMessage)> {
    let queued_ix = queued_messages.iter().position(|queued| {
        queued.thread_id == thread_id
            && matches!(queued.status, AiQueuedUserMessageStatus::Queued)
    })?;
    queued_messages[queued_ix].status = AiQueuedUserMessageStatus::PendingConfirmation {
        accepted_after_sequence,
    };
    Some((queued_ix, queued_messages[queued_ix].clone()))
}

fn reset_ai_queued_message_to_queued(
    queued_messages: &mut [AiQueuedUserMessage],
    queued_ix: usize,
) {
    let Some(queued) = queued_messages.get_mut(queued_ix) else {
        return;
    };
    queued.status = AiQueuedUserMessageStatus::Queued;
}

impl DiffViewer {
    pub(crate) fn ai_queued_message_row_ids_for_thread(&self, thread_id: &str) -> Vec<String> {
        self.ai_queued_messages
            .iter()
            .enumerate()
            .filter(|(_, queued)| queued.thread_id == thread_id)
            .map(|(queued_ix, queued)| ai_queued_message_row_id(queued, queued_ix))
            .collect()
    }

    pub(crate) fn ai_queued_message_for_row_id(&self, row_id: &str) -> Option<AiQueuedUserMessage> {
        self.ai_queued_messages
            .iter()
            .enumerate()
            .find_map(|(queued_ix, queued)| {
                (ai_queued_message_row_id(queued, queued_ix) == row_id).then(|| queued.clone())
            })
    }

    fn restore_ai_queued_messages_to_drafts(
        &mut self,
        queued_messages: Vec<AiQueuedUserMessage>,
    ) -> BTreeSet<AiComposerDraftKey> {
        let mut touched = BTreeSet::new();

        for queued in queued_messages {
            let target_key = AiComposerDraftKey::Thread(queued.thread_id.clone());
            let draft = self.ai_composer_drafts.entry(target_key.clone()).or_default();
            merge_restored_ai_prompt(&mut draft.prompt, queued.prompt.as_str());
            for image_path in queued.local_images {
                if !draft.local_images.contains(&image_path) {
                    draft.local_images.push(image_path);
                }
            }
            if draft.prompt == queued.prompt {
                draft.skill_bindings = queued.skill_bindings;
            }
            touched.insert(target_key);
        }

        touched
    }

    fn restore_all_visible_ai_queued_messages_to_drafts(&mut self) -> BTreeSet<AiComposerDraftKey> {
        self.ai_interrupt_restore_queued_thread_ids.clear();
        let queued_messages = take_all_ai_queued_messages(&mut self.ai_queued_messages);
        self.restore_ai_queued_messages_to_drafts(queued_messages)
    }

    fn maybe_restore_interrupted_ai_queued_messages_to_drafts(&mut self) -> BTreeSet<AiComposerDraftKey> {
        let queued_messages = reconcile_ai_queued_messages_after_snapshot(
            &mut self.ai_queued_messages,
            &mut self.ai_interrupt_restore_queued_thread_ids,
            &self.ai_state_snapshot,
        );
        self.restore_ai_queued_messages_to_drafts(queued_messages)
    }

    fn queue_current_ai_prompt_for_thread(
        &mut self,
        thread_id: String,
        prompt: String,
        local_images: Vec<PathBuf>,
        selected_skills: Vec<crate::app::AiPromptSkillReference>,
        skill_bindings: Vec<crate::app::AiComposerSkillBinding>,
    ) {
        self.ai_queued_messages.push(AiQueuedUserMessage {
            thread_id,
            prompt,
            local_images,
            selected_skills,
            skill_bindings,
            queued_at: Instant::now(),
            status: AiQueuedUserMessageStatus::Queued,
        });
    }

    fn edit_last_ai_queued_message_for_thread(
        &mut self,
        thread_id: &str,
    ) -> Option<AiQueuedUserMessage> {
        take_last_editable_ai_queued_message_for_thread(&mut self.ai_queued_messages, thread_id)
    }

    fn maybe_submit_ready_ai_queued_messages(
        &mut self,
        workspace_key: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let ready_thread_ids = ready_ai_queued_message_thread_ids(
            self.ai_queued_messages.as_slice(),
            &self.ai_interrupt_restore_queued_thread_ids,
            &self.ai_state_snapshot,
        );
        for thread_id in ready_thread_ids {
            let accepted_after_sequence =
                thread_latest_timeline_sequence(&self.ai_state_snapshot, thread_id.as_str());
            let Some((queued_ix, queued)) = mark_next_ai_queued_message_pending_confirmation(
                self.ai_queued_messages.as_mut_slice(),
                thread_id.as_str(),
                accepted_after_sequence,
            )
            else {
                continue;
            };

            let prompt = (!queued.prompt.trim().is_empty()).then_some(queued.prompt.clone());
            let sent = self.send_ai_worker_command_for_workspace(
                workspace_key,
                AiWorkerCommand::SendPrompt {
                    thread_id: queued.thread_id.clone(),
                    prompt,
                    local_image_paths: queued.local_images.clone(),
                    selected_skills: queued.selected_skills.clone(),
                    skill_bindings: queued.skill_bindings.clone(),
                    session_overrides: self.current_ai_turn_session_overrides(),
                },
                false,
                cx,
            );
            if !sent {
                reset_ai_queued_message_to_queued(
                    self.ai_queued_messages.as_mut_slice(),
                    queued_ix,
                );
            }
        }
    }
}
