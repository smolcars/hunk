use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use crate::protocol::AppsListParams;
use crate::protocol::AppsListResponse;
use crate::protocol::CancelLoginAccountParams;
use crate::protocol::CancelLoginAccountResponse;
use crate::protocol::CollabAgentToolCallStatus;
use crate::protocol::CommandExecParams;
use crate::protocol::CommandExecResponse;
use crate::protocol::CommandExecutionStatus;
use crate::protocol::DynamicToolCallStatus;
use crate::protocol::GetAccountParams;
use crate::protocol::GetAccountRateLimitsResponse;
use crate::protocol::GetAccountResponse;
use crate::protocol::LoginAccountParams;
use crate::protocol::LoginAccountResponse;
use crate::protocol::LogoutAccountResponse;
use crate::protocol::McpToolCallStatus;
use crate::protocol::ModelListParams;
use crate::protocol::ModelListResponse;
use crate::protocol::PatchApplyStatus;
use crate::protocol::RequestId;
use crate::protocol::ReviewStartParams;
use crate::protocol::ReviewStartResponse;
use crate::protocol::ServerNotification;
use crate::protocol::SkillsConfigWriteParams;
use crate::protocol::SkillsConfigWriteResponse;
use crate::protocol::SkillsListParams;
use crate::protocol::SkillsListResponse;
use crate::protocol::Thread;
use crate::protocol::ThreadArchiveParams;
use crate::protocol::ThreadArchiveResponse;
use crate::protocol::ThreadCompactStartParams;
use crate::protocol::ThreadCompactStartResponse;
use crate::protocol::ThreadForkParams;
use crate::protocol::ThreadForkResponse;
use crate::protocol::ThreadItem;
use crate::protocol::ThreadListCwdFilter;
use crate::protocol::ThreadListParams;
use crate::protocol::ThreadListResponse;
use crate::protocol::ThreadLoadedListParams;
use crate::protocol::ThreadLoadedListResponse;
use crate::protocol::ThreadReadParams;
use crate::protocol::ThreadReadResponse;
use crate::protocol::ThreadResumeParams;
use crate::protocol::ThreadResumeResponse;
use crate::protocol::ThreadRollbackParams;
use crate::protocol::ThreadRollbackResponse;
use crate::protocol::ThreadSortKey;
use crate::protocol::ThreadStartParams;
use crate::protocol::ThreadStartResponse;
use crate::protocol::ThreadStatus;
use crate::protocol::ThreadUnarchiveParams;
use crate::protocol::ThreadUnarchiveResponse;
use crate::protocol::ThreadUnsubscribeParams;
use crate::protocol::ThreadUnsubscribeResponse;
use crate::protocol::ThreadUnsubscribeStatus;
use crate::protocol::TurnInterruptParams;
use crate::protocol::TurnInterruptResponse;
use crate::protocol::TurnStartParams;
use crate::protocol::TurnStartResponse;
use crate::protocol::TurnStatus;
use crate::protocol::TurnSteerParams;
use crate::protocol::TurnSteerResponse;
use crate::protocol::UserInput;
use crate::protocol::{CollaborationModeListParams, CollaborationModeListResponse};
use crate::protocol::{ExperimentalFeatureListParams, ExperimentalFeatureListResponse};
use serde::Serialize;
use serde::de::DeserializeOwned;

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
use crate::app_server_client::AppServerClient;

const BUFFERED_NOTIFICATION_DRAIN_TIMEOUT: Duration = Duration::ZERO;

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
            cwd: normalize_workspace_path(cwd.as_path()),
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

    fn request_with_notifications<P, R>(
        &mut self,
        session: &mut impl AppServerClient,
        method: &str,
        params: Option<&P>,
        timeout: Duration,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        self.request_and_reconcile(session, method, params, timeout, |_, _| Ok(()))
    }

    fn request_and_reconcile<P, R, F>(
        &mut self,
        session: &mut impl AppServerClient,
        method: &str,
        params: Option<&P>,
        timeout: Duration,
        reconcile: F,
    ) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
        F: FnOnce(&mut Self, &mut R) -> Result<()>,
    {
        let mut response: R = session.request_typed(method, params, timeout)?;
        reconcile(self, &mut response)?;
        self.apply_buffered_notifications(session)?;
        Ok(response)
    }

    fn apply_buffered_notifications(
        &mut self,
        session: &mut impl AppServerClient,
    ) -> Result<()> {
        for notification in
            session.drain_buffered_notifications(BUFFERED_NOTIFICATION_DRAIN_TIMEOUT)?
        {
            self.apply_server_notification(notification);
        }
        Ok(())
    }

    pub fn list_threads(
        &mut self,
        session: &mut impl AppServerClient,
        cursor: Option<String>,
        limit: Option<u32>,
        timeout: Duration,
    ) -> Result<ThreadListResponse> {
        let mut aliases = workspace_path_aliases(self.cwd.as_path()).into_iter();
        let primary_cwd = aliases.next().unwrap_or_else(|| self.cwd_key());
        let mut response =
            self.list_threads_for_cwd(session, cursor.clone(), limit, primary_cwd, timeout)?;
        let mut merged_alias_results = false;

        if cursor.is_none() {
            for alias in aliases {
                let alias_response = self.list_threads_for_cwd(session, None, limit, alias, timeout)?;
                merge_thread_list_response(&mut response, alias_response);
                merged_alias_results = true;
            }
        }

        if merged_alias_results {
            sort_threads_for_workspace_list(&mut response.data);
            if let Some(limit) = limit {
                response.data.truncate(limit as usize);
            }
        }

        Ok(response)
    }

    fn list_threads_for_cwd(
        &mut self,
        session: &mut impl AppServerClient,
        cursor: Option<String>,
        limit: Option<u32>,
        cwd: String,
        timeout: Duration,
    ) -> Result<ThreadListResponse> {
        let params = ThreadListParams {
            cursor,
            limit,
            sort_key: Some(ThreadSortKey::UpdatedAt),
            sort_direction: None,
            model_providers: None,
            source_kinds: None,
            archived: Some(false),
            cwd: Some(ThreadListCwdFilter::One(cwd)),
            use_state_db_only: false,
            search_term: None,
        };

        self.request_and_reconcile(
            session,
            api::method::THREAD_LIST,
            Some(&params),
            timeout,
            |service, response: &mut ThreadListResponse| {
                response
                    .data
                    .retain(|thread| service.thread_matches_workspace(thread));
                for thread in &response.data {
                    service.ingest_thread_snapshot(thread);
                }
                Ok(())
            },
        )
    }

    pub fn list_loaded_threads(
        &mut self,
        session: &mut impl AppServerClient,
        cursor: Option<String>,
        limit: Option<u32>,
        timeout: Duration,
    ) -> Result<ThreadLoadedListResponse> {
        let params = ThreadLoadedListParams { cursor, limit };
        self.request_with_notifications(
            session,
            api::method::THREAD_LOADED_LIST,
            Some(&params),
            timeout,
        )
    }

    pub fn list_skills(
        &mut self,
        session: &mut impl AppServerClient,
        force_reload: bool,
        timeout: Duration,
    ) -> Result<SkillsListResponse> {
        let params = SkillsListParams {
            cwds: vec![self.cwd.clone()],
            force_reload,
            per_cwd_extra_user_roots: None,
        };
        self.request_with_notifications(session, api::method::SKILLS_LIST, Some(&params), timeout)
    }

    pub fn write_skills_config(
        &mut self,
        session: &mut impl AppServerClient,
        path: PathBuf,
        enabled: bool,
        timeout: Duration,
    ) -> Result<SkillsConfigWriteResponse> {
        let absolute_path = path
            .clone()
            .try_into()
            .map_err(|source| CodexIntegrationError::InvalidPath {
                path: path.display().to_string(),
                source,
            })?;
        let params = SkillsConfigWriteParams {
            path: Some(absolute_path),
            name: None,
            enabled,
        };
        self.request_with_notifications(
            session,
            api::method::SKILLS_CONFIG_WRITE,
            Some(&params),
            timeout,
        )
    }

    pub fn list_apps(
        &mut self,
        session: &mut impl AppServerClient,
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
        self.request_with_notifications(session, api::method::APP_LIST, Some(&params), timeout)
    }

    pub fn list_models(
        &mut self,
        session: &mut impl AppServerClient,
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
        self.request_with_notifications(session, api::method::MODEL_LIST, Some(&params), timeout)
    }

    pub fn list_experimental_features(
        &mut self,
        session: &mut impl AppServerClient,
        cursor: Option<String>,
        limit: Option<u32>,
        timeout: Duration,
    ) -> Result<ExperimentalFeatureListResponse> {
        let params = ExperimentalFeatureListParams { cursor, limit };
        self.request_with_notifications(
            session,
            api::method::EXPERIMENTAL_FEATURE_LIST,
            Some(&params),
            timeout,
        )
    }

    pub fn list_collaboration_modes(
        &mut self,
        session: &mut impl AppServerClient,
        timeout: Duration,
    ) -> Result<CollaborationModeListResponse> {
        self.request_with_notifications(
            session,
            api::method::COLLABORATION_MODE_LIST,
            Some(&CollaborationModeListParams::default()),
            timeout,
        )
    }

    pub fn read_account(
        &mut self,
        session: &mut impl AppServerClient,
        refresh_token: bool,
        timeout: Duration,
    ) -> Result<GetAccountResponse> {
        let params = GetAccountParams { refresh_token };
        self.request_with_notifications(session, api::method::ACCOUNT_READ, Some(&params), timeout)
    }

    pub fn login_account(
        &mut self,
        session: &mut impl AppServerClient,
        params: LoginAccountParams,
        timeout: Duration,
    ) -> Result<LoginAccountResponse> {
        self.request_with_notifications(
            session,
            api::method::ACCOUNT_LOGIN_START,
            Some(&params),
            timeout,
        )
    }

    pub fn cancel_account_login(
        &mut self,
        session: &mut impl AppServerClient,
        login_id: String,
        timeout: Duration,
    ) -> Result<CancelLoginAccountResponse> {
        let params = CancelLoginAccountParams { login_id };
        self.request_with_notifications(
            session,
            api::method::ACCOUNT_LOGIN_CANCEL,
            Some(&params),
            timeout,
        )
    }

    pub fn logout_account(
        &mut self,
        session: &mut impl AppServerClient,
        timeout: Duration,
    ) -> Result<LogoutAccountResponse> {
        self.request_with_notifications(
            session,
            api::method::ACCOUNT_LOGOUT,
            Option::<&()>::None,
            timeout,
        )
    }

    pub fn read_account_rate_limits(
        &mut self,
        session: &mut impl AppServerClient,
        timeout: Duration,
    ) -> Result<GetAccountRateLimitsResponse> {
        self.request_with_notifications(
            session,
            api::method::ACCOUNT_RATE_LIMITS_READ,
            Option::<&()>::None,
            timeout,
        )
    }

    pub fn start_thread(
        &mut self,
        session: &mut impl AppServerClient,
        mut params: ThreadStartParams,
        timeout: Duration,
    ) -> Result<ThreadStartResponse> {
        params.cwd = Some(self.cwd_key());
        self.request_and_reconcile(
            session,
            api::method::THREAD_START,
            Some(&params),
            timeout,
            |service, response: &mut ThreadStartResponse| {
                service.ensure_thread_in_workspace(&response.thread)?;
                service.ingest_thread_snapshot(&response.thread);
                service.select_active_thread(response.thread.id.clone());
                Ok(())
            },
        )
    }

    pub fn resume_thread(
        &mut self,
        session: &mut impl AppServerClient,
        mut params: ThreadResumeParams,
        timeout: Duration,
    ) -> Result<ThreadResumeResponse> {
        if params.cwd.is_none() {
            params.cwd = Some(self.cwd_key());
        }
        self.request_and_reconcile(
            session,
            api::method::THREAD_RESUME,
            Some(&params),
            timeout,
            |service, response: &mut ThreadResumeResponse| {
                service.ensure_thread_in_workspace(&response.thread)?;
                service.replace_thread_turns_from_snapshot(&response.thread);
                service.ingest_thread_snapshot(&response.thread);
                service.select_active_thread(response.thread.id.clone());
                Ok(())
            },
        )
    }

    pub fn fork_thread(
        &mut self,
        session: &mut impl AppServerClient,
        mut params: ThreadForkParams,
        timeout: Duration,
    ) -> Result<ThreadForkResponse> {
        if params.cwd.is_none() {
            params.cwd = Some(self.cwd_key());
        }
        self.request_and_reconcile(
            session,
            api::method::THREAD_FORK,
            Some(&params),
            timeout,
            |service, response: &mut ThreadForkResponse| {
                service.ensure_thread_in_workspace(&response.thread)?;
                service.replace_thread_turns_from_snapshot(&response.thread);
                service.ingest_thread_snapshot(&response.thread);
                service.select_active_thread(response.thread.id.clone());
                Ok(())
            },
        )
    }

    pub fn read_thread(
        &mut self,
        session: &mut impl AppServerClient,
        thread_id: String,
        include_turns: bool,
        timeout: Duration,
    ) -> Result<ThreadReadResponse> {
        let params = ThreadReadParams {
            thread_id,
            include_turns,
        };
        self.request_and_reconcile(
            session,
            api::method::THREAD_READ,
            Some(&params),
            timeout,
            |service, response: &mut ThreadReadResponse| {
                service.ensure_thread_in_workspace(&response.thread)?;
                if include_turns {
                    service.replace_thread_turns_from_snapshot(&response.thread);
                }
                service.ingest_thread_snapshot(&response.thread);
                Ok(())
            },
        )
    }

    pub fn start_turn(
        &mut self,
        session: &mut impl AppServerClient,
        params: TurnStartParams,
        timeout: Duration,
    ) -> Result<TurnStartResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        let thread_id = params.thread_id.clone();
        self.request_and_reconcile(
            session,
            api::method::TURN_START,
            Some(&params),
            timeout,
            move |service, response: &mut TurnStartResponse| {
                service.apply_turn_snapshot(&thread_id, &response.turn);
                Ok(())
            },
        )
    }

    pub fn steer_turn(
        &mut self,
        session: &mut impl AppServerClient,
        params: TurnSteerParams,
        timeout: Duration,
    ) -> Result<TurnSteerResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        self.request_with_notifications(session, api::method::TURN_STEER, Some(&params), timeout)
    }

    pub fn interrupt_turn(
        &mut self,
        session: &mut impl AppServerClient,
        params: TurnInterruptParams,
        timeout: Duration,
    ) -> Result<TurnInterruptResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        let thread_id = params.thread_id.clone();
        let turn_id = params.turn_id.clone();
        self.request_and_reconcile(
            session,
            api::method::TURN_INTERRUPT,
            Some(&params),
            timeout,
            move |service, _: &mut TurnInterruptResponse| {
                service.apply_event(ReducerEvent::TurnCompleted { thread_id, turn_id });
                Ok(())
            },
        )
    }

    pub fn start_review(
        &mut self,
        session: &mut impl AppServerClient,
        params: ReviewStartParams,
        timeout: Duration,
    ) -> Result<ReviewStartResponse> {
        self.ensure_thread_id_in_workspace(&params.thread_id)?;
        self.request_and_reconcile(
            session,
            api::method::REVIEW_START,
            Some(&params),
            timeout,
            |service, response: &mut ReviewStartResponse| {
                service.ensure_local_thread(response.review_thread_id.clone());
                service.select_active_thread(response.review_thread_id.clone());
                service.apply_turn_snapshot(&response.review_thread_id, &response.turn);
                Ok(())
            },
        )
    }

    pub fn command_exec(
        &mut self,
        session: &mut impl AppServerClient,
        mut params: CommandExecParams,
        timeout: Duration,
    ) -> Result<CommandExecResponse> {
        if params.cwd.is_none() {
            params.cwd = Some(self.cwd.clone());
        }
        self.request_with_notifications(session, api::method::COMMAND_EXEC, Some(&params), timeout)
    }
}

fn merge_thread_list_response(
    response: &mut ThreadListResponse,
    alias_response: ThreadListResponse,
) {
    for thread in alias_response.data {
        if response.data.iter().any(|existing| existing.id == thread.id) {
            continue;
        }
        response.data.push(thread);
    }
}

fn sort_threads_for_workspace_list(threads: &mut [Thread]) {
    threads.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
}
