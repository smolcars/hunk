#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AiComposerCompletionAction {
    SelectNext,
    SelectPrevious,
    Accept,
    Dismiss,
}

pub(super) fn ai_composer_completion_action_for_keystroke(
    keystroke: &gpui::Keystroke,
) -> Option<AiComposerCompletionAction> {
    let modifiers = &keystroke.modifiers;
    if modifiers.modified() {
        return None;
    }

    match keystroke.key.as_str() {
        "down" => Some(AiComposerCompletionAction::SelectNext),
        "up" => Some(AiComposerCompletionAction::SelectPrevious),
        "enter" => Some(AiComposerCompletionAction::Accept),
        "escape" => Some(AiComposerCompletionAction::Dismiss),
        _ => None,
    }
}

impl DiffViewer {
    fn ai_composer_completion_menu_token(
        menu: &AiComposerFileCompletionMenuState,
    ) -> ActivePrefixedToken {
        ActivePrefixedToken {
            query: menu.query.clone(),
            replace_range: menu.replace_range.clone(),
        }
    }

    fn ai_composer_skill_completion_menu_token(
        menu: &AiComposerSkillCompletionMenuState,
    ) -> ActivePrefixedToken {
        ActivePrefixedToken {
            query: menu.query.clone(),
            replace_range: menu.replace_range.clone(),
        }
    }

    fn current_ai_composer_file_completion_candidate(
        &self,
        cx: &Context<Self>,
    ) -> Option<AiComposerFileCompletionMenuState> {
        let input = self.ai_composer_input_state.read(cx);
        let text = input.value().to_string();
        let cursor = input.cursor();
        self.ai_composer_file_completion_provider
            .menu_state(text.as_str(), cursor)
    }

    fn current_ai_composer_skill_completion_candidate(
        &self,
        cx: &Context<Self>,
    ) -> Option<AiComposerSkillCompletionMenuState> {
        let input = self.ai_composer_input_state.read(cx);
        let text = input.value().to_string();
        let cursor = input.cursor();
        skill_completion_menu_state(self.ai_skills.as_slice(), text.as_str(), cursor)
    }

    pub(super) fn sync_ai_composer_completion_menus(&mut self, cx: &mut Context<Self>) {
        self.sync_ai_composer_file_completion_menu(cx);
        self.sync_ai_composer_skill_completion_menu(cx);
    }

    pub(super) fn sync_ai_composer_file_completion_menu(&mut self, cx: &mut Context<Self>) {
        let next_menu = self.current_ai_composer_file_completion_candidate(cx);
        let next_token = next_menu
            .as_ref()
            .map(Self::ai_composer_completion_menu_token);
        if self.ai_composer_file_completion_dismissed_token.as_ref() != next_token.as_ref() {
            self.ai_composer_file_completion_dismissed_token = None;
        }

        let mut next_visible_menu = next_menu.clone();
        if self.ai_composer_file_completion_dismissed_token.as_ref() == next_token.as_ref() {
            next_visible_menu = None;
        }

        let current_token = self
            .ai_composer_file_completion_menu
            .as_ref()
            .map(Self::ai_composer_completion_menu_token);
        if current_token != next_token {
            self.ai_composer_file_completion_selected_ix = 0;
        }

        let mut changed = self.ai_composer_file_completion_menu != next_visible_menu;
        self.ai_composer_file_completion_menu = next_visible_menu;
        if let Some(menu) = self.ai_composer_file_completion_menu.as_ref() {
            let clamped_ix = self
                .ai_composer_file_completion_selected_ix
                .min(menu.items.len().saturating_sub(1));
            if clamped_ix != self.ai_composer_file_completion_selected_ix {
                self.ai_composer_file_completion_selected_ix = clamped_ix;
                changed = true;
            }
        } else if self.ai_composer_file_completion_selected_ix != 0 {
            self.ai_composer_file_completion_selected_ix = 0;
            changed = true;
        }

        if changed {
            cx.notify();
        }
    }

    pub(super) fn sync_ai_composer_skill_completion_menu(&mut self, cx: &mut Context<Self>) {
        let next_menu = self.current_ai_composer_skill_completion_candidate(cx);
        let next_token = next_menu
            .as_ref()
            .map(Self::ai_composer_skill_completion_menu_token);
        if self.ai_composer_skill_completion_dismissed_token.as_ref() != next_token.as_ref() {
            self.ai_composer_skill_completion_dismissed_token = None;
        }

        let mut next_visible_menu = next_menu.clone();
        if self.ai_composer_skill_completion_dismissed_token.as_ref() == next_token.as_ref() {
            next_visible_menu = None;
        }

        let current_token = self
            .ai_composer_skill_completion_menu
            .as_ref()
            .map(Self::ai_composer_skill_completion_menu_token);
        if current_token != next_token {
            self.ai_composer_skill_completion_selected_ix = 0;
        }

        let mut changed = self.ai_composer_skill_completion_menu != next_visible_menu;
        self.ai_composer_skill_completion_menu = next_visible_menu;
        if let Some(menu) = self.ai_composer_skill_completion_menu.as_ref() {
            let clamped_ix = self
                .ai_composer_skill_completion_selected_ix
                .min(menu.items.len().saturating_sub(1));
            if clamped_ix != self.ai_composer_skill_completion_selected_ix {
                self.ai_composer_skill_completion_selected_ix = clamped_ix;
                changed = true;
            }
        } else if self.ai_composer_skill_completion_selected_ix != 0 {
            self.ai_composer_skill_completion_selected_ix = 0;
            changed = true;
        }

        if changed {
            cx.notify();
        }
    }

    pub(super) fn ai_handle_composer_completion_keystroke(
        &mut self,
        action: AiComposerCompletionAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.workspace_view_mode != WorkspaceViewMode::Ai {
            return false;
        }

        let composer_focus_handle =
            gpui::Focusable::focus_handle(self.ai_composer_input_state.read(cx), cx);
        if !composer_focus_handle.is_focused(window) {
            return false;
        }

        let skill_menu_open = self.ai_composer_skill_completion_menu.is_some();
        let file_menu_open = self.ai_composer_file_completion_menu.is_some();
        if !skill_menu_open && !file_menu_open {
            return false;
        }

        window.prevent_default();
        cx.stop_propagation();

        if skill_menu_open {
            return match action {
                AiComposerCompletionAction::SelectNext => {
                    if let Some(menu) = self.ai_composer_skill_completion_menu.as_ref() {
                        self.ai_composer_skill_completion_selected_ix = (self
                            .ai_composer_skill_completion_selected_ix
                            + 1)
                            .min(menu.items.len().saturating_sub(1));
                    }
                    cx.notify();
                    true
                }
                AiComposerCompletionAction::SelectPrevious => {
                    self.ai_composer_skill_completion_selected_ix =
                        self.ai_composer_skill_completion_selected_ix.saturating_sub(1);
                    cx.notify();
                    true
                }
                AiComposerCompletionAction::Accept => {
                    self.accept_ai_composer_skill_completion(window, cx)
                }
                AiComposerCompletionAction::Dismiss => {
                    self.dismiss_ai_composer_skill_completion(cx);
                    true
                }
            };
        }

        match action {
            AiComposerCompletionAction::SelectNext => {
                if let Some(menu) = self.ai_composer_file_completion_menu.as_ref() {
                    self.ai_composer_file_completion_selected_ix = (self
                        .ai_composer_file_completion_selected_ix
                        + 1)
                        .min(menu.items.len().saturating_sub(1));
                }
                cx.notify();
                true
            }
            AiComposerCompletionAction::SelectPrevious => {
                self.ai_composer_file_completion_selected_ix = self
                    .ai_composer_file_completion_selected_ix
                    .saturating_sub(1);
                cx.notify();
                true
            }
            AiComposerCompletionAction::Accept => {
                self.accept_ai_composer_file_completion(window, cx)
            }
            AiComposerCompletionAction::Dismiss => {
                self.dismiss_ai_composer_file_completion(cx);
                true
            }
        }
    }

    fn dismiss_ai_composer_file_completion(&mut self, cx: &mut Context<Self>) {
        self.ai_composer_file_completion_dismissed_token = self
            .ai_composer_file_completion_menu
            .as_ref()
            .map(Self::ai_composer_completion_menu_token);
        self.ai_composer_file_completion_menu = None;
        self.ai_composer_file_completion_selected_ix = 0;
        cx.notify();
    }

    fn dismiss_ai_composer_skill_completion(&mut self, cx: &mut Context<Self>) {
        self.ai_composer_skill_completion_dismissed_token = self
            .ai_composer_skill_completion_menu
            .as_ref()
            .map(Self::ai_composer_skill_completion_menu_token);
        self.ai_composer_skill_completion_menu = None;
        self.ai_composer_skill_completion_selected_ix = 0;
        cx.notify();
    }

    fn accept_ai_composer_file_completion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.ai_composer_file_completion_menu.clone() else {
            return false;
        };
        let Some(path) = menu
            .items
            .get(self.ai_composer_file_completion_selected_ix)
            .cloned()
        else {
            return false;
        };

        let inserted_text = ai_composer_inserted_path_text(path.as_str());
        let utf16_range = self.ai_composer_input_state.read(cx).text();
        let utf16_range = utf16_range.offset_to_offset_utf16(menu.replace_range.start)
            ..utf16_range.offset_to_offset_utf16(menu.replace_range.end);

        self.ai_composer_input_state.update(cx, |state, cx| {
            state.replace_text_in_range(Some(utf16_range), inserted_text.as_str(), window, cx);
            state.focus(window, cx);
        });

        self.ai_composer_file_completion_dismissed_token = None;
        self.sync_ai_composer_file_completion_menu(cx);
        true
    }

    fn accept_ai_composer_skill_completion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.ai_composer_skill_completion_menu.clone() else {
            return false;
        };
        let Some(skill) = menu
            .items
            .get(self.ai_composer_skill_completion_selected_ix)
            .cloned()
        else {
            return false;
        };

        let current_text = self.ai_composer_input_state.read(cx).value().to_string();
        let inserted_text =
            crate::app::ai_composer_completion::ai_composer_inserted_skill_text(skill.name.as_str());
        let mut next_prompt = current_text.clone();
        next_prompt.replace_range(menu.replace_range.clone(), inserted_text.as_str());
        let next_binding = crate::app::ai_composer_completion::ai_composer_inserted_skill_binding(
            skill.name.as_str(),
            skill.path.clone(),
            menu.replace_range.clone(),
        );
        let utf16_range = self.ai_composer_input_state.read(cx).text();
        let utf16_range = utf16_range.offset_to_offset_utf16(menu.replace_range.start)
            ..utf16_range.offset_to_offset_utf16(menu.replace_range.end);

        if let Some(draft) = self.current_ai_composer_draft_mut() {
            draft.skill_bindings = crate::app::ai_composer_completion::reconcile_ai_composer_skill_bindings(
                draft.prompt.as_str(),
                draft.skill_bindings.as_slice(),
                next_prompt.as_str(),
            );
            draft.prompt = next_prompt;
            draft.skill_bindings.push(next_binding);
        }

        self.ai_composer_input_state.update(cx, |state, cx| {
            state.replace_text_in_range(Some(utf16_range), inserted_text.as_str(), window, cx);
            state.focus(window, cx);
        });

        self.ai_composer_skill_completion_dismissed_token = None;
        self.sync_ai_composer_skill_completion_menu(cx);
        true
    }

    pub(super) fn ai_accept_composer_file_completion_path(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.ai_composer_file_completion_menu.as_ref() else {
            return;
        };
        let Some(selected_ix) = menu.items.iter().position(|item| item == &path) else {
            return;
        };
        self.ai_composer_file_completion_selected_ix = selected_ix;
        let _ = self.accept_ai_composer_file_completion(window, cx);
    }

    pub(super) fn ai_accept_composer_skill_completion_name(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.ai_composer_skill_completion_menu.as_ref() else {
            return;
        };
        let Some(selected_ix) = menu.items.iter().position(|item| item.name == name) else {
            return;
        };
        self.ai_composer_skill_completion_selected_ix = selected_ix;
        let _ = self.accept_ai_composer_skill_completion(window, cx);
    }

    pub(super) fn request_ai_composer_file_completion_reload(&mut self, cx: &mut Context<Self>) {
        self.request_ai_composer_file_completion_reload_for_workspace(self.ai_workspace_cwd(), cx);
    }

    pub(super) fn request_ai_composer_file_completion_reload_for_workspace(
        &mut self,
        workspace_root: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_root) = workspace_root else {
            self.ai_composer_file_completion_provider.clear();
            self.ai_composer_file_completion_reload_task = Task::ready(());
            self.ai_composer_file_completion_menu = None;
            self.ai_composer_file_completion_dismissed_token = None;
            self.ai_composer_file_completion_selected_ix = 0;
            cx.notify();
            return;
        };

        let provider = self.ai_composer_file_completion_provider.clone();
        let generation = provider.begin_reload(Some(workspace_root.clone()));

        self.ai_composer_file_completion_reload_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn({
                    let workspace_root = workspace_root.clone();
                    async move { hunk_git::git::load_visible_repo_file_paths(&workspace_root) }
                })
                .await;

            match result {
                Ok(paths) => {
                    provider.apply_reload(generation, workspace_root.as_path(), paths);
                }
                Err(error) => {
                    warn!(
                        "failed to refresh AI composer file completions for {}: {error:#}",
                        workspace_root.display()
                    );
                    provider.apply_reload(generation, workspace_root.as_path(), Vec::new());
                }
            }

            let _ = this.update(cx, |this, cx| {
                this.sync_ai_composer_file_completion_menu(cx);
            });
        });
    }
}
