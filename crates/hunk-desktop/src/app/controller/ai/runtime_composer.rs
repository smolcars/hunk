impl DiffViewer {
    fn current_ai_composer_status_key(&self) -> AiComposerStatusKey {
        self.current_ai_composer_draft_key()
            .map(AiComposerStatusKey::Draft)
            .unwrap_or_else(|| AiComposerStatusKey::Workspace(self.ai_workspace_key_for_draft()))
    }

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

    fn next_ai_composer_status_generation(&mut self, key: &AiComposerStatusKey) -> usize {
        self.ai_composer_status_generation = self.ai_composer_status_generation.saturating_add(1);
        let generation = self.ai_composer_status_generation;
        self.ai_composer_status_generation_by_key
            .insert(key.clone(), generation);
        generation
    }

    fn clear_ai_composer_status_key_if_generation_matches(
        &mut self,
        key: &AiComposerStatusKey,
        generation: usize,
    ) -> bool {
        if self.ai_composer_status_generation_by_key.get(key).copied() != Some(generation) {
            return false;
        }

        self.ai_composer_status_generation_by_key.remove(key);
        match key {
            AiComposerStatusKey::Draft(draft_key) => {
                self.ai_composer_status_by_draft.remove(draft_key).is_some()
            }
            AiComposerStatusKey::Workspace(workspace_key) => {
                if self.ai_workspace_key_for_draft().as_ref() == workspace_key.as_ref() {
                    self.ai_status_message.take().is_some()
                } else if let Some(workspace_key) = workspace_key {
                    self.ai_workspace_states
                        .get_mut(workspace_key)
                        .and_then(|state| state.status_message.take())
                        .is_some()
                } else {
                    false
                }
            }
        }
    }

    fn set_ai_composer_status_for_target(
        &mut self,
        target_key: Option<AiComposerDraftKey>,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let message = message.into();
        let status_tone = ai_composer_status_tone(message.as_str());
        let status_key = target_key
            .clone()
            .map(AiComposerStatusKey::Draft)
            .unwrap_or_else(|| AiComposerStatusKey::Workspace(self.ai_workspace_key_for_draft()));

        if let Some(key) = target_key {
            self.ai_composer_status_by_draft.insert(key, message);
        } else {
            self.ai_status_message = Some(message);
        }

        if status_tone.is_none() {
            self.ai_composer_status_generation_by_key.remove(&status_key);
            return;
        }

        let generation = self.next_ai_composer_status_generation(&status_key);
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(AI_COMPOSER_STATUS_AUTO_DISMISS_DELAY)
                .await;
            cx.update(|cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| {
                        if this.clear_ai_composer_status_key_if_generation_matches(
                            &status_key,
                            generation,
                        ) {
                            cx.notify();
                        }
                    });
                }
            });
        })
        .detach();
    }

    fn set_current_ai_composer_status(
        &mut self,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let target_key = self.current_ai_composer_draft_key();
        self.set_ai_composer_status_for_target(target_key, message, cx);
    }

    fn clear_ai_composer_status_for_target(&mut self, target_key: Option<&AiComposerDraftKey>) {
        if let Some(key) = target_key {
            self.ai_composer_status_by_draft.remove(key);
            self.ai_composer_status_generation_by_key
                .remove(&AiComposerStatusKey::Draft(key.clone()));
        } else {
            self.ai_status_message = None;
            self.ai_composer_status_generation_by_key
                .remove(&self.current_ai_composer_status_key());
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
        let current_prompt = self.ai_composer_input_state.read(cx).value().to_string();
        if current_prompt == prompt {
            return;
        }
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
        self.ai_composer_status_generation_by_key
            .retain(|key, _| match key {
                AiComposerStatusKey::Draft(AiComposerDraftKey::Thread(thread_id)) => {
                    thread_ids.contains(thread_id)
                }
                _ => true,
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
        self.invalidate_ai_visible_frame_state_with_reason("thread");
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
