impl ThreadService {
    pub fn archive_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        timeout: Duration,
    ) -> Result<ThreadArchiveResponse> {
        let params = ThreadArchiveParams {
            thread_id: thread_id.clone(),
        };
        self.request_and_reconcile(
            session,
            api::method::THREAD_ARCHIVE,
            Some(&params),
            timeout,
            move |service, _: &mut ThreadArchiveResponse| {
                if service.is_known_thread(&thread_id) {
                    service.apply_event(ReducerEvent::ThreadArchived { thread_id });
                }
                Ok(())
            },
        )
    }

    pub fn mark_thread_archived_if_known(&mut self, thread_id: String) {
        if self.is_known_thread(&thread_id) {
            self.apply_event(ReducerEvent::ThreadArchived { thread_id });
        }
    }

    pub fn unarchive_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        timeout: Duration,
    ) -> Result<ThreadUnarchiveResponse> {
        let params = ThreadUnarchiveParams { thread_id };
        self.request_and_reconcile(
            session,
            api::method::THREAD_UNARCHIVE,
            Some(&params),
            timeout,
            |service, response: &mut ThreadUnarchiveResponse| {
                service.ensure_thread_in_workspace(&response.thread)?;
                service.ingest_thread_snapshot(&response.thread);
                service.apply_event(ReducerEvent::ThreadUnarchived {
                    thread_id: response.thread.id.clone(),
                });
                Ok(())
            },
        )
    }

    pub fn compact_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        timeout: Duration,
    ) -> Result<ThreadCompactStartResponse> {
        let params = ThreadCompactStartParams { thread_id };
        self.request_with_notifications(
            session,
            api::method::THREAD_COMPACT_START,
            Some(&params),
            timeout,
        )
    }

    pub fn rollback_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        num_turns: u32,
        timeout: Duration,
    ) -> Result<ThreadRollbackResponse> {
        let params = ThreadRollbackParams {
            thread_id,
            num_turns,
        };
        self.request_and_reconcile(
            session,
            api::method::THREAD_ROLLBACK,
            Some(&params),
            timeout,
            |service, response: &mut ThreadRollbackResponse| {
                service.ensure_thread_in_workspace(&response.thread)?;
                service.replace_thread_turns_from_snapshot(&response.thread);
                service.ingest_thread_snapshot(&response.thread);
                Ok(())
            },
        )
    }

    pub fn unsubscribe_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        timeout: Duration,
    ) -> Result<ThreadUnsubscribeResponse> {
        let params = ThreadUnsubscribeParams {
            thread_id: thread_id.clone(),
        };
        self.request_and_reconcile(
            session,
            api::method::THREAD_UNSUBSCRIBE,
            Some(&params),
            timeout,
            move |service, response: &mut ThreadUnsubscribeResponse| {
                if matches!(
                    response.status,
                    ThreadUnsubscribeStatus::Unsubscribed | ThreadUnsubscribeStatus::NotLoaded
                ) && service.is_known_thread(&thread_id)
                {
                    service.apply_event(ReducerEvent::ThreadStatusChanged {
                        thread_id,
                        status: ThreadLifecycleStatus::NotLoaded,
                    });
                }
                Ok(())
            },
        )
    }

    pub fn apply_server_notification(&mut self, notification: ServerNotification) {
        match notification {
            ServerNotification::ThreadStarted(notification) => {
                if self.thread_matches_workspace(&notification.thread) {
                    self.ingest_thread_snapshot(&notification.thread);
                }
            }
            ServerNotification::ThreadStatusChanged(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::ThreadStatusChanged {
                        thread_id: notification.thread_id.clone(),
                        status: lifecycle_status_from_thread_status(&notification.status),
                    });
                    self.reconcile_thread_turns_for_status(
                        notification.thread_id.as_str(),
                        &notification.status,
                    );
                }
            }
            ServerNotification::ThreadArchived(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::ThreadArchived {
                        thread_id: notification.thread_id,
                    });
                }
            }
            ServerNotification::ThreadUnarchived(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::ThreadUnarchived {
                        thread_id: notification.thread_id,
                    });
                }
            }
            ServerNotification::ThreadClosed(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::ThreadStatusChanged {
                        thread_id: notification.thread_id.clone(),
                        status: ThreadLifecycleStatus::Closed,
                    });
                    self.complete_in_progress_turns(notification.thread_id.as_str());
                }
            }
            ServerNotification::ThreadNameUpdated(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::ThreadMetadataUpdated {
                        thread_id: notification.thread_id,
                        title: notification.thread_name,
                        updated_at: None,
                    });
                }
            }
            ServerNotification::ThreadTokenUsageUpdated(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::ThreadTokenUsageUpdated {
                        thread_id: notification.thread_id,
                        turn_id: notification.turn_id,
                        total: notification.token_usage.total.into(),
                        last: notification.token_usage.last.into(),
                        model_context_window: notification.token_usage.model_context_window,
                    });
                }
            }
            ServerNotification::TurnStarted(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_turn_snapshot(&notification.thread_id, &notification.turn);
                }
            }
            ServerNotification::TurnCompleted(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_turn_snapshot(&notification.thread_id, &notification.turn);
                }
            }
            ServerNotification::TurnDiffUpdated(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::TurnDiffUpdated {
                        thread_id: notification.thread_id,
                        turn_id: notification.turn_id,
                        diff: notification.diff,
                    });
                }
            }
            ServerNotification::TurnPlanUpdated(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::TurnPlanUpdated {
                        thread_id: notification.thread_id,
                        turn_id: notification.turn_id,
                        explanation: notification.explanation,
                        steps: notification
                            .plan
                            .into_iter()
                            .map(|step| crate::state::TurnPlanStepSummary {
                                step: step.step,
                                status: match step.status {
                                    codex_app_server_protocol::TurnPlanStepStatus::Pending => {
                                        crate::state::TurnPlanStepStatus::Pending
                                    }
                                    codex_app_server_protocol::TurnPlanStepStatus::InProgress => {
                                        crate::state::TurnPlanStepStatus::InProgress
                                    }
                                    codex_app_server_protocol::TurnPlanStepStatus::Completed => {
                                        crate::state::TurnPlanStepStatus::Completed
                                    }
                                },
                            })
                            .collect(),
                    });
                }
            }
            ServerNotification::ItemStarted(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_item_snapshot(
                        &notification.thread_id,
                        &notification.turn_id,
                        &notification.item,
                    );
                }
            }
            ServerNotification::ItemCompleted(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    let item_id = notification.item.id().to_string();
                    self.apply_item_snapshot(
                        &notification.thread_id,
                        &notification.turn_id,
                        &notification.item,
                    );
                    self.apply_event(ReducerEvent::ItemCompleted {
                        thread_id: notification.thread_id,
                        turn_id: notification.turn_id,
                        item_id,
                    });
                }
            }
            ServerNotification::AgentMessageDelta(notification) => {
                self.apply_item_delta_if_thread_known(
                    &notification.thread_id,
                    &notification.turn_id,
                    &notification.item_id,
                    "agentMessage",
                    &notification.delta,
                );
            }
            ServerNotification::PlanDelta(notification) => {
                self.apply_item_delta_if_thread_known(
                    &notification.thread_id,
                    &notification.turn_id,
                    &notification.item_id,
                    "plan",
                    &notification.delta,
                );
            }
            ServerNotification::ReasoningSummaryTextDelta(notification) => {
                self.apply_item_delta_if_thread_known(
                    &notification.thread_id,
                    &notification.turn_id,
                    &notification.item_id,
                    "reasoning",
                    &notification.delta,
                );
            }
            ServerNotification::ReasoningTextDelta(notification) => {
                self.apply_item_delta_if_thread_known(
                    &notification.thread_id,
                    &notification.turn_id,
                    &notification.item_id,
                    "reasoning",
                    &notification.delta,
                );
            }
            ServerNotification::CommandExecutionOutputDelta(notification) => {
                self.apply_item_delta_if_thread_known(
                    &notification.thread_id,
                    &notification.turn_id,
                    &notification.item_id,
                    "commandExecution",
                    &notification.delta,
                );
            }
            ServerNotification::FileChangeOutputDelta(notification) => {
                self.apply_item_delta_if_thread_known(
                    &notification.thread_id,
                    &notification.turn_id,
                    &notification.item_id,
                    "fileChange",
                    &notification.delta,
                );
            }
            ServerNotification::ServerRequestResolved(notification) => {
                if self.is_known_thread(&notification.thread_id) {
                    self.apply_event(ReducerEvent::ServerRequestResolved {
                        request_id: request_id_key(&notification.request_id),
                        item_id: None,
                        decision: ServerRequestDecision::Unknown,
                    });
                }
            }
            ServerNotification::Error(notification) => {
                if self.is_known_thread(&notification.thread_id) && !notification.will_retry {
                    self.apply_event(ReducerEvent::TurnStarted {
                        thread_id: notification.thread_id.clone(),
                        turn_id: notification.turn_id.clone(),
                    });
                    self.apply_event(ReducerEvent::TurnCompleted {
                        thread_id: notification.thread_id,
                        turn_id: notification.turn_id,
                    });
                }
            }
            _ => {}
        }
    }

    pub fn apply_queued_notifications(&mut self, session: &mut JsonRpcSession) {
        let _ = self.drain_and_apply_queued_notifications(session);
    }

    pub fn drain_and_apply_queued_notifications(
        &mut self,
        session: &mut JsonRpcSession,
    ) -> Vec<ServerNotification> {
        let notifications = session.drain_server_notifications();
        for notification in notifications.iter().cloned() {
            self.apply_server_notification(notification);
        }
        notifications
    }

    pub fn drain_queued_server_requests(
        &mut self,
        session: &mut JsonRpcSession,
    ) -> Vec<ServerRequest> {
        session.drain_server_requests()
    }

    pub fn record_server_request_resolved(
        &mut self,
        request_id: RequestId,
        item_id: Option<String>,
        decision: ServerRequestDecision,
    ) {
        self.apply_event(ReducerEvent::ServerRequestResolved {
            request_id: request_id_key(&request_id),
            item_id,
            decision,
        });
    }

    pub fn ingest_rollout_fallback_history(
        &mut self,
        thread_id: String,
        turns: &[RolloutFallbackTurn],
    ) {
        if turns.is_empty() {
            return;
        }

        self.ensure_local_thread(thread_id.clone());

        for turn in turns {
            self.apply_event(ReducerEvent::TurnStarted {
                thread_id: thread_id.clone(),
                turn_id: turn.turn_id.clone(),
            });

            for (item_index, item) in turn.items.iter().enumerate() {
                let item_id = format!(
                    "rollout:{}:{}:{}",
                    thread_id,
                    turn.turn_id,
                    item_index.saturating_add(1)
                );
                if self
                    .state
                    .items
                    .contains_key(item_storage_key(&thread_id, &turn.turn_id, &item_id).as_str())
                {
                    continue;
                }

                self.apply_event(ReducerEvent::ItemStarted {
                    thread_id: thread_id.clone(),
                    turn_id: turn.turn_id.clone(),
                    item_id: item_id.clone(),
                    kind: item.kind.clone(),
                });
                if !item.content.is_empty() {
                    self.apply_event(ReducerEvent::ItemDelta {
                        thread_id: thread_id.clone(),
                        turn_id: turn.turn_id.clone(),
                        item_id: item_id.clone(),
                        delta: item.content.clone(),
                    });
                }
                self.apply_event(ReducerEvent::ItemCompleted {
                    thread_id: thread_id.clone(),
                    turn_id: turn.turn_id.clone(),
                    item_id,
                });
            }

            if turn.completed {
                self.apply_event(ReducerEvent::TurnCompleted {
                    thread_id: thread_id.clone(),
                    turn_id: turn.turn_id.clone(),
                });
            }
        }
    }

    fn ensure_thread_id_in_workspace(&self, thread_id: &str) -> Result<()> {
        if let Some(thread) = self.state.threads.get(thread_id) {
            if thread.cwd == self.cwd_key() {
                return Ok(());
            }
            return Err(CodexIntegrationError::ThreadOutsideWorkspace {
                thread_id: thread_id.to_string(),
                expected_cwd: self.cwd_key(),
                actual_cwd: thread.cwd.clone(),
            });
        }
        Ok(())
    }

    fn ensure_local_thread(&mut self, thread_id: String) {
        if self.state.threads.contains_key(&thread_id) {
            return;
        }

        self.apply_event(ReducerEvent::ThreadStarted {
            thread_id,
            cwd: self.cwd_key(),
            title: None,
            created_at: None,
            updated_at: None,
        });
    }

    fn apply_turn_snapshot(&mut self, thread_id: &str, turn: &codex_app_server_protocol::Turn) {
        self.apply_event(ReducerEvent::TurnStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn.id.clone(),
        });
        if !matches!(turn.status, TurnStatus::InProgress) {
            self.apply_event(ReducerEvent::TurnCompleted {
                thread_id: thread_id.to_string(),
                turn_id: turn.id.clone(),
            });
        }
    }

    fn apply_item_snapshot(&mut self, thread_id: &str, turn_id: &str, item: &ThreadItem) {
        let item_id = item.id().to_string();
        let item_key = item_storage_key(thread_id, turn_id, item_id.as_str());
        let kind = thread_item_kind(item).to_string();
        let seed_content = thread_item_seed_content(item);
        let should_seed_content = self
            .state
            .items
            .get(item_key.as_str())
            .is_none_or(|existing| existing.content.is_empty());
        self.apply_event(ReducerEvent::TurnStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
        });
        self.apply_event(ReducerEvent::ItemStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            item_id: item_id.clone(),
            kind: kind.clone(),
        });
        if let Some(display_metadata) = thread_item_display_metadata(item) {
            self.apply_event(ReducerEvent::ItemDisplayMetadataUpdated {
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                item_id: item_id.clone(),
                metadata: display_metadata,
            });
        }

        if should_seed_content && let Some(seed_content) = seed_content.as_ref() {
            self.apply_event(ReducerEvent::ItemDelta {
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                item_id: item_id.clone(),
                delta: seed_content.clone(),
            });
        } else if let Some(seed_content) = seed_content.as_ref()
            && self.should_replace_item_snapshot_content(item, item_key.as_str(), seed_content)
        {
            self.apply_event(ReducerEvent::ItemContentSet {
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                item_id: item_id.clone(),
                content: seed_content.clone(),
            });
        }

        let content = self
            .state
            .items
            .get(item_key.as_str())
            .map(|item| item.content.clone())
            .unwrap_or_default();
        self.reconcile_rollout_fallback_item(
            thread_id,
            turn_id,
            kind.as_str(),
            content.as_str(),
        );

        if thread_item_is_complete(item) {
            self.apply_event(ReducerEvent::ItemCompleted {
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                item_id,
            });
        }
    }

    fn apply_item_delta_if_thread_known(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        item_id: &str,
        kind: &str,
        delta: &str,
    ) {
        if !self.is_known_thread(thread_id) {
            return;
        }

        self.apply_event(ReducerEvent::TurnStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
        });
        self.apply_event(ReducerEvent::ItemStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            item_id: item_id.to_string(),
            kind: kind.to_string(),
        });
        self.apply_event(ReducerEvent::ItemDelta {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            item_id: item_id.to_string(),
            delta: delta.to_string(),
        });
        let item_key = item_storage_key(thread_id, turn_id, item_id);
        let content = self
            .state
            .items
            .get(item_key.as_str())
            .map(|item| item.content.clone())
            .unwrap_or_default();
        self.reconcile_rollout_fallback_item(thread_id, turn_id, kind, content.as_str());
    }

    fn ingest_thread_snapshot(&mut self, thread: &Thread) {
        let title = thread
            .name
            .clone()
            .or_else(|| (!thread.preview.trim().is_empty()).then(|| thread.preview.clone()));
        self.apply_event(ReducerEvent::ThreadStarted {
            thread_id: thread.id.clone(),
            cwd: workspace_path_key(thread.cwd.as_path()),
            title,
            created_at: Some(thread.created_at),
            updated_at: Some(thread.updated_at),
        });
        self.apply_event(ReducerEvent::ThreadStatusChanged {
            thread_id: thread.id.clone(),
            status: lifecycle_status_from_thread_status(&thread.status),
        });

        for turn in &thread.turns {
            self.apply_event(ReducerEvent::TurnStarted {
                thread_id: thread.id.clone(),
                turn_id: turn.id.clone(),
            });
            if !matches!(turn.status, TurnStatus::InProgress) {
                self.apply_event(ReducerEvent::TurnCompleted {
                    thread_id: thread.id.clone(),
                    turn_id: turn.id.clone(),
                });
            }
            for item in &turn.items {
                self.apply_item_snapshot(&thread.id, &turn.id, item);
            }
        }

        self.reconcile_thread_turns_for_status(thread.id.as_str(), &thread.status);
    }

    fn replace_thread_turns_from_snapshot(&mut self, thread: &Thread) {
        let keep_turn_ids: BTreeSet<String> =
            thread.turns.iter().map(|turn| turn.id.clone()).collect();
        let keep_item_keys: BTreeSet<String> = thread
            .turns
            .iter()
            .flat_map(|turn| {
                turn.items.iter().map(|item| {
                    item_storage_key(thread.id.as_str(), turn.id.as_str(), item.id())
                })
            })
            .collect();
        let removed_turn_keys: BTreeSet<String> = self
            .state
            .turns
            .iter()
            .filter(|(_, turn)| turn.thread_id == thread.id && !keep_turn_ids.contains(&turn.id))
            .map(|(turn_key, _)| turn_key.clone())
            .collect();

        for turn_key in &removed_turn_keys {
            self.state.turns.remove(turn_key);
        }

        self.state
            .items
            .retain(|item_key, item| {
                item.thread_id != thread.id
                    || (keep_turn_ids.contains(item.turn_id.as_str())
                        && keep_item_keys.contains(item_key.as_str()))
            });
        self.state
            .turn_diffs
            .retain(|turn_key, _| !removed_turn_keys.contains(turn_key));
        self.state
            .turn_plans
            .retain(|turn_key, _| !removed_turn_keys.contains(turn_key));
    }

    fn reconcile_rollout_fallback_item(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        kind: &str,
        content: &str,
    ) {
        if content.is_empty() {
            return;
        }

        let turn_item_key_prefix = item_storage_key(thread_id, turn_id, "");
        let item_keys_to_remove = self
            .state
            .items
            .range(turn_item_key_prefix.clone()..)
            .take_while(|(item_key, _)| item_key.starts_with(turn_item_key_prefix.as_str()))
            .filter(|(_, item)| {
                is_rollout_fallback_item_id(item.id.as_str())
                    && item.kind == kind
                    && normalized_item_content(item.content.as_str())
                        == normalized_item_content(content)
            })
            .map(|(item_key, _)| item_key.clone())
            .collect::<Vec<_>>();
        for item_key in item_keys_to_remove {
            self.state.items.remove(item_key.as_str());
        }
    }

    fn should_replace_item_snapshot_content(
        &self,
        item: &ThreadItem,
        item_key: &str,
        seed_content: &str,
    ) -> bool {
        matches!(item, ThreadItem::UserMessage { .. })
            && self
                .state
                .items
                .get(item_key)
                .is_some_and(|existing| {
                    normalized_item_content(existing.content.as_str())
                        != normalized_item_content(seed_content)
                })
    }

    fn ensure_thread_in_workspace(&self, thread: &Thread) -> Result<()> {
        if self.thread_matches_workspace(thread) {
            return Ok(());
        }

        Err(CodexIntegrationError::ThreadOutsideWorkspace {
            thread_id: thread.id.clone(),
            expected_cwd: self.cwd_key(),
            actual_cwd: workspace_path_key(thread.cwd.as_path()),
        })
    }

    fn reconcile_thread_turns_for_status(&mut self, thread_id: &str, status: &ThreadStatus) {
        match status {
            ThreadStatus::Active { .. } => {}
            ThreadStatus::Idle | ThreadStatus::NotLoaded | ThreadStatus::SystemError => {
                self.complete_in_progress_turns(thread_id);
            }
        }
    }

    fn complete_in_progress_turns(&mut self, thread_id: &str) {
        let in_progress_turn_ids = self
            .state
            .turns
            .values()
            .filter(|turn| {
                turn.thread_id == thread_id && turn.status == StateTurnStatus::InProgress
            })
            .map(|turn| turn.id.clone())
            .collect::<Vec<_>>();

        for turn_id in in_progress_turn_ids {
            self.apply_event(ReducerEvent::TurnCompleted {
                thread_id: thread_id.to_string(),
                turn_id,
            });
        }
    }

    fn thread_matches_workspace(&self, thread: &Thread) -> bool {
        normalize_workspace_path(thread.cwd.as_path()) == self.cwd
    }

    fn is_known_thread(&self, thread_id: &str) -> bool {
        self.state.threads.contains_key(thread_id)
    }

    fn select_active_thread(&mut self, thread_id: String) {
        self.apply_event(ReducerEvent::ActiveThreadSelected {
            cwd: self.cwd_key(),
            thread_id,
        });
    }

    fn apply_event(&mut self, payload: ReducerEvent) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        let _ = self.state.apply_stream_event(StreamEvent {
            sequence,
            dedupe_key: None,
            payload,
        });
    }

    fn cwd_key(&self) -> String {
        workspace_path_key(self.cwd.as_path())
    }
}

fn workspace_path_key(path: &Path) -> String {
    normalize_workspace_path(path).to_string_lossy().to_string()
}

fn workspace_path_aliases(path: &Path) -> Vec<String> {
    let normalized = normalize_workspace_path(path);
    let normalized_text = normalized.to_string_lossy().to_string();

    #[cfg(windows)]
    {
        let mut aliases = vec![normalized_text.clone()];
        let text = aliases[0].clone();
        if let Some(legacy) = windows_verbatim_workspace_alias(text.as_str())
            && !aliases.iter().any(|alias| alias == &legacy)
        {
            aliases.push(legacy);
        }
        aliases
    }

    #[cfg(not(windows))]
    {
        vec![normalized_text]
    }
}

fn normalize_workspace_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{stripped}"));
        }
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }

    path.to_path_buf()
}

#[cfg(windows)]
fn windows_verbatim_workspace_alias(path: &str) -> Option<String> {
    if path.starts_with(r"\\?\") {
        return None;
    }
    if let Some(stripped) = path.strip_prefix(r"\\") {
        return Some(format!(r"\\?\UNC\{stripped}"));
    }
    if path.len() >= 3 && path.as_bytes()[1] == b':' && path.as_bytes()[2] == b'\\' {
        return Some(format!(r"\\?\{path}"));
    }
    None
}

fn is_rollout_fallback_item_id(item_id: &str) -> bool {
    item_id.starts_with("rollout:")
}

fn normalized_item_content(content: &str) -> &str {
    content.trim()
}
