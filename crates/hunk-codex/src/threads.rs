use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use codex_app_server_protocol::AppsListParams;
use codex_app_server_protocol::AppsListResponse;
use codex_app_server_protocol::CancelLoginAccountParams;
use codex_app_server_protocol::CancelLoginAccountResponse;
use codex_app_server_protocol::CollabAgentToolCallStatus;
use codex_app_server_protocol::CommandExecParams;
use codex_app_server_protocol::CommandExecResponse;
use codex_app_server_protocol::CommandExecutionStatus;
use codex_app_server_protocol::DynamicToolCallStatus;
use codex_app_server_protocol::GetAccountParams;
use codex_app_server_protocol::GetAccountRateLimitsResponse;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::LoginAccountParams;
use codex_app_server_protocol::LoginAccountResponse;
use codex_app_server_protocol::LogoutAccountResponse;
use codex_app_server_protocol::McpToolCallStatus;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::PatchApplyStatus;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewStartParams;
use codex_app_server_protocol::ReviewStartResponse;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::SkillsConfigWriteParams;
use codex_app_server_protocol::SkillsConfigWriteResponse;
use codex_app_server_protocol::SkillsListParams;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadArchiveResponse;
use codex_app_server_protocol::ThreadCompactStartParams;
use codex_app_server_protocol::ThreadCompactStartResponse;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadRollbackParams;
use codex_app_server_protocol::ThreadRollbackResponse;
use codex_app_server_protocol::ThreadSortKey;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadUnarchiveParams;
use codex_app_server_protocol::ThreadUnarchiveResponse;
use codex_app_server_protocol::ThreadUnsubscribeParams;
use codex_app_server_protocol::ThreadUnsubscribeResponse;
use codex_app_server_protocol::ThreadUnsubscribeStatus;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnInterruptResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::TurnSteerParams;
use codex_app_server_protocol::TurnSteerResponse;
use codex_app_server_protocol::{CollaborationModeListParams, CollaborationModeListResponse};
use codex_app_server_protocol::{ExperimentalFeatureListParams, ExperimentalFeatureListResponse};

use crate::api;
use crate::errors::CodexIntegrationError;
use crate::errors::Result;
use crate::state::ActiveThreadStore;
use crate::state::AiState;
use crate::state::ReducerEvent;
use crate::state::ServerRequestDecision;
use crate::state::StreamEvent;
use crate::state::ThreadLifecycleStatus;
use crate::state::TurnStatus as StateTurnStatus;
use crate::ws_client::JsonRpcSession;

#[derive(Debug, Clone)]
pub struct ThreadService {
    cwd: PathBuf,
    state: AiState,
    next_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutFallbackItem {
    pub kind: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutFallbackTurn {
    pub turn_id: String,
    pub completed: bool,
    pub items: Vec<RolloutFallbackItem>,
}

impl ThreadService {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            state: AiState::default(),
            next_sequence: 1,
        }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn state(&self) -> &AiState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AiState {
        &mut self.state
    }

    pub fn active_thread_for_workspace(&self) -> Option<&str> {
        self.state.active_thread_for_cwd(self.cwd_key().as_str())
    }

    pub fn hydrate_active_thread_for_workspace<S>(
        &mut self,
        store: &S,
    ) -> std::result::Result<Option<String>, S::Error>
    where
        S: ActiveThreadStore,
    {
        self.state
            .hydrate_active_thread_for_cwd(store, self.cwd_key().as_str())
    }

    pub fn persist_active_thread_for_workspace<S>(
        &mut self,
        store: &mut S,
        thread_id: String,
    ) -> std::result::Result<(), S::Error>
    where
        S: ActiveThreadStore,
    {
        self.state
            .persist_active_thread_for_cwd(store, self.cwd_key(), thread_id)
    }

    pub fn list_threads(
        &mut self,
        session: &mut JsonRpcSession,
        cursor: Option<String>,
        limit: Option<u32>,
        timeout: Duration,
    ) -> Result<ThreadListResponse> {
        let params = ThreadListParams {
            cursor,
            limit,
            sort_key: Some(ThreadSortKey::UpdatedAt),
            model_providers: None,
            source_kinds: None,
            archived: Some(false),
            cwd: Some(self.cwd_key()),
            search_term: None,
        };

        let mut response: ThreadListResponse =
            session.request_typed(api::method::THREAD_LIST, Some(&params), timeout)?;
        response
            .data
            .retain(|thread| self.thread_matches_workspace(thread));
        for thread in &response.data {
            self.ingest_thread_snapshot(thread);
        }
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn list_loaded_threads(
        &mut self,
        session: &mut JsonRpcSession,
        cursor: Option<String>,
        limit: Option<u32>,
        timeout: Duration,
    ) -> Result<ThreadLoadedListResponse> {
        let params = ThreadLoadedListParams { cursor, limit };
        let response: ThreadLoadedListResponse =
            session.request_typed(api::method::THREAD_LOADED_LIST, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn list_skills(
        &mut self,
        session: &mut JsonRpcSession,
        force_reload: bool,
        timeout: Duration,
    ) -> Result<SkillsListResponse> {
        let params = SkillsListParams {
            cwds: vec![self.cwd.clone()],
            force_reload,
            per_cwd_extra_user_roots: None,
        };
        let response: SkillsListResponse =
            session.request_typed(api::method::SKILLS_LIST, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn write_skills_config(
        &mut self,
        session: &mut JsonRpcSession,
        path: PathBuf,
        enabled: bool,
        timeout: Duration,
    ) -> Result<SkillsConfigWriteResponse> {
        let params = SkillsConfigWriteParams { path, enabled };
        let response: SkillsConfigWriteResponse =
            session.request_typed(api::method::SKILLS_CONFIG_WRITE, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn list_apps(
        &mut self,
        session: &mut JsonRpcSession,
        cursor: Option<String>,
        limit: Option<u32>,
        force_refetch: bool,
        timeout: Duration,
    ) -> Result<AppsListResponse> {
        let params = AppsListParams {
            cursor,
            limit,
            thread_id: self.active_thread_for_workspace().map(ToOwned::to_owned),
            force_refetch,
        };
        let response: AppsListResponse =
            session.request_typed(api::method::APP_LIST, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn list_models(
        &mut self,
        session: &mut JsonRpcSession,
        cursor: Option<String>,
        limit: Option<u32>,
        include_hidden: Option<bool>,
        timeout: Duration,
    ) -> Result<ModelListResponse> {
        let params = ModelListParams {
            cursor,
            limit,
            include_hidden,
        };
        let response: ModelListResponse =
            session.request_typed(api::method::MODEL_LIST, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn list_experimental_features(
        &mut self,
        session: &mut JsonRpcSession,
        cursor: Option<String>,
        limit: Option<u32>,
        timeout: Duration,
    ) -> Result<ExperimentalFeatureListResponse> {
        let params = ExperimentalFeatureListParams { cursor, limit };
        let response: ExperimentalFeatureListResponse = session.request_typed(
            api::method::EXPERIMENTAL_FEATURE_LIST,
            Some(&params),
            timeout,
        )?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn list_collaboration_modes(
        &mut self,
        session: &mut JsonRpcSession,
        timeout: Duration,
    ) -> Result<CollaborationModeListResponse> {
        let response: CollaborationModeListResponse = session.request_typed(
            api::method::COLLABORATION_MODE_LIST,
            Some(&CollaborationModeListParams::default()),
            timeout,
        )?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn read_account(
        &mut self,
        session: &mut JsonRpcSession,
        refresh_token: bool,
        timeout: Duration,
    ) -> Result<GetAccountResponse> {
        let params = GetAccountParams { refresh_token };
        let response: GetAccountResponse =
            session.request_typed(api::method::ACCOUNT_READ, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn login_account(
        &mut self,
        session: &mut JsonRpcSession,
        params: LoginAccountParams,
        timeout: Duration,
    ) -> Result<LoginAccountResponse> {
        let response: LoginAccountResponse =
            session.request_typed(api::method::ACCOUNT_LOGIN_START, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn cancel_account_login(
        &mut self,
        session: &mut JsonRpcSession,
        login_id: String,
        timeout: Duration,
    ) -> Result<CancelLoginAccountResponse> {
        let params = CancelLoginAccountParams { login_id };
        let response: CancelLoginAccountResponse =
            session.request_typed(api::method::ACCOUNT_LOGIN_CANCEL, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn logout_account(
        &mut self,
        session: &mut JsonRpcSession,
        timeout: Duration,
    ) -> Result<LogoutAccountResponse> {
        let response: LogoutAccountResponse =
            session.request_typed(api::method::ACCOUNT_LOGOUT, Option::<&()>::None, timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn read_account_rate_limits(
        &mut self,
        session: &mut JsonRpcSession,
        timeout: Duration,
    ) -> Result<GetAccountRateLimitsResponse> {
        let response: GetAccountRateLimitsResponse = session.request_typed(
            api::method::ACCOUNT_RATE_LIMITS_READ,
            Option::<&()>::None,
            timeout,
        )?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn start_thread(
        &mut self,
        session: &mut JsonRpcSession,
        mut params: ThreadStartParams,
        timeout: Duration,
    ) -> Result<ThreadStartResponse> {
        params.cwd = Some(self.cwd_key());
        let response: ThreadStartResponse =
            session.request_typed(api::method::THREAD_START, Some(&params), timeout)?;
        self.ensure_thread_in_workspace(&response.thread)?;
        self.ingest_thread_snapshot(&response.thread);
        self.select_active_thread(response.thread.id.clone());
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn resume_thread(
        &mut self,
        session: &mut JsonRpcSession,
        mut params: ThreadResumeParams,
        timeout: Duration,
    ) -> Result<ThreadResumeResponse> {
        if params.cwd.is_none() {
            params.cwd = Some(self.cwd_key());
        }
        let response: ThreadResumeResponse =
            session.request_typed(api::method::THREAD_RESUME, Some(&params), timeout)?;
        self.ensure_thread_in_workspace(&response.thread)?;
        self.replace_thread_turns_from_snapshot(&response.thread);
        self.ingest_thread_snapshot(&response.thread);
        self.select_active_thread(response.thread.id.clone());
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn fork_thread(
        &mut self,
        session: &mut JsonRpcSession,
        mut params: ThreadForkParams,
        timeout: Duration,
    ) -> Result<ThreadForkResponse> {
        if params.cwd.is_none() {
            params.cwd = Some(self.cwd_key());
        }
        let response: ThreadForkResponse =
            session.request_typed(api::method::THREAD_FORK, Some(&params), timeout)?;
        self.ensure_thread_in_workspace(&response.thread)?;
        self.replace_thread_turns_from_snapshot(&response.thread);
        self.ingest_thread_snapshot(&response.thread);
        self.select_active_thread(response.thread.id.clone());
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn read_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        include_turns: bool,
        timeout: Duration,
    ) -> Result<ThreadReadResponse> {
        let params = ThreadReadParams {
            thread_id,
            include_turns,
        };
        let response: ThreadReadResponse =
            session.request_typed(api::method::THREAD_READ, Some(&params), timeout)?;
        self.ensure_thread_in_workspace(&response.thread)?;
        if include_turns {
            self.replace_thread_turns_from_snapshot(&response.thread);
        }
        self.ingest_thread_snapshot(&response.thread);
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn start_turn(
        &mut self,
        session: &mut JsonRpcSession,
        params: TurnStartParams,
        timeout: Duration,
    ) -> Result<TurnStartResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        let response: TurnStartResponse =
            session.request_typed(api::method::TURN_START, Some(&params), timeout)?;
        self.apply_turn_snapshot(&params.thread_id, &response.turn);
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn steer_turn(
        &mut self,
        session: &mut JsonRpcSession,
        params: TurnSteerParams,
        timeout: Duration,
    ) -> Result<TurnSteerResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        let response: TurnSteerResponse =
            session.request_typed(api::method::TURN_STEER, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn interrupt_turn(
        &mut self,
        session: &mut JsonRpcSession,
        params: TurnInterruptParams,
        timeout: Duration,
    ) -> Result<TurnInterruptResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        let response: TurnInterruptResponse =
            session.request_typed(api::method::TURN_INTERRUPT, Some(&params), timeout)?;
        self.apply_event(ReducerEvent::TurnCompleted {
            turn_id: params.turn_id,
        });
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn start_review(
        &mut self,
        session: &mut JsonRpcSession,
        params: ReviewStartParams,
        timeout: Duration,
    ) -> Result<ReviewStartResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        let response: ReviewStartResponse =
            session.request_typed(api::method::REVIEW_START, Some(&params), timeout)?;
        self.ensure_local_thread(response.review_thread_id.clone());
        self.select_active_thread(response.review_thread_id.clone());
        self.apply_turn_snapshot(&response.review_thread_id, &response.turn);
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn command_exec(
        &mut self,
        session: &mut JsonRpcSession,
        mut params: CommandExecParams,
        timeout: Duration,
    ) -> Result<CommandExecResponse> {
        if params.cwd.is_none() {
            params.cwd = Some(self.cwd.clone());
        }
        let response: CommandExecResponse =
            session.request_typed(api::method::COMMAND_EXEC, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn archive_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        timeout: Duration,
    ) -> Result<ThreadArchiveResponse> {
        let params = ThreadArchiveParams {
            thread_id: thread_id.clone(),
        };
        let response: ThreadArchiveResponse =
            session.request_typed(api::method::THREAD_ARCHIVE, Some(&params), timeout)?;
        if self.is_known_thread(&thread_id) {
            self.apply_event(ReducerEvent::ThreadArchived { thread_id });
        }
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn unarchive_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        timeout: Duration,
    ) -> Result<ThreadUnarchiveResponse> {
        let params = ThreadUnarchiveParams { thread_id };
        let response: ThreadUnarchiveResponse =
            session.request_typed(api::method::THREAD_UNARCHIVE, Some(&params), timeout)?;
        self.ensure_thread_in_workspace(&response.thread)?;
        self.ingest_thread_snapshot(&response.thread);
        self.apply_event(ReducerEvent::ThreadUnarchived {
            thread_id: response.thread.id.clone(),
        });
        self.apply_queued_notifications(session);
        Ok(response)
    }

    pub fn compact_thread(
        &mut self,
        session: &mut JsonRpcSession,
        thread_id: String,
        timeout: Duration,
    ) -> Result<ThreadCompactStartResponse> {
        let params = ThreadCompactStartParams { thread_id };
        let response: ThreadCompactStartResponse =
            session.request_typed(api::method::THREAD_COMPACT_START, Some(&params), timeout)?;
        self.apply_queued_notifications(session);
        Ok(response)
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
        let response: ThreadRollbackResponse =
            session.request_typed(api::method::THREAD_ROLLBACK, Some(&params), timeout)?;
        self.ensure_thread_in_workspace(&response.thread)?;
        self.replace_thread_turns_from_snapshot(&response.thread);
        self.ingest_thread_snapshot(&response.thread);
        self.apply_queued_notifications(session);
        Ok(response)
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
        let response: ThreadUnsubscribeResponse =
            session.request_typed(api::method::THREAD_UNSUBSCRIBE, Some(&params), timeout)?;
        if matches!(
            response.status,
            ThreadUnsubscribeStatus::Unsubscribed | ThreadUnsubscribeStatus::NotLoaded
        ) && self.is_known_thread(&thread_id)
        {
            self.apply_event(ReducerEvent::ThreadStatusChanged {
                thread_id,
                status: ThreadLifecycleStatus::NotLoaded,
            });
        }
        self.apply_queued_notifications(session);
        Ok(response)
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
                        status: ThreadLifecycleStatus::NotLoaded,
                    });
                    self.complete_in_progress_turns(notification.thread_id.as_str());
                }
            }
            ServerNotification::ThreadNameUpdated(notification) => {
                if let Some(thread) = self.state.threads.get_mut(&notification.thread_id) {
                    thread.title = notification.thread_name;
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
                        turn_id: notification.turn_id,
                        diff: notification.diff,
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
                    self.apply_event(ReducerEvent::ItemCompleted { item_id });
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
                if self.state.items.contains_key(&item_id) {
                    continue;
                }

                self.apply_event(ReducerEvent::ItemStarted {
                    turn_id: turn.turn_id.clone(),
                    item_id: item_id.clone(),
                    kind: item.kind.clone(),
                });
                if !item.content.is_empty() {
                    self.apply_event(ReducerEvent::ItemDelta {
                        item_id: item_id.clone(),
                        delta: item.content.clone(),
                    });
                }
                self.apply_event(ReducerEvent::ItemCompleted { item_id });
            }

            if turn.completed {
                self.apply_event(ReducerEvent::TurnCompleted {
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
                turn_id: turn.id.clone(),
            });
        }
    }

    fn apply_item_snapshot(&mut self, thread_id: &str, turn_id: &str, item: &ThreadItem) {
        let item_id = item.id().to_string();
        let should_seed_content = self
            .state
            .items
            .get(&item_id)
            .is_none_or(|existing| existing.content.is_empty());
        self.apply_event(ReducerEvent::TurnStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
        });
        self.apply_event(ReducerEvent::ItemStarted {
            turn_id: turn_id.to_string(),
            item_id: item_id.clone(),
            kind: thread_item_kind(item).to_string(),
        });

        if should_seed_content && let Some(seed_content) = thread_item_seed_content(item) {
            self.apply_event(ReducerEvent::ItemDelta {
                item_id: item_id.clone(),
                delta: seed_content,
            });
        }

        if thread_item_is_complete(item) {
            self.apply_event(ReducerEvent::ItemCompleted { item_id });
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
            turn_id: turn_id.to_string(),
            item_id: item_id.to_string(),
            kind: kind.to_string(),
        });
        self.apply_event(ReducerEvent::ItemDelta {
            item_id: item_id.to_string(),
            delta: delta.to_string(),
        });
    }

    fn ingest_thread_snapshot(&mut self, thread: &Thread) {
        let title = thread
            .name
            .clone()
            .or_else(|| (!thread.preview.trim().is_empty()).then(|| thread.preview.clone()));
        self.apply_event(ReducerEvent::ThreadStarted {
            thread_id: thread.id.clone(),
            cwd: thread.cwd.to_string_lossy().to_string(),
            title,
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
        let removed_turn_ids: BTreeSet<String> = self
            .state
            .turns
            .values()
            .filter(|turn| turn.thread_id == thread.id && !keep_turn_ids.contains(&turn.id))
            .map(|turn| turn.id.clone())
            .collect();

        for turn_id in &removed_turn_ids {
            self.state.turns.remove(turn_id);
        }

        self.state
            .items
            .retain(|_, item| !removed_turn_ids.contains(&item.turn_id));
    }

    fn ensure_thread_in_workspace(&self, thread: &Thread) -> Result<()> {
        if self.thread_matches_workspace(thread) {
            return Ok(());
        }

        Err(CodexIntegrationError::ThreadOutsideWorkspace {
            thread_id: thread.id.clone(),
            expected_cwd: self.cwd_key(),
            actual_cwd: thread.cwd.to_string_lossy().to_string(),
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
            self.apply_event(ReducerEvent::TurnCompleted { turn_id });
        }
    }

    fn thread_matches_workspace(&self, thread: &Thread) -> bool {
        thread.cwd == self.cwd
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
        self.cwd.to_string_lossy().to_string()
    }
}

fn lifecycle_status_from_thread_status(status: &ThreadStatus) -> ThreadLifecycleStatus {
    match status {
        ThreadStatus::NotLoaded => ThreadLifecycleStatus::NotLoaded,
        ThreadStatus::Idle | ThreadStatus::SystemError | ThreadStatus::Active { .. } => {
            ThreadLifecycleStatus::Active
        }
    }
}

fn thread_item_kind(item: &ThreadItem) -> &'static str {
    match item {
        ThreadItem::UserMessage { .. } => "userMessage",
        ThreadItem::AgentMessage { .. } => "agentMessage",
        ThreadItem::Plan { .. } => "plan",
        ThreadItem::Reasoning { .. } => "reasoning",
        ThreadItem::CommandExecution { .. } => "commandExecution",
        ThreadItem::FileChange { .. } => "fileChange",
        ThreadItem::McpToolCall { .. } => "mcpToolCall",
        ThreadItem::DynamicToolCall { .. } => "dynamicToolCall",
        ThreadItem::CollabAgentToolCall { .. } => "collabAgentToolCall",
        ThreadItem::WebSearch { .. } => "webSearch",
        ThreadItem::ImageView { .. } => "imageView",
        ThreadItem::EnteredReviewMode { .. } => "enteredReviewMode",
        ThreadItem::ExitedReviewMode { .. } => "exitedReviewMode",
        ThreadItem::ContextCompaction { .. } => "contextCompaction",
    }
}

fn thread_item_seed_content(item: &ThreadItem) -> Option<String> {
    match item {
        ThreadItem::AgentMessage { text, .. } | ThreadItem::Plan { text, .. } => {
            (!text.is_empty()).then(|| text.clone())
        }
        ThreadItem::Reasoning {
            summary, content, ..
        } => {
            let mut parts = String::new();
            if !summary.is_empty() {
                parts.push_str(&summary.join(""));
            }
            if !content.is_empty() {
                parts.push_str(&content.join(""));
            }
            (!parts.is_empty()).then_some(parts)
        }
        ThreadItem::CommandExecution {
            aggregated_output, ..
        } => aggregated_output.clone().filter(|value| !value.is_empty()),
        ThreadItem::FileChange { changes, .. } => {
            let joined = changes
                .iter()
                .map(|change| change.diff.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            (!joined.is_empty()).then_some(joined)
        }
        ThreadItem::McpToolCall { error, .. } => error.as_ref().map(|value| value.message.clone()),
        ThreadItem::EnteredReviewMode { review, .. }
        | ThreadItem::ExitedReviewMode { review, .. } => {
            (!review.is_empty()).then(|| review.clone())
        }
        ThreadItem::DynamicToolCall { .. }
        | ThreadItem::CollabAgentToolCall { .. }
        | ThreadItem::WebSearch { .. }
        | ThreadItem::ImageView { .. }
        | ThreadItem::ContextCompaction { .. }
        | ThreadItem::UserMessage { .. } => None,
    }
}

fn thread_item_is_complete(item: &ThreadItem) -> bool {
    match item {
        ThreadItem::CommandExecution { status, .. } => {
            !matches!(status, CommandExecutionStatus::InProgress)
        }
        ThreadItem::FileChange { status, .. } => !matches!(status, PatchApplyStatus::InProgress),
        ThreadItem::McpToolCall { status, .. } => !matches!(status, McpToolCallStatus::InProgress),
        ThreadItem::DynamicToolCall { status, .. } => {
            !matches!(status, DynamicToolCallStatus::InProgress)
        }
        ThreadItem::CollabAgentToolCall { status, .. } => {
            !matches!(status, CollabAgentToolCallStatus::InProgress)
        }
        _ => false,
    }
}

fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        RequestId::Integer(value) => value.to_string(),
        RequestId::String(value) => value.clone(),
    }
}
