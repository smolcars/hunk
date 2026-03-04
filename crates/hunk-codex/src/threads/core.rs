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
use codex_app_server_protocol::UserInput;
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
use crate::state::item_storage_key;
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
            thread_id: params.thread_id,
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
}
