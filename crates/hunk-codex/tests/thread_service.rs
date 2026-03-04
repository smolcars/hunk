use std::net::TcpListener;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::CommandExecParams;
use codex_app_server_protocol::CommandExecResponse;
use codex_app_server_protocol::ErrorNotification;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::LoginAccountParams;
use codex_app_server_protocol::LoginAccountResponse;
use codex_app_server_protocol::LogoutAccountResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewStartParams;
use codex_app_server_protocol::ReviewStartResponse;
use codex_app_server_protocol::ReviewTarget;
use codex_app_server_protocol::SandboxPolicy;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequestResolvedNotification;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::SkillsConfigWriteResponse;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadArchiveResponse;
use codex_app_server_protocol::ThreadClosedNotification;
use codex_app_server_protocol::ThreadCompactStartResponse;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadRollbackResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::ThreadStatusChangedNotification;
use codex_app_server_protocol::ThreadUnarchiveResponse;
use codex_app_server_protocol::ThreadUnsubscribeResponse;
use codex_app_server_protocol::ThreadUnsubscribeStatus;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnError;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnInterruptResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::TurnSteerParams;
use codex_app_server_protocol::TurnSteerResponse;
use codex_app_server_protocol::UserInput;
use codex_app_server_protocol::{AppInfo, AppsListResponse};
use codex_app_server_protocol::{CancelLoginAccountStatus, GetAccountRateLimitsResponse};
use codex_app_server_protocol::{GetAccountResponse, RateLimitSnapshot, RateLimitWindow};
use hunk_codex::api;
use hunk_codex::api::InitializeOptions;
use hunk_codex::errors::CodexIntegrationError;
use hunk_codex::state::ReducerEvent;
use hunk_codex::state::ServerRequestDecision;
use hunk_codex::state::StreamEvent;
use hunk_codex::state::ThreadLifecycleStatus;
use hunk_codex::threads::RolloutFallbackItem;
use hunk_codex::threads::RolloutFallbackTurn;
use hunk_codex::threads::ThreadService;
use hunk_codex::ws_client::JsonRpcSession;
use hunk_codex::ws_client::WebSocketEndpoint;
use serde_json::Value;
use tungstenite::Message;
use tungstenite::WebSocket;
use tungstenite::accept;

const WORKSPACE_CWD: &str = "/repo-a";
const OTHER_CWD: &str = "/repo-b";
const TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn listing_threads_is_scoped_to_current_workspace_cwd() {
    let server = TestServer::spawn(Scenario::ListThreadsScoped);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let response = service
        .list_threads(&mut session, None, Some(50), TIMEOUT)
        .expect("thread/list should succeed");

    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].id, "thread-in-workspace");
    assert!(service.state().threads.contains_key("thread-in-workspace"));
    assert!(
        !service
            .state()
            .threads
            .contains_key("thread-outside-workspace")
    );

    server.join();
}

#[test]
fn resume_external_thread_updates_active_workspace_thread() {
    let server = TestServer::spawn(Scenario::ResumeExternalThread);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let response = service
        .resume_thread(
            &mut session,
            ThreadResumeParams {
                thread_id: "external-thread".to_string(),
                ..ThreadResumeParams::default()
            },
            TIMEOUT,
        )
        .expect("thread/resume should succeed");

    assert_eq!(response.thread.id, "external-thread");
    assert_eq!(
        service.active_thread_for_workspace(),
        Some("external-thread")
    );
    assert!(service.state().turns.contains_key("resume-turn-1"));
    assert!(service.state().items.contains_key("resume-item-1"));
    assert_eq!(
        service
            .state()
            .items
            .get("resume-item-1")
            .expect("resume item should exist")
            .content,
        "resume prompt"
    );

    server.join();
}

#[test]
fn unsubscribe_semantics_apply_not_loaded_status_on_unsubscribed() {
    let server = TestServer::spawn(Scenario::UnsubscribeSemantics);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");
    assert_eq!(
        service
            .state()
            .threads
            .get("thread-unsub")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::Idle
    );

    let first = service
        .unsubscribe_thread(&mut session, "thread-unsub".to_string(), TIMEOUT)
        .expect("first unsubscribe should succeed");
    assert_eq!(first.status, ThreadUnsubscribeStatus::NotSubscribed);
    assert_eq!(
        service
            .state()
            .threads
            .get("thread-unsub")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::Idle
    );

    let second = service
        .unsubscribe_thread(&mut session, "thread-unsub".to_string(), TIMEOUT)
        .expect("second unsubscribe should succeed");
    assert_eq!(second.status, ThreadUnsubscribeStatus::Unsubscribed);
    assert_eq!(
        service
            .state()
            .threads
            .get("thread-unsub")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::NotLoaded
    );

    server.join();
}

#[test]
fn archive_and_unarchive_round_trip_updates_state() {
    let server = TestServer::spawn(Scenario::ArchiveRoundTrip);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");

    service
        .archive_thread(&mut session, "thread-archive".to_string(), TIMEOUT)
        .expect("thread/archive should succeed");
    assert_eq!(
        service
            .state()
            .threads
            .get("thread-archive")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::Archived
    );

    service
        .unarchive_thread(&mut session, "thread-archive".to_string(), TIMEOUT)
        .expect("thread/unarchive should succeed");
    assert_eq!(
        service
            .state()
            .threads
            .get("thread-archive")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::Idle
    );

    server.join();
}

#[test]
fn compact_start_applies_buffered_notifications_before_return() {
    let server = TestServer::spawn(Scenario::CompactWithBufferedNotification);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");

    service
        .compact_thread(&mut session, "thread-compact".to_string(), TIMEOUT)
        .expect("thread/compact/start should succeed");
    assert_eq!(
        service
            .state()
            .threads
            .get("thread-compact")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::Archived
    );

    server.join();
}

#[test]
fn rollback_replaces_turn_set_and_prunes_removed_items() {
    let server = TestServer::spawn(Scenario::RollbackPrunesTurns);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .resume_thread(
            &mut session,
            ThreadResumeParams {
                thread_id: "thread-rollback".to_string(),
                ..ThreadResumeParams::default()
            },
            TIMEOUT,
        )
        .expect("thread/resume should succeed");

    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 999,
        dedupe_key: Some("item-start:item-removed".to_string()),
        payload: ReducerEvent::ItemStarted {
            turn_id: "turn-removed".to_string(),
            item_id: "item-removed".to_string(),
            kind: "agentMessage".to_string(),
        },
    });

    assert!(service.state().turns.contains_key("turn-removed"));
    assert!(service.state().items.contains_key("item-removed"));

    service
        .rollback_thread(&mut session, "thread-rollback".to_string(), 1, TIMEOUT)
        .expect("thread/rollback should succeed");

    assert!(service.state().turns.contains_key("turn-keep"));
    assert!(!service.state().turns.contains_key("turn-removed"));
    assert!(!service.state().items.contains_key("item-removed"));

    server.join();
}

#[test]
fn loaded_list_read_and_fork_are_wired_with_workspace_cwd() {
    let server = TestServer::spawn(Scenario::LoadedReadFork);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let loaded = service
        .list_loaded_threads(&mut session, None, Some(10), TIMEOUT)
        .expect("thread/loaded/list should succeed");
    assert_eq!(loaded.data, vec!["thread-read".to_string()]);

    let read = service
        .read_thread(&mut session, "thread-read".to_string(), true, TIMEOUT)
        .expect("thread/read should succeed");
    assert_eq!(read.thread.id, "thread-read");
    assert!(service.state().turns.contains_key("read-turn-1"));

    let fork = service
        .fork_thread(
            &mut session,
            ThreadForkParams {
                thread_id: "thread-read".to_string(),
                ..ThreadForkParams::default()
            },
            TIMEOUT,
        )
        .expect("thread/fork should succeed");
    assert_eq!(fork.thread.id, "thread-forked");
    assert_eq!(service.active_thread_for_workspace(), Some("thread-forked"));

    server.join();
}

#[test]
fn skills_and_apps_metadata_endpoints_are_wired() {
    let server = TestServer::spawn(Scenario::SkillsAndAppsMetadata);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let skills = service
        .list_skills(&mut session, true, TIMEOUT)
        .expect("skills/list should succeed");
    assert_eq!(skills.data.len(), 1);
    assert_eq!(
        skills.data[0].skills[0].name,
        "repo-workspace-skill".to_string()
    );

    let write_result = service
        .write_skills_config(
            &mut session,
            PathBuf::from(format!(
                "{WORKSPACE_CWD}/.codex/skills/repo-workspace-skill/SKILL.md"
            )),
            false,
            TIMEOUT,
        )
        .expect("skills/config/write should succeed");
    assert!(!write_result.effective_enabled);

    let apps = service
        .list_apps(
            &mut session,
            Some("cursor-1".to_string()),
            Some(5),
            true,
            TIMEOUT,
        )
        .expect("app/list should succeed");
    assert_eq!(apps.data.len(), 1);
    assert_eq!(apps.data[0].id, "github".to_string());
    assert_eq!(apps.next_cursor, Some("next-cursor".to_string()));

    server.join();
}

#[test]
fn account_endpoints_are_wired() {
    let server = TestServer::spawn(Scenario::AccountEndpoints);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let account = service
        .read_account(&mut session, false, TIMEOUT)
        .expect("account/read should succeed");
    assert!(account.account.is_none());
    assert!(account.requires_openai_auth);

    let login = service
        .login_account(&mut session, LoginAccountParams::Chatgpt, TIMEOUT)
        .expect("account/login/start should succeed");
    match login {
        LoginAccountResponse::Chatgpt { login_id, auth_url } => {
            assert_eq!(login_id, "login-1".to_string());
            assert_eq!(auth_url, "https://auth.example/login".to_string());
        }
        other => panic!("expected chatgpt login response, got: {other:?}"),
    }

    let cancel = service
        .cancel_account_login(&mut session, "login-1".to_string(), TIMEOUT)
        .expect("account/login/cancel should succeed");
    assert_eq!(cancel.status, CancelLoginAccountStatus::Canceled);

    let rate_limits = service
        .read_account_rate_limits(&mut session, TIMEOUT)
        .expect("account/rateLimits/read should succeed");
    assert_eq!(
        rate_limits
            .rate_limits
            .primary
            .as_ref()
            .expect("primary rate limit should exist")
            .used_percent,
        42
    );

    service
        .logout_account(&mut session, TIMEOUT)
        .expect("account/logout should succeed");

    server.join();
}

#[test]
fn model_and_session_metadata_endpoints_are_wired() {
    let server = TestServer::spawn(Scenario::ModelAndSessionMetadata);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let first_page = service
        .list_models(&mut session, None, Some(1), Some(false), TIMEOUT)
        .expect("first model/list should succeed");
    assert_eq!(first_page.data.len(), 1);
    assert_eq!(first_page.data[0].id, "model-visible");
    assert!(!first_page.data[0].hidden);
    assert_eq!(first_page.next_cursor, Some("cursor-hidden".to_string()));

    let second_page = service
        .list_models(
            &mut session,
            first_page.next_cursor.clone(),
            Some(1),
            Some(true),
            TIMEOUT,
        )
        .expect("second model/list should succeed");
    assert_eq!(second_page.data.len(), 1);
    assert_eq!(second_page.data[0].id, "model-hidden");
    assert!(second_page.data[0].hidden);
    assert_eq!(second_page.next_cursor, None);

    let features = service
        .list_experimental_features(&mut session, None, Some(20), TIMEOUT)
        .expect("experimentalFeature/list should succeed");
    assert_eq!(features.data.len(), 1);
    assert_eq!(features.data[0].name, "collaboration_modes".to_string());
    assert!(features.data[0].enabled);

    let modes = service
        .list_collaboration_modes(&mut session, TIMEOUT)
        .expect("collaborationMode/list should succeed");
    assert_eq!(modes.data.len(), 1);
    assert_eq!(modes.data[0].name, "Plan".to_string());
    assert_eq!(modes.data[0].model, Some("gpt-5-codex".to_string()));

    server.join();
}

#[test]
fn unknown_thread_status_notification_is_ignored() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service.apply_server_notification(ServerNotification::ThreadStatusChanged(
        ThreadStatusChangedNotification {
            thread_id: "unknown-thread".to_string(),
            status: ThreadStatus::NotLoaded,
        },
    ));

    assert!(service.state().threads.is_empty());
}

#[test]
fn known_thread_not_loaded_status_is_preserved() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::ThreadStarted {
            thread_id: "thread-known".to_string(),
            cwd: WORKSPACE_CWD.to_string(),
            title: None,
            updated_at: Some(42),
        },
    });

    service.apply_server_notification(ServerNotification::ThreadStatusChanged(
        ThreadStatusChangedNotification {
            thread_id: "thread-known".to_string(),
            status: ThreadStatus::NotLoaded,
        },
    ));

    assert_eq!(
        service
            .state()
            .threads
            .get("thread-known")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::NotLoaded
    );
}

#[test]
fn idle_status_notification_completes_in_progress_turns() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::ThreadStarted {
            thread_id: "thread-known".to_string(),
            cwd: WORKSPACE_CWD.to_string(),
            title: None,
            updated_at: None,
        },
    });
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::TurnStarted {
            thread_id: "thread-known".to_string(),
            turn_id: "turn-known".to_string(),
        },
    });

    service.apply_server_notification(ServerNotification::ThreadStatusChanged(
        ThreadStatusChangedNotification {
            thread_id: "thread-known".to_string(),
            status: ThreadStatus::Idle,
        },
    ));

    assert_eq!(
        service
            .state()
            .turns
            .get("turn-known")
            .expect("turn should exist")
            .status,
        hunk_codex::state::TurnStatus::Completed
    );
}

#[test]
fn thread_closed_notification_marks_not_loaded_and_completes_in_progress_turns() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::ThreadStarted {
            thread_id: "thread-known".to_string(),
            cwd: WORKSPACE_CWD.to_string(),
            title: None,
            updated_at: None,
        },
    });
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::TurnStarted {
            thread_id: "thread-known".to_string(),
            turn_id: "turn-known".to_string(),
        },
    });

    service.apply_server_notification(ServerNotification::ThreadClosed(ThreadClosedNotification {
        thread_id: "thread-known".to_string(),
    }));

    assert_eq!(
        service
            .state()
            .threads
            .get("thread-known")
            .expect("thread should exist")
            .status,
        ThreadLifecycleStatus::NotLoaded
    );
    assert_eq!(
        service
            .state()
            .turns
            .get("turn-known")
            .expect("turn should exist")
            .status,
        hunk_codex::state::TurnStatus::Completed
    );
}

#[test]
fn non_retryable_error_notification_completes_turn() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::ThreadStarted {
            thread_id: "thread-known".to_string(),
            cwd: WORKSPACE_CWD.to_string(),
            title: None,
            updated_at: None,
        },
    });
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::TurnStarted {
            thread_id: "thread-known".to_string(),
            turn_id: "turn-known".to_string(),
        },
    });

    service.apply_server_notification(ServerNotification::Error(ErrorNotification {
        error: TurnError {
            message: "upstream error".to_string(),
            codex_error_info: None,
            additional_details: None,
        },
        will_retry: false,
        thread_id: "thread-known".to_string(),
        turn_id: "turn-known".to_string(),
    }));

    assert_eq!(
        service
            .state()
            .turns
            .get("turn-known")
            .expect("turn should exist")
            .status,
        hunk_codex::state::TurnStatus::Completed
    );
}

#[test]
fn retryable_error_notification_keeps_turn_in_progress() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::ThreadStarted {
            thread_id: "thread-known".to_string(),
            cwd: WORKSPACE_CWD.to_string(),
            title: None,
            updated_at: None,
        },
    });
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::TurnStarted {
            thread_id: "thread-known".to_string(),
            turn_id: "turn-known".to_string(),
        },
    });

    service.apply_server_notification(ServerNotification::Error(ErrorNotification {
        error: TurnError {
            message: "transient overload".to_string(),
            codex_error_info: None,
            additional_details: None,
        },
        will_retry: true,
        thread_id: "thread-known".to_string(),
        turn_id: "turn-known".to_string(),
    }));

    assert_eq!(
        service
            .state()
            .turns
            .get("turn-known")
            .expect("turn should exist")
            .status,
        hunk_codex::state::TurnStatus::InProgress
    );
}

#[test]
fn rollout_fallback_history_is_ingested_into_turn_items() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::ThreadStarted {
            thread_id: "thread-known".to_string(),
            cwd: WORKSPACE_CWD.to_string(),
            title: None,
            updated_at: None,
        },
    });

    service.ingest_rollout_fallback_history(
        "thread-known".to_string(),
        &[RolloutFallbackTurn {
            turn_id: "turn-known".to_string(),
            completed: true,
            items: vec![
                RolloutFallbackItem {
                    kind: "userMessage".to_string(),
                    content: "hello".to_string(),
                },
                RolloutFallbackItem {
                    kind: "agentMessage".to_string(),
                    content: "world".to_string(),
                },
            ],
        }],
    );

    assert_eq!(
        service
            .state()
            .turns
            .get("turn-known")
            .expect("turn should exist")
            .status,
        hunk_codex::state::TurnStatus::Completed
    );
    assert_eq!(
        service
            .state()
            .items
            .values()
            .filter(|item| item.turn_id == "turn-known")
            .count(),
        2
    );
}

#[test]
fn server_request_resolved_notification_is_recorded_for_known_thread() {
    let mut service = ThreadService::new(WORKSPACE_CWD.into());
    let _ = service.state_mut().apply_stream_event(StreamEvent {
        sequence: 1,
        dedupe_key: None,
        payload: ReducerEvent::ThreadStarted {
            thread_id: "thread-known".to_string(),
            cwd: WORKSPACE_CWD.to_string(),
            title: None,
            updated_at: None,
        },
    });

    service.apply_server_notification(ServerNotification::ServerRequestResolved(
        ServerRequestResolvedNotification {
            thread_id: "thread-known".to_string(),
            request_id: RequestId::Integer(123),
        },
    ));

    let summary = service
        .state()
        .server_requests
        .get("123")
        .expect("server request should be tracked");
    assert_eq!(summary.decision, ServerRequestDecision::Unknown);
    assert_eq!(summary.item_id, None);
}

#[test]
fn turn_start_applies_streamed_delta_completion_and_diff() {
    let server = TestServer::spawn(Scenario::TurnStartStreaming);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");

    service
        .start_turn(
            &mut session,
            TurnStartParams {
                thread_id: "thread-turn-stream".to_string(),
                input: vec![UserInput::Text {
                    text: "hello".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
            TIMEOUT,
        )
        .expect("turn/start should succeed");

    assert_eq!(
        service
            .state()
            .turns
            .get("turn-stream")
            .expect("turn should exist")
            .status,
        hunk_codex::state::TurnStatus::Completed
    );
    assert_eq!(
        service
            .state()
            .items
            .get("item-stream")
            .expect("item should exist")
            .content,
        "hello"
    );
    assert_eq!(
        service
            .state()
            .turn_diffs
            .get("turn-stream")
            .map(String::as_str),
        Some("diff --git a/a b/a")
    );

    server.join();
}

#[test]
fn item_completed_snapshot_does_not_duplicate_existing_delta_content() {
    let server = TestServer::spawn(Scenario::ItemCompletedNoDup);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");

    service
        .start_turn(
            &mut session,
            TurnStartParams {
                thread_id: "thread-no-dup".to_string(),
                input: vec![UserInput::Text {
                    text: "hello".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
            TIMEOUT,
        )
        .expect("turn/start should succeed");

    assert_eq!(
        service
            .state()
            .items
            .get("item-no-dup")
            .expect("item should exist")
            .content,
        "hello"
    );

    server.join();
}

#[test]
fn turn_steer_round_trip_returns_target_turn_id() {
    let server = TestServer::spawn(Scenario::TurnSteerRoundTrip);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");

    let response = service
        .steer_turn(
            &mut session,
            TurnSteerParams {
                thread_id: "thread-steer".to_string(),
                input: vec![UserInput::Text {
                    text: "continue".to_string(),
                    text_elements: Vec::new(),
                }],
                expected_turn_id: "turn-active".to_string(),
            },
            TIMEOUT,
        )
        .expect("turn/steer should succeed");

    assert_eq!(response.turn_id, "turn-active");
    server.join();
}

#[test]
fn turn_interrupt_marks_turn_completed() {
    let server = TestServer::spawn(Scenario::TurnInterrupt);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");
    service
        .start_turn(
            &mut session,
            TurnStartParams {
                thread_id: "thread-interrupt".to_string(),
                input: vec![UserInput::Text {
                    text: "start".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
            TIMEOUT,
        )
        .expect("turn/start should succeed");

    service
        .interrupt_turn(
            &mut session,
            TurnInterruptParams {
                thread_id: "thread-interrupt".to_string(),
                turn_id: "turn-interrupt".to_string(),
            },
            TIMEOUT,
        )
        .expect("turn/interrupt should succeed");

    assert_eq!(
        service
            .state()
            .turns
            .get("turn-interrupt")
            .expect("turn should exist")
            .status,
        hunk_codex::state::TurnStatus::Completed
    );

    server.join();
}

#[test]
fn review_start_selects_review_thread() {
    let server = TestServer::spawn(Scenario::ReviewStartDetached);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");

    let review = service
        .start_review(
            &mut session,
            ReviewStartParams {
                thread_id: "thread-review-root".to_string(),
                target: ReviewTarget::Custom {
                    instructions: "Review my diff".to_string(),
                },
                delivery: None,
            },
            TIMEOUT,
        )
        .expect("review/start should succeed");

    assert_eq!(review.review_thread_id, "thread-review-detached");
    assert_eq!(
        service.active_thread_for_workspace(),
        Some("thread-review-detached")
    );
    assert!(
        service
            .state()
            .threads
            .contains_key("thread-review-detached")
    );
    assert!(service.state().turns.contains_key("review-turn"));

    server.join();
}

#[test]
fn review_start_streams_mode_entry_and_exit_items() {
    let server = TestServer::spawn(Scenario::ReviewStartDetached);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    service
        .start_thread(&mut session, ThreadStartParams::default(), TIMEOUT)
        .expect("thread/start should succeed");

    service
        .start_review(
            &mut session,
            ReviewStartParams {
                thread_id: "thread-review-root".to_string(),
                target: ReviewTarget::Custom {
                    instructions: "Review my diff".to_string(),
                },
                delivery: None,
            },
            TIMEOUT,
        )
        .expect("review/start should succeed");

    let state = service.state();
    let entered = state
        .items
        .get("review-entered")
        .expect("entered review item should exist");
    assert_eq!(entered.turn_id, "review-turn");
    assert_eq!(entered.kind, "enteredReviewMode");
    assert_eq!(entered.status, hunk_codex::state::ItemStatus::Streaming);
    assert_eq!(entered.content, "Reviewing working-copy diff");

    let exited = state
        .items
        .get("review-exited")
        .expect("exited review item should exist");
    assert_eq!(exited.turn_id, "review-turn");
    assert_eq!(exited.kind, "exitedReviewMode");
    assert_eq!(exited.status, hunk_codex::state::ItemStatus::Completed);
    assert_eq!(exited.content, "No blocking issues found.");

    server.join();
}

#[test]
fn command_exec_injects_workspace_cwd_when_missing() {
    let server = TestServer::spawn(Scenario::CommandExecRoundTrip);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let response = service
        .command_exec(
            &mut session,
            CommandExecParams {
                command: vec!["pwd".to_string()],
                timeout_ms: None,
                cwd: None,
                sandbox_policy: None,
            },
            TIMEOUT,
        )
        .expect("command/exec should succeed");

    assert_eq!(response.exit_code, 0);
    assert_eq!(response.stdout, "ok");
    assert_eq!(response.stderr, "");

    server.join();
}

#[test]
fn command_exec_error_is_mapped_to_jsonrpc_server_error() {
    let server = TestServer::spawn(Scenario::CommandExecServerError);
    let mut session = connect_initialized_session(server.port);
    let mut service = ThreadService::new(WORKSPACE_CWD.into());

    let error = service
        .command_exec(
            &mut session,
            CommandExecParams {
                command: vec!["false".to_string()],
                timeout_ms: None,
                cwd: None,
                sandbox_policy: None,
            },
            TIMEOUT,
        )
        .expect_err("command/exec should fail");

    match error {
        CodexIntegrationError::JsonRpcServerError { code, message } => {
            assert_eq!(code, -32003);
            assert_eq!(message, "command failed");
        }
        other => panic!("unexpected error: {other:?}"),
    }

    server.join();
}

#[derive(Clone)]
enum Scenario {
    ListThreadsScoped,
    ResumeExternalThread,
    UnsubscribeSemantics,
    ArchiveRoundTrip,
    CompactWithBufferedNotification,
    RollbackPrunesTurns,
    LoadedReadFork,
    TurnStartStreaming,
    ItemCompletedNoDup,
    TurnSteerRoundTrip,
    TurnInterrupt,
    ReviewStartDetached,
    CommandExecRoundTrip,
    CommandExecServerError,
    SkillsAndAppsMetadata,
    AccountEndpoints,
    ModelAndSessionMetadata,
}

struct TestServer {
    port: u16,
    join: thread::JoinHandle<()>,
}

impl TestServer {
    fn spawn(scenario: Scenario) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind should succeed");
            let port = listener
                .local_addr()
                .expect("local addr should exist")
                .port();
            tx.send(port).expect("port should be sent");

            let (stream, _) = listener.accept().expect("accept should succeed");
            let mut socket = accept(stream).expect("websocket handshake should succeed");
            run_initialize_handshake(&mut socket);

            match scenario {
                Scenario::ListThreadsScoped => run_list_threads_scoped(&mut socket),
                Scenario::ResumeExternalThread => run_resume_external_thread(&mut socket),
                Scenario::UnsubscribeSemantics => run_unsubscribe_semantics(&mut socket),
                Scenario::ArchiveRoundTrip => run_archive_round_trip(&mut socket),
                Scenario::CompactWithBufferedNotification => {
                    run_compact_with_buffered_notification(&mut socket)
                }
                Scenario::RollbackPrunesTurns => run_rollback_prunes_turns(&mut socket),
                Scenario::LoadedReadFork => run_loaded_read_fork(&mut socket),
                Scenario::TurnStartStreaming => run_turn_start_streaming(&mut socket),
                Scenario::ItemCompletedNoDup => run_item_completed_no_dup(&mut socket),
                Scenario::TurnSteerRoundTrip => run_turn_steer_round_trip(&mut socket),
                Scenario::TurnInterrupt => run_turn_interrupt(&mut socket),
                Scenario::ReviewStartDetached => run_review_start_detached(&mut socket),
                Scenario::CommandExecRoundTrip => run_command_exec_round_trip(&mut socket),
                Scenario::CommandExecServerError => run_command_exec_server_error(&mut socket),
                Scenario::SkillsAndAppsMetadata => run_skills_and_apps_metadata(&mut socket),
                Scenario::AccountEndpoints => run_account_endpoints(&mut socket),
                Scenario::ModelAndSessionMetadata => run_model_and_session_metadata(&mut socket),
            }
        });

        let port = rx.recv().expect("port should be received");
        Self { port, join }
    }

    fn join(self) {
        self.join
            .join()
            .expect("test server thread should complete");
    }
}

fn run_initialize_handshake(socket: &mut WebSocket<TcpStream>) {
    let initialize = expect_request(socket, api::method::INITIALIZE);
    send_success_response(
        socket,
        initialize.id,
        serde_json::json!({ "userAgent": "hunk-thread-service-test-server" }),
    );
    expect_notification(socket, api::method::INITIALIZED);
}

fn run_list_threads_scoped(socket: &mut WebSocket<TcpStream>) {
    let request = expect_request(socket, api::method::THREAD_LIST);
    let params = request
        .params
        .expect("thread/list params should be present");
    assert_eq!(
        param_string(&params, "cwd"),
        Some(WORKSPACE_CWD.to_string())
    );
    assert_eq!(params.get("archived"), Some(&serde_json::json!(false)));

    let response = ThreadListResponse {
        data: vec![
            thread(
                "thread-in-workspace",
                WORKSPACE_CWD,
                ThreadStatus::Idle,
                vec![],
            ),
            thread(
                "thread-outside-workspace",
                OTHER_CWD,
                ThreadStatus::Idle,
                vec![],
            ),
        ],
        next_cursor: None,
    };
    send_typed_success_response(socket, request.id, &response);
}

fn run_resume_external_thread(socket: &mut WebSocket<TcpStream>) {
    let request = expect_request(socket, api::method::THREAD_RESUME);
    let params = request
        .params
        .expect("thread/resume params should be present");
    assert_eq!(
        param_string(&params, "threadId"),
        Some("external-thread".to_string())
    );
    assert_eq!(
        param_string(&params, "cwd"),
        Some(WORKSPACE_CWD.to_string())
    );

    let response = thread_resume_response(thread(
        "external-thread",
        WORKSPACE_CWD,
        ThreadStatus::Idle,
        vec![turn_with_items(
            "resume-turn-1",
            TurnStatus::Completed,
            vec![ThreadItem::UserMessage {
                id: "resume-item-1".to_string(),
                content: vec![UserInput::Text {
                    text: "resume prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            }],
        )],
    ));
    send_typed_success_response(socket, request.id, &response);
}

fn run_unsubscribe_semantics(socket: &mut WebSocket<TcpStream>) {
    let start = expect_request(socket, api::method::THREAD_START);
    let start_params = start.params.expect("thread/start params should be present");
    assert_eq!(
        param_string(&start_params, "cwd"),
        Some(WORKSPACE_CWD.to_string())
    );
    send_typed_success_response(
        socket,
        start.id,
        &thread_start_response(thread(
            "thread-unsub",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let first = expect_request(socket, api::method::THREAD_UNSUBSCRIBE);
    send_typed_success_response(
        socket,
        first.id,
        &ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::NotSubscribed,
        },
    );

    let second = expect_request(socket, api::method::THREAD_UNSUBSCRIBE);
    send_thread_status_changed_notification(socket, "thread-unsub", ThreadStatus::NotLoaded);
    send_typed_success_response(
        socket,
        second.id,
        &ThreadUnsubscribeResponse {
            status: ThreadUnsubscribeStatus::Unsubscribed,
        },
    );
}

fn run_archive_round_trip(socket: &mut WebSocket<TcpStream>) {
    let start = expect_request(socket, api::method::THREAD_START);
    send_typed_success_response(
        socket,
        start.id,
        &thread_start_response(thread(
            "thread-archive",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let archive = expect_request(socket, api::method::THREAD_ARCHIVE);
    send_typed_success_response(socket, archive.id, &ThreadArchiveResponse {});

    let unarchive = expect_request(socket, api::method::THREAD_UNARCHIVE);
    send_typed_success_response(
        socket,
        unarchive.id,
        &ThreadUnarchiveResponse {
            thread: thread("thread-archive", WORKSPACE_CWD, ThreadStatus::Idle, vec![]),
        },
    );
}

fn run_compact_with_buffered_notification(socket: &mut WebSocket<TcpStream>) {
    let start = expect_request(socket, api::method::THREAD_START);
    send_typed_success_response(
        socket,
        start.id,
        &thread_start_response(thread(
            "thread-compact",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let compact = expect_request(socket, api::method::THREAD_COMPACT_START);
    let compact_params = compact
        .params
        .expect("thread/compact/start params should be present");
    assert_eq!(
        param_string(&compact_params, "threadId"),
        Some("thread-compact".to_string())
    );
    send_thread_archived_notification(socket, "thread-compact");
    send_typed_success_response(socket, compact.id, &ThreadCompactStartResponse {});
}

fn run_rollback_prunes_turns(socket: &mut WebSocket<TcpStream>) {
    let resume = expect_request(socket, api::method::THREAD_RESUME);
    send_typed_success_response(
        socket,
        resume.id,
        &thread_resume_response(thread(
            "thread-rollback",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![
                turn("turn-keep", TurnStatus::Completed),
                turn("turn-removed", TurnStatus::Completed),
            ],
        )),
    );

    let rollback = expect_request(socket, api::method::THREAD_ROLLBACK);
    let rollback_params = rollback
        .params
        .expect("thread/rollback params should be present");
    assert_eq!(
        param_string(&rollback_params, "threadId"),
        Some("thread-rollback".to_string())
    );
    assert_eq!(rollback_params.get("numTurns"), Some(&serde_json::json!(1)));
    send_typed_success_response(
        socket,
        rollback.id,
        &ThreadRollbackResponse {
            thread: thread(
                "thread-rollback",
                WORKSPACE_CWD,
                ThreadStatus::Idle,
                vec![turn("turn-keep", TurnStatus::Completed)],
            ),
        },
    );
}

fn run_loaded_read_fork(socket: &mut WebSocket<TcpStream>) {
    let loaded = expect_request(socket, api::method::THREAD_LOADED_LIST);
    send_typed_success_response(
        socket,
        loaded.id,
        &ThreadLoadedListResponse {
            data: vec!["thread-read".to_string()],
            next_cursor: None,
        },
    );

    let read = expect_request(socket, api::method::THREAD_READ);
    let read_params = read.params.expect("thread/read params should be present");
    assert_eq!(
        param_string(&read_params, "threadId"),
        Some("thread-read".to_string())
    );
    assert_eq!(
        read_params.get("includeTurns"),
        Some(&serde_json::json!(true))
    );
    send_typed_success_response(
        socket,
        read.id,
        &ThreadReadResponse {
            thread: thread(
                "thread-read",
                WORKSPACE_CWD,
                ThreadStatus::Idle,
                vec![turn("read-turn-1", TurnStatus::Completed)],
            ),
        },
    );

    let fork = expect_request(socket, api::method::THREAD_FORK);
    let fork_params = fork.params.expect("thread/fork params should be present");
    assert_eq!(
        param_string(&fork_params, "threadId"),
        Some("thread-read".to_string())
    );
    assert_eq!(
        param_string(&fork_params, "cwd"),
        Some(WORKSPACE_CWD.to_string())
    );
    send_typed_success_response(
        socket,
        fork.id,
        &ThreadForkResponse {
            thread: thread(
                "thread-forked",
                WORKSPACE_CWD,
                ThreadStatus::Idle,
                vec![turn("fork-turn-1", TurnStatus::Completed)],
            ),
            model: "gpt-5-codex".to_string(),
            model_provider: "openai".to_string(),
            service_tier: None,
            cwd: WORKSPACE_CWD.into(),
            approval_policy: AskForApproval::OnRequest,
            sandbox: SandboxPolicy::DangerFullAccess,
            reasoning_effort: None,
        },
    );
}

fn run_turn_start_streaming(socket: &mut WebSocket<TcpStream>) {
    let start_thread = expect_request(socket, api::method::THREAD_START);
    send_typed_success_response(
        socket,
        start_thread.id,
        &thread_start_response(thread(
            "thread-turn-stream",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let start_turn = expect_request(socket, api::method::TURN_START);
    let start_turn_params = start_turn
        .params
        .expect("turn/start params should be present");
    assert_eq!(
        param_string(&start_turn_params, "threadId"),
        Some("thread-turn-stream".to_string())
    );

    send_notification(
        socket,
        "item/agentMessage/delta",
        serde_json::json!({
            "threadId": "thread-turn-stream",
            "turnId": "turn-stream",
            "itemId": "item-stream",
            "delta": "hello"
        }),
    );
    send_notification(
        socket,
        "turn/diff/updated",
        serde_json::json!({
            "threadId": "thread-turn-stream",
            "turnId": "turn-stream",
            "diff": "diff --git a/a b/a"
        }),
    );
    send_notification(
        socket,
        "turn/completed",
        serde_json::json!({
            "threadId": "thread-turn-stream",
            "turn": {
                "id": "turn-stream",
                "items": [],
                "status": "completed",
                "error": null
            }
        }),
    );

    send_typed_success_response(
        socket,
        start_turn.id,
        &TurnStartResponse {
            turn: turn("turn-stream", TurnStatus::InProgress),
        },
    );
}

fn run_item_completed_no_dup(socket: &mut WebSocket<TcpStream>) {
    let start_thread = expect_request(socket, api::method::THREAD_START);
    send_typed_success_response(
        socket,
        start_thread.id,
        &thread_start_response(thread(
            "thread-no-dup",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let start_turn = expect_request(socket, api::method::TURN_START);
    send_notification(
        socket,
        "item/agentMessage/delta",
        serde_json::json!({
            "threadId": "thread-no-dup",
            "turnId": "turn-no-dup",
            "itemId": "item-no-dup",
            "delta": "hello"
        }),
    );
    send_notification(
        socket,
        "item/completed",
        serde_json::json!({
            "threadId": "thread-no-dup",
            "turnId": "turn-no-dup",
            "item": {
                "type": "agentMessage",
                "id": "item-no-dup",
                "text": "hello",
                "phase": null
            }
        }),
    );
    send_typed_success_response(
        socket,
        start_turn.id,
        &TurnStartResponse {
            turn: turn("turn-no-dup", TurnStatus::InProgress),
        },
    );
}

fn run_turn_steer_round_trip(socket: &mut WebSocket<TcpStream>) {
    let start_thread = expect_request(socket, api::method::THREAD_START);
    send_typed_success_response(
        socket,
        start_thread.id,
        &thread_start_response(thread(
            "thread-steer",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let steer = expect_request(socket, api::method::TURN_STEER);
    let steer_params = steer.params.expect("turn/steer params should be present");
    assert_eq!(
        param_string(&steer_params, "threadId"),
        Some("thread-steer".to_string())
    );
    assert_eq!(
        param_string(&steer_params, "expectedTurnId"),
        Some("turn-active".to_string())
    );
    send_typed_success_response(
        socket,
        steer.id,
        &TurnSteerResponse {
            turn_id: "turn-active".to_string(),
        },
    );
}

fn run_turn_interrupt(socket: &mut WebSocket<TcpStream>) {
    let start_thread = expect_request(socket, api::method::THREAD_START);
    send_typed_success_response(
        socket,
        start_thread.id,
        &thread_start_response(thread(
            "thread-interrupt",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let start_turn = expect_request(socket, api::method::TURN_START);
    send_typed_success_response(
        socket,
        start_turn.id,
        &TurnStartResponse {
            turn: turn("turn-interrupt", TurnStatus::InProgress),
        },
    );

    let interrupt = expect_request(socket, api::method::TURN_INTERRUPT);
    let interrupt_params = interrupt
        .params
        .expect("turn/interrupt params should be present");
    assert_eq!(
        param_string(&interrupt_params, "threadId"),
        Some("thread-interrupt".to_string())
    );
    assert_eq!(
        param_string(&interrupt_params, "turnId"),
        Some("turn-interrupt".to_string())
    );
    send_typed_success_response(socket, interrupt.id, &TurnInterruptResponse {});
}

fn run_review_start_detached(socket: &mut WebSocket<TcpStream>) {
    let start_thread = expect_request(socket, api::method::THREAD_START);
    send_typed_success_response(
        socket,
        start_thread.id,
        &thread_start_response(thread(
            "thread-review-root",
            WORKSPACE_CWD,
            ThreadStatus::Idle,
            vec![],
        )),
    );

    let review = expect_request(socket, api::method::REVIEW_START);
    let review_params = review
        .params
        .expect("review/start params should be present");
    assert_eq!(
        param_string(&review_params, "threadId"),
        Some("thread-review-root".to_string())
    );
    send_notification(
        socket,
        "item/started",
        serde_json::json!({
            "threadId": "thread-review-detached",
            "turnId": "review-turn",
            "item": {
                "type": "enteredReviewMode",
                "id": "review-entered",
                "review": "Reviewing working-copy diff"
            }
        }),
    );
    send_notification(
        socket,
        "item/completed",
        serde_json::json!({
            "threadId": "thread-review-detached",
            "turnId": "review-turn",
            "item": {
                "type": "exitedReviewMode",
                "id": "review-exited",
                "review": "No blocking issues found."
            }
        }),
    );
    send_typed_success_response(
        socket,
        review.id,
        &ReviewStartResponse {
            turn: turn("review-turn", TurnStatus::InProgress),
            review_thread_id: "thread-review-detached".to_string(),
        },
    );
}

fn run_command_exec_round_trip(socket: &mut WebSocket<TcpStream>) {
    let command_exec = expect_request(socket, api::method::COMMAND_EXEC);
    let command_exec_params = command_exec
        .params
        .expect("command/exec params should be present");
    assert_eq!(
        param_string(&command_exec_params, "cwd"),
        Some(WORKSPACE_CWD.to_string())
    );
    send_typed_success_response(
        socket,
        command_exec.id,
        &CommandExecResponse {
            exit_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
        },
    );
}

fn run_command_exec_server_error(socket: &mut WebSocket<TcpStream>) {
    let command_exec = expect_request(socket, api::method::COMMAND_EXEC);
    let command_exec_params = command_exec
        .params
        .expect("command/exec params should be present");
    assert_eq!(
        param_string(&command_exec_params, "cwd"),
        Some(WORKSPACE_CWD.to_string())
    );
    send_error_response(socket, command_exec.id, -32003, "command failed");
}

fn run_skills_and_apps_metadata(socket: &mut WebSocket<TcpStream>) {
    let skills_list = expect_request(socket, api::method::SKILLS_LIST);
    let skills_list_params = skills_list
        .params
        .expect("skills/list params should be present");
    assert_eq!(
        skills_list_params["cwds"],
        serde_json::json!([WORKSPACE_CWD])
    );
    assert_eq!(skills_list_params["forceReload"], serde_json::json!(true));
    send_typed_success_response(
        socket,
        skills_list.id,
        &SkillsListResponse {
            data: vec![
                serde_json::from_value(serde_json::json!({
                    "cwd": WORKSPACE_CWD,
                    "skills": [
                        {
                            "name": "repo-workspace-skill",
                            "description": "Workspace skill",
                            "path": format!("{WORKSPACE_CWD}/.codex/skills/repo-workspace-skill"),
                            "scope": "repo",
                            "enabled": true
                        }
                    ],
                    "errors": []
                }))
                .expect("skills list entry should deserialize"),
            ],
        },
    );

    let skills_config_write = expect_request(socket, api::method::SKILLS_CONFIG_WRITE);
    let skills_config_write_params = skills_config_write
        .params
        .expect("skills/config/write params should be present");
    assert_eq!(
        param_string(&skills_config_write_params, "path"),
        Some(format!(
            "{WORKSPACE_CWD}/.codex/skills/repo-workspace-skill/SKILL.md"
        ))
    );
    assert_eq!(
        skills_config_write_params["enabled"],
        serde_json::json!(false)
    );
    send_typed_success_response(
        socket,
        skills_config_write.id,
        &SkillsConfigWriteResponse {
            effective_enabled: false,
        },
    );

    let app_list = expect_request(socket, api::method::APP_LIST);
    let app_list_params = app_list.params.expect("app/list params should be present");
    assert_eq!(
        param_string(&app_list_params, "cursor"),
        Some("cursor-1".to_string())
    );
    assert_eq!(app_list_params["limit"], serde_json::json!(5));
    assert_eq!(app_list_params["forceRefetch"], serde_json::json!(true));
    assert_eq!(app_list_params["threadId"], serde_json::Value::Null);
    send_typed_success_response(
        socket,
        app_list.id,
        &AppsListResponse {
            data: vec![AppInfo {
                id: "github".to_string(),
                name: "GitHub".to_string(),
                description: Some("GitHub app".to_string()),
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: None,
                is_accessible: true,
                is_enabled: true,
            }],
            next_cursor: Some("next-cursor".to_string()),
        },
    );
}

fn run_account_endpoints(socket: &mut WebSocket<TcpStream>) {
    let account_read = expect_request(socket, api::method::ACCOUNT_READ);
    let account_read_params = account_read
        .params
        .expect("account/read params should be present");
    assert_eq!(
        account_read_params["refreshToken"],
        serde_json::json!(false)
    );
    send_typed_success_response(
        socket,
        account_read.id,
        &GetAccountResponse {
            account: None,
            requires_openai_auth: true,
        },
    );

    let login_start = expect_request(socket, api::method::ACCOUNT_LOGIN_START);
    let login_start_params = login_start
        .params
        .expect("account/login/start params should be present");
    assert_eq!(login_start_params["type"], serde_json::json!("chatgpt"));
    send_typed_success_response(
        socket,
        login_start.id,
        &LoginAccountResponse::Chatgpt {
            login_id: "login-1".to_string(),
            auth_url: "https://auth.example/login".to_string(),
        },
    );

    let login_cancel = expect_request(socket, api::method::ACCOUNT_LOGIN_CANCEL);
    let login_cancel_params = login_cancel
        .params
        .expect("account/login/cancel params should be present");
    assert_eq!(login_cancel_params["loginId"], serde_json::json!("login-1"));
    send_typed_success_response(
        socket,
        login_cancel.id,
        &codex_app_server_protocol::CancelLoginAccountResponse {
            status: CancelLoginAccountStatus::Canceled,
        },
    );

    let rate_limits = expect_request(socket, api::method::ACCOUNT_RATE_LIMITS_READ);
    send_typed_success_response(
        socket,
        rate_limits.id,
        &GetAccountRateLimitsResponse {
            rate_limits: RateLimitSnapshot {
                limit_id: Some("codex".to_string()),
                limit_name: Some("Codex".to_string()),
                primary: Some(RateLimitWindow {
                    used_percent: 42,
                    window_duration_mins: Some(60),
                    resets_at: Some(1_700_000_000),
                }),
                secondary: None,
                credits: None,
                plan_type: None,
            },
            rate_limits_by_limit_id: None,
        },
    );

    let logout = expect_request(socket, api::method::ACCOUNT_LOGOUT);
    send_typed_success_response(socket, logout.id, &LogoutAccountResponse {});
}

fn run_model_and_session_metadata(socket: &mut WebSocket<TcpStream>) {
    let first_models = expect_request(socket, api::method::MODEL_LIST);
    let first_params = first_models
        .params
        .expect("first model/list params should be present");
    assert_eq!(first_params.get("cursor"), Some(&serde_json::Value::Null));
    assert_eq!(first_params.get("limit"), Some(&serde_json::json!(1)));
    assert_eq!(
        first_params.get("includeHidden"),
        Some(&serde_json::json!(false))
    );
    send_success_response(
        socket,
        first_models.id,
        serde_json::json!({
            "data": [{
                "id": "model-visible",
                "model": "gpt-5-codex",
                "upgrade": null,
                "upgradeInfo": null,
                "availabilityNux": null,
                "displayName": "GPT-5 Codex",
                "description": "Visible model",
                "hidden": false,
                "supportedReasoningEfforts": [{
                    "reasoningEffort": "high",
                    "description": "High reasoning effort"
                }],
                "defaultReasoningEffort": "high",
                "inputModalities": ["text"],
                "supportsPersonality": false,
                "isDefault": true
            }],
            "nextCursor": "cursor-hidden"
        }),
    );

    let second_models = expect_request(socket, api::method::MODEL_LIST);
    let second_params = second_models
        .params
        .expect("second model/list params should be present");
    assert_eq!(
        param_string(&second_params, "cursor"),
        Some("cursor-hidden".to_string())
    );
    assert_eq!(second_params.get("limit"), Some(&serde_json::json!(1)));
    assert_eq!(
        second_params.get("includeHidden"),
        Some(&serde_json::json!(true))
    );
    send_success_response(
        socket,
        second_models.id,
        serde_json::json!({
            "data": [{
                "id": "model-hidden",
                "model": "gpt-5-mini",
                "upgrade": null,
                "upgradeInfo": null,
                "availabilityNux": null,
                "displayName": "GPT-5 Mini",
                "description": "Hidden model",
                "hidden": true,
                "supportedReasoningEfforts": [{
                    "reasoningEffort": "low",
                    "description": "Low reasoning effort"
                }],
                "defaultReasoningEffort": "low",
                "inputModalities": ["text"],
                "supportsPersonality": false,
                "isDefault": false
            }],
            "nextCursor": null
        }),
    );

    let features = expect_request(socket, api::method::EXPERIMENTAL_FEATURE_LIST);
    let feature_params = features
        .params
        .expect("experimentalFeature/list params should be present");
    assert_eq!(feature_params.get("cursor"), Some(&serde_json::Value::Null));
    assert_eq!(feature_params.get("limit"), Some(&serde_json::json!(20)));
    send_success_response(
        socket,
        features.id,
        serde_json::json!({
            "data": [{
                "name": "collaboration_modes",
                "stage": "removed",
                "displayName": null,
                "description": null,
                "announcement": null,
                "enabled": true,
                "defaultEnabled": true
            }],
            "nextCursor": null
        }),
    );

    let modes = expect_request(socket, api::method::COLLABORATION_MODE_LIST);
    let mode_params = modes
        .params
        .expect("collaborationMode/list params should be present");
    assert_eq!(mode_params, serde_json::json!({}));
    send_success_response(
        socket,
        modes.id,
        serde_json::json!({
            "data": [{
                "name": "Plan",
                "mode": "plan",
                "model": "gpt-5-codex",
                "reasoning_effort": "high"
            }]
        }),
    );
}

fn connect_initialized_session(port: u16) -> JsonRpcSession {
    let endpoint = WebSocketEndpoint::loopback(port);
    let mut session = JsonRpcSession::connect(&endpoint).expect("session should connect");
    session
        .initialize(InitializeOptions::default(), TIMEOUT)
        .expect("initialize should succeed");
    session
}

fn thread(id: &str, cwd: &str, status: ThreadStatus, turns: Vec<Turn>) -> Thread {
    Thread {
        id: id.to_string(),
        preview: format!("preview-{id}"),
        ephemeral: false,
        model_provider: "openai".to_string(),
        created_at: 1,
        updated_at: 2,
        status,
        path: Some(format!("/tmp/.codex/threads/{id}.jsonl").into()),
        cwd: cwd.into(),
        cli_version: "0.1.0".to_string(),
        source: SessionSource::AppServer,
        agent_nickname: None,
        agent_role: None,
        git_info: None,
        name: Some(format!("Thread {id}")),
        turns,
    }
}

fn turn(id: &str, status: TurnStatus) -> Turn {
    Turn {
        id: id.to_string(),
        items: Vec::new(),
        status,
        error: None,
    }
}

fn turn_with_items(id: &str, status: TurnStatus, items: Vec<ThreadItem>) -> Turn {
    Turn {
        id: id.to_string(),
        items,
        status,
        error: None,
    }
}

fn thread_start_response(thread: Thread) -> ThreadStartResponse {
    ThreadStartResponse {
        cwd: thread.cwd.clone(),
        thread,
        model: "gpt-5-codex".to_string(),
        model_provider: "openai".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::OnRequest,
        sandbox: SandboxPolicy::DangerFullAccess,
        reasoning_effort: None,
    }
}

fn thread_resume_response(thread: Thread) -> ThreadResumeResponse {
    ThreadResumeResponse {
        cwd: thread.cwd.clone(),
        thread,
        model: "gpt-5-codex".to_string(),
        model_provider: "openai".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::OnRequest,
        sandbox: SandboxPolicy::DangerFullAccess,
        reasoning_effort: None,
    }
}

fn send_thread_archived_notification(socket: &mut WebSocket<TcpStream>, thread_id: &str) {
    send_notification(
        socket,
        "thread/archived",
        serde_json::json!({ "threadId": thread_id }),
    );
}

fn send_thread_status_changed_notification(
    socket: &mut WebSocket<TcpStream>,
    thread_id: &str,
    status: ThreadStatus,
) {
    let notification = ThreadStatusChangedNotification {
        thread_id: thread_id.to_string(),
        status,
    };
    let params =
        serde_json::to_value(notification).expect("notification serialization should succeed");
    send_notification(socket, "thread/status/changed", params);
}

fn send_notification(socket: &mut WebSocket<TcpStream>, method: &str, params: Value) {
    send_jsonrpc(
        socket,
        JSONRPCMessage::Notification(JSONRPCNotification {
            method: method.to_string(),
            params: Some(params),
        }),
    );
}

fn expect_request(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCRequest {
    match read_jsonrpc(socket) {
        JSONRPCMessage::Request(request) => {
            assert_eq!(request.method, method, "unexpected method");
            request
        }
        other => panic!("expected request, got: {other:?}"),
    }
}

fn expect_notification(socket: &mut WebSocket<TcpStream>, method: &str) -> JSONRPCNotification {
    match read_jsonrpc(socket) {
        JSONRPCMessage::Notification(notification) => {
            assert_eq!(
                notification.method, method,
                "unexpected notification method"
            );
            notification
        }
        other => panic!("expected notification, got: {other:?}"),
    }
}

fn send_typed_success_response<T: serde::Serialize>(
    socket: &mut WebSocket<TcpStream>,
    id: RequestId,
    result: &T,
) {
    let value = serde_json::to_value(result).expect("response serialization should succeed");
    send_success_response(socket, id, value);
}

fn send_success_response(socket: &mut WebSocket<TcpStream>, id: RequestId, result: Value) {
    send_jsonrpc(
        socket,
        JSONRPCMessage::Response(JSONRPCResponse { id, result }),
    );
}

fn send_error_response(socket: &mut WebSocket<TcpStream>, id: RequestId, code: i64, message: &str) {
    send_jsonrpc(
        socket,
        JSONRPCMessage::Error(JSONRPCError {
            id,
            error: JSONRPCErrorError {
                code,
                data: None,
                message: message.to_string(),
            },
        }),
    );
}

fn send_jsonrpc(socket: &mut WebSocket<TcpStream>, message: JSONRPCMessage) {
    let payload = serde_json::to_string(&message).expect("serialize should succeed");
    socket
        .send(Message::Text(payload.into()))
        .expect("socket send should succeed");
}

fn read_jsonrpc(socket: &mut WebSocket<TcpStream>) -> JSONRPCMessage {
    loop {
        let frame = socket.read().expect("socket read should succeed");
        match frame {
            Message::Text(text) => {
                return serde_json::from_str(text.as_ref()).expect("json parse should succeed");
            }
            Message::Binary(bytes) => {
                return serde_json::from_slice(bytes.as_ref()).expect("json parse should succeed");
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .expect("pong send should succeed");
            }
            Message::Pong(_) | Message::Frame(_) => {}
            Message::Close(_) => panic!("unexpected socket close"),
        }
    }
}

fn param_string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}
