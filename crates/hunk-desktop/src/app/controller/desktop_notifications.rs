impl DiffViewer {
    pub(super) fn refresh_macos_notification_permission_status(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.desktop_notification_permission_task = cx.spawn(async move |this, cx| {
                let status = cx
                    .background_executor()
                    .spawn(async move {
                        crate::app::desktop_notifications::macos_notification_permission_status()
                    })
                    .await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |this, cx| {
                    match status {
                        Ok(status) => {
                            this.macos_notification_permission_state = status;
                            if matches!(
                                status,
                                crate::app::desktop_notifications::MacOsNotificationPermissionState::NotDetermined
                            ) && this.workspace_view_mode == WorkspaceViewMode::Ai
                            {
                                this.request_macos_notification_permission_for_ai(cx);
                            } else {
                                cx.notify();
                            }
                        }
                        Err(err) => {
                            error!("failed to read macOS notification permission status: {err:#}");
                        }
                    }
                });
            });
        }
    }

    pub(super) fn request_macos_notification_permission_for_ai(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            if !self.ai_desktop_notifications_enabled()
                || self.workspace_view_mode != WorkspaceViewMode::Ai
                || self.macos_notification_permission_request_in_flight
            {
                return;
            }
            if !matches!(
                self.macos_notification_permission_state,
                crate::app::desktop_notifications::MacOsNotificationPermissionState::NotDetermined
                    | crate::app::desktop_notifications::MacOsNotificationPermissionState::Unknown
            ) {
                return;
            }

            self.macos_notification_permission_request_in_flight = true;
            self.desktop_notification_permission_task = cx.spawn(async move |this, cx| {
                let status = cx
                    .background_executor()
                    .spawn(async move {
                        crate::app::desktop_notifications::request_macos_notification_permission()
                    })
                    .await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |this, cx| {
                    this.macos_notification_permission_request_in_flight = false;
                    match status {
                        Ok(status) => {
                            this.macos_notification_permission_state = status;
                            cx.notify();
                        }
                        Err(err) => {
                            error!("failed to request macOS notification permission: {err:#}");
                        }
                    }
                });
            });
        }
    }

    pub(super) fn maybe_prepare_ai_desktop_notifications(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            if matches!(
                self.macos_notification_permission_state,
                crate::app::desktop_notifications::MacOsNotificationPermissionState::Unknown
            ) {
                self.refresh_macos_notification_permission_status(cx);
                return;
            }
            if matches!(
                self.macos_notification_permission_state,
                crate::app::desktop_notifications::MacOsNotificationPermissionState::NotDetermined
            ) {
                self.request_macos_notification_permission_for_ai(cx);
            }
        }
    }

    pub(super) fn desktop_notification_settings_status_note(&self) -> Option<&'static str> {
        #[cfg(target_os = "macos")]
        {
            if !self.ai_desktop_notifications_enabled() {
                return None;
            }

            match self.macos_notification_permission_state {
                crate::app::desktop_notifications::MacOsNotificationPermissionState::Unavailable => {
                    Some(DESKTOP_NOTIFICATION_SETTINGS_STATUS_UNAVAILABLE)
                }
                crate::app::desktop_notifications::MacOsNotificationPermissionState::Denied => {
                    Some(DESKTOP_NOTIFICATION_SETTINGS_STATUS_DENIED)
                }
                crate::app::desktop_notifications::MacOsNotificationPermissionState::NotDetermined => {
                    Some(DESKTOP_NOTIFICATION_SETTINGS_STATUS_PENDING)
                }
                _ => None,
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    pub(super) fn clear_ai_desktop_notification_state(&mut self, workspace_key: Option<&str>) {
        if let Some(workspace_key) = workspace_key {
            self.ai_desktop_notification_state_by_workspace
                .remove(workspace_key);
        }
    }

    pub(super) fn maybe_emit_visible_ai_desktop_notification(&mut self, cx: &mut Context<Self>) {
        let Some(workspace_key) = self.ai_workspace_key() else {
            return;
        };
        let workspace_label =
            self.ai_active_workspace_label_with_root(self.ai_workspace_cwd().as_deref());
        let snapshot = self.ai_desktop_notification_snapshot(
            &self.ai_state_snapshot,
            self.ai_pending_approvals.as_slice(),
            self.ai_pending_user_inputs.as_slice(),
            &self.ai_followup_prompt_state_by_thread,
            workspace_label,
        );
        self.update_ai_desktop_notification_state(workspace_key, snapshot, cx);
    }

    pub(super) fn maybe_emit_background_ai_desktop_notification(
        &mut self,
        workspace_key: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.ai_workspace_states.get(workspace_key) else {
            return;
        };
        let workspace_label =
            self.ai_workspace_label_for_root(std::path::Path::new(workspace_key));
        let snapshot = self.ai_desktop_notification_snapshot(
            &state.state_snapshot,
            state.pending_approvals.as_slice(),
            state.pending_user_inputs.as_slice(),
            &state.followup_prompt_state_by_thread,
            workspace_label,
        );
        self.update_ai_desktop_notification_state(workspace_key.to_string(), snapshot, cx);
    }

    fn update_ai_desktop_notification_state(
        &mut self,
        workspace_key: String,
        snapshot: crate::app::desktop_notifications::AiDesktopNotificationSnapshot,
        cx: &mut Context<Self>,
    ) {
        let previous = self
            .ai_desktop_notification_state_by_workspace
            .get(workspace_key.as_str())
            .cloned();
        let (next_state, event) =
            crate::app::desktop_notifications::next_ai_desktop_notification_state(
                previous.as_ref(),
                snapshot,
            );
        self.ai_desktop_notification_state_by_workspace
            .insert(workspace_key, next_state);
        if let Some(event) = event {
            self.maybe_deliver_ai_desktop_notification(event, cx);
        }
    }

    fn maybe_deliver_ai_desktop_notification(
        &mut self,
        event: crate::app::desktop_notifications::AiDesktopNotificationEvent,
        cx: &mut Context<Self>,
    ) {
        if !self.ai_desktop_notification_kind_enabled(event.kind()) {
            return;
        }
        if self.config.desktop_notifications.only_when_unfocused
            && self.window_handle.update(cx, |_, window, _| window.is_window_active()).unwrap_or(false)
        {
            return;
        }
        #[cfg(target_os = "macos")]
        if !matches!(
            self.macos_notification_permission_state,
            crate::app::desktop_notifications::MacOsNotificationPermissionState::Authorized
        ) {
            return;
        }

        let request = event.request();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::app::desktop_notifications::show_desktop_notification(&request)
                })
                .await;
            if let Err(err) = result {
                error!("failed to deliver desktop notification: {err:#}");
            }
        })
        .detach();
    }

    fn ai_desktop_notification_snapshot(
        &self,
        state: &hunk_codex::state::AiState,
        pending_approvals: &[AiPendingApproval],
        pending_user_inputs: &[AiPendingUserInputRequest],
        prompt_state_by_thread: &BTreeMap<String, AiThreadFollowupPromptState>,
        workspace_label: String,
    ) -> crate::app::desktop_notifications::AiDesktopNotificationSnapshot {
        crate::app::desktop_notifications::AiDesktopNotificationSnapshot {
            workspace_label,
            approval_request_thread_by_id: pending_approvals
                .iter()
                .map(|request| (request.request_id.clone(), request.thread_id.clone()))
                .collect(),
            user_input_thread_by_id: pending_user_inputs
                .iter()
                .map(|request| (request.request_id.clone(), request.thread_id.clone()))
                .collect(),
            plan_prompt_sequence_by_thread: prompt_state_by_thread
                .iter()
                .filter_map(|(thread_id, prompt_state)| {
                    (prompt_state.prompt_source_sequence > 0)
                        .then_some((thread_id.clone(), prompt_state.prompt_source_sequence))
                })
                .collect(),
            in_progress_turns: state
                .turns
                .values()
                .filter(|turn| turn.status == hunk_codex::state::TurnStatus::InProgress)
                .map(|turn| crate::app::desktop_notifications::AiInProgressTurnKey {
                    thread_id: turn.thread_id.clone(),
                    turn_id: turn.id.clone(),
                })
                .collect(),
            thread_label_by_id: state
                .threads
                .values()
                .filter_map(|thread| {
                    thread
                        .title
                        .as_deref()
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .map(|title| (thread.id.clone(), title.to_string()))
                })
                .collect(),
        }
    }

    fn ai_desktop_notifications_enabled(&self) -> bool {
        self.config.desktop_notifications.enabled
            && (self.config.desktop_notifications.ai.agent_finished
                || self.config.desktop_notifications.ai.plan_ready
                || self.config.desktop_notifications.ai.user_input_required
                || self.config.desktop_notifications.ai.approval_required)
    }

    fn ai_desktop_notification_kind_enabled(
        &self,
        kind: crate::app::desktop_notifications::AiDesktopNotificationKind,
    ) -> bool {
        if !self.config.desktop_notifications.enabled {
            return false;
        }

        match kind {
            crate::app::desktop_notifications::AiDesktopNotificationKind::ApprovalRequired => {
                self.config.desktop_notifications.ai.approval_required
            }
            crate::app::desktop_notifications::AiDesktopNotificationKind::UserInputRequired => {
                self.config.desktop_notifications.ai.user_input_required
            }
            crate::app::desktop_notifications::AiDesktopNotificationKind::PlanReady => {
                self.config.desktop_notifications.ai.plan_ready
            }
            crate::app::desktop_notifications::AiDesktopNotificationKind::AgentFinished => {
                self.config.desktop_notifications.ai.agent_finished
            }
        }
    }
}
