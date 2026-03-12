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

fn next_ai_queued_message_for_thread(
    queued_messages: &[AiQueuedUserMessage],
    thread_id: &str,
) -> Option<(usize, AiQueuedUserMessage)> {
    let queued_ix = queued_messages
        .iter()
        .position(|queued| queued.thread_id == thread_id)?;
    Some((queued_ix, queued_messages[queued_ix].clone()))
}

fn take_last_ai_queued_message_for_thread(
    queued_messages: &mut Vec<AiQueuedUserMessage>,
    thread_id: &str,
) -> Option<AiQueuedUserMessage> {
    let queued_ix = queued_messages
        .iter()
        .rposition(|queued| queued.thread_id == thread_id)?;
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

fn ready_ai_queued_message_thread_ids(
    queued_messages: &[AiQueuedUserMessage],
    interrupt_restore_thread_ids: &BTreeSet<String>,
    state: &hunk_codex::state::AiState,
) -> Vec<String> {
    let mut thread_ids = Vec::new();
    let mut seen = BTreeSet::new();

    for queued in queued_messages {
        if seen.contains(queued.thread_id.as_str()) {
            continue;
        }
        if interrupt_restore_thread_ids.contains(queued.thread_id.as_str()) {
            continue;
        }
        if !state.threads.contains_key(queued.thread_id.as_str()) {
            continue;
        }
        if ai_thread_has_in_progress_turn(state, queued.thread_id.as_str()) {
            continue;
        }
        seen.insert(queued.thread_id.clone());
        thread_ids.push(queued.thread_id.clone());
    }

    thread_ids
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
        let queued_messages = take_interrupted_ai_queued_messages(
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
    ) {
        self.ai_interrupt_restore_queued_thread_ids.remove(thread_id.as_str());
        self.ai_queued_messages.push(AiQueuedUserMessage {
            thread_id,
            prompt,
            local_images,
            queued_at: Instant::now(),
        });
    }

    fn edit_last_ai_queued_message_for_thread(
        &mut self,
        thread_id: &str,
    ) -> Option<AiQueuedUserMessage> {
        take_last_ai_queued_message_for_thread(&mut self.ai_queued_messages, thread_id)
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
            let Some((queued_ix, queued)) =
                next_ai_queued_message_for_thread(self.ai_queued_messages.as_slice(), thread_id.as_str())
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
                    session_overrides: self.current_ai_turn_session_overrides(),
                },
                false,
                cx,
            );
            if sent && queued_ix < self.ai_queued_messages.len() {
                self.ai_queued_messages.remove(queued_ix);
            }
        }
    }
}
