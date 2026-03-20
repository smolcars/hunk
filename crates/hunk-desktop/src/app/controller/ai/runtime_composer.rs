impl DiffViewer {
    fn ai_composer_retained_thread_ids(&self) -> BTreeSet<String> {
        ai_composer_retained_thread_ids(&self.ai_state_snapshot, &self.ai_workspace_states)
    }

    fn workspace_ai_composer_draft_key(&self) -> Option<AiComposerDraftKey> {
        let workspace_key = self.ai_workspace_key_for_draft();
        ai_composer_draft_key(None, workspace_key.as_deref())
    }

    fn current_ai_composer_draft_key(&self) -> Option<AiComposerDraftKey> {
        let current_thread_id = self.current_ai_thread_id();
        let workspace_key = self.ai_workspace_key();
        ai_composer_draft_key(current_thread_id.as_deref(), workspace_key.as_deref())
    }

    fn current_ai_composer_draft(&self) -> Option<&AiComposerDraft> {
        let key = self.current_ai_composer_draft_key()?;
        self.ai_composer_drafts.get(&key)
    }

    fn current_ai_composer_draft_mut(&mut self) -> Option<&mut AiComposerDraft> {
        let key = self.current_ai_composer_draft_key()?;
        Some(self.ai_composer_drafts.entry(key).or_default())
    }

    pub(crate) fn current_ai_composer_local_images(&self) -> Vec<PathBuf> {
        self.current_ai_composer_draft()
            .map(|draft| draft.local_images.clone())
            .unwrap_or_default()
    }

    pub(crate) fn current_ai_composer_skill_bindings(
        &self,
    ) -> Vec<crate::app::AiComposerSkillBinding> {
        self.current_ai_composer_draft()
            .map(|draft| draft.skill_bindings.clone())
            .unwrap_or_default()
    }

    fn composer_status_message_for_target(
        &self,
        target_key: Option<&AiComposerDraftKey>,
    ) -> Option<&str> {
        target_key.and_then(|key| {
            self.ai_composer_status_by_draft
                .get(key)
                .map(String::as_str)
        })
    }

    pub(crate) fn current_ai_composer_status_message(&self) -> Option<&str> {
        self.composer_status_message_for_target(self.current_ai_composer_draft_key().as_ref())
            .or(self.ai_status_message.as_deref())
    }

    fn set_ai_composer_status_for_target(
        &mut self,
        target_key: Option<AiComposerDraftKey>,
        message: impl Into<String>,
    ) {
        let message = message.into();
        if let Some(key) = target_key {
            self.ai_composer_status_by_draft.insert(key, message);
        } else {
            self.ai_status_message = Some(message);
        }
    }

    fn set_current_ai_composer_status(&mut self, message: impl Into<String>) {
        let target_key = self.current_ai_composer_draft_key();
        self.set_ai_composer_status_for_target(target_key, message);
    }

    fn clear_ai_composer_status_for_target(&mut self, target_key: Option<&AiComposerDraftKey>) {
        if let Some(key) = target_key {
            self.ai_composer_status_by_draft.remove(key);
        } else {
            self.ai_status_message = None;
        }
    }

    fn clear_current_ai_composer_status(&mut self) {
        let target_key = self.current_ai_composer_draft_key();
        self.clear_ai_composer_status_for_target(target_key.as_ref());
    }

    fn sync_ai_visible_composer_prompt_to_draft(&mut self, cx: &Context<Self>) {
        let prompt = self.ai_composer_input_state.read(cx).value().to_string();
        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.skill_bindings = crate::app::ai_composer_completion::reconcile_ai_composer_skill_bindings(
                draft.prompt.as_str(),
                draft.skill_bindings.as_slice(),
                prompt.as_str(),
            );
            draft.prompt = prompt;
        }
    }

    fn restore_ai_visible_composer_from_current_draft(&mut self, cx: &mut Context<Self>) {
        let prompt = ai_composer_prompt_for_target(
            &self.ai_composer_drafts,
            self.current_ai_composer_draft_key().as_ref(),
        );
        let ai_composer_state = self.ai_composer_input_state.clone();
        if let Err(error) = Self::update_any_window(cx, move |window, cx| {
            ai_composer_state.update(cx, |state, cx| {
                state.set_value(prompt.clone(), window, cx);
            });
        }) {
            error!("failed to restore AI composer input after thread change: {error:#}");
        }
    }

    fn restore_ai_visible_composer_from_current_draft_in_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = ai_composer_prompt_for_target(
            &self.ai_composer_drafts,
            self.current_ai_composer_draft_key().as_ref(),
        );
        self.ai_composer_input_state.update(cx, |state, cx| {
            state.set_value(prompt, window, cx);
        });
    }

    fn prune_ai_composer_drafts(&mut self) {
        let thread_ids = self.ai_composer_retained_thread_ids();
        self.ai_composer_drafts.retain(|key, _| match key {
            AiComposerDraftKey::Thread(thread_id) => thread_ids.contains(thread_id),
            AiComposerDraftKey::Workspace(_) => true,
        });
    }

    fn prune_ai_composer_statuses(&mut self) {
        let thread_ids = self.ai_composer_retained_thread_ids();
        self.ai_composer_status_by_draft.retain(|key, _| match key {
            AiComposerDraftKey::Thread(thread_id) => thread_ids.contains(thread_id),
            AiComposerDraftKey::Workspace(_) => true,
        });
    }

    fn restore_ai_new_thread_draft_after_failure(&mut self, cx: &mut Context<Self>) {
        if self.ai_pending_new_thread_selection {
            self.ai_new_thread_draft_active = true;
        }
        self.ai_pending_new_thread_selection = false;
        let Some(pending) = self.ai_pending_thread_start.take() else {
            return;
        };
        let current_workspace_key = self.ai_workspace_key_for_draft();
        if current_workspace_key.as_deref() != Some(pending.workspace_key.as_str()) {
            self.ai_pending_thread_start = Some(pending);
            return;
        }
        let target_key = self.workspace_ai_composer_draft_key();
        if let Some(target_key) = target_key {
            let draft = self.ai_composer_drafts.entry(target_key).or_default();
            draft.prompt = pending.prompt;
            draft.local_images = pending.local_images;
            draft.skill_bindings = pending.skill_bindings;
        }
        self.restore_ai_visible_composer_from_current_draft(cx);
    }

    fn current_ai_composer_activity_elapsed_second(&self) -> Option<u64> {
        let thread_id = self.current_ai_thread_id()?;
        let turn_id = self.current_ai_in_progress_turn_id(thread_id.as_str())?;
        let tracking_key = format!("{thread_id}::{turn_id}");
        self.ai_in_progress_turn_started_at
            .get(tracking_key.as_str())
            .map(|started_at| started_at.elapsed().as_secs())
    }

    fn sync_ai_composer_activity_elapsed_second(&mut self) -> bool {
        let next = self.current_ai_composer_activity_elapsed_second();
        if self.ai_composer_activity_elapsed_second == next {
            return false;
        }
        self.ai_composer_activity_elapsed_second = next;
        true
    }
}

fn ai_composer_retained_thread_ids(
    state_snapshot: &hunk_codex::state::AiState,
    workspace_states: &BTreeMap<String, AiWorkspaceState>,
) -> BTreeSet<String> {
    let mut thread_ids = state_snapshot.threads.keys().cloned().collect::<BTreeSet<_>>();

    for workspace_state in workspace_states.values() {
        thread_ids.extend(workspace_state.state_snapshot.threads.keys().cloned());
    }

    thread_ids
}
