use std::collections::BTreeMap;

use hunk_codex::state::ActiveThreadStore;
use hunk_codex::state::AiState;
use hunk_codex::state::ApplyOutcome;
use hunk_codex::state::ItemDisplayMetadata;
use hunk_codex::state::ItemStatus;
use hunk_codex::state::ReducerEvent;
use hunk_codex::state::ServerRequestDecision;
use hunk_codex::state::StreamEvent;
use hunk_codex::state::ThreadLifecycleStatus;
use hunk_codex::state::TurnPlanStepStatus;
use hunk_codex::state::TurnPlanStepSummary;
use hunk_codex::state::TurnStatus;
use hunk_codex::state::turn_storage_key;

#[test]
fn ordered_stream_application_updates_all_entities() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: Some("Main Thread".to_string()),
                created_at: Some(150),
                updated_at: Some(200),
            },
        ),
        event(
            2,
            Some("turn-start:r1"),
            ReducerEvent::TurnStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
        event(
            3,
            Some("item-start:i1"),
            ReducerEvent::ItemStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "agentMessage".to_string(),
            },
        ),
        event(
            4,
            Some("item-delta:i1:1"),
            ReducerEvent::ItemDelta {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: "Hello".to_string(),
            },
        ),
        event(
            5,
            Some("item-delta:i1:2"),
            ReducerEvent::ItemDelta {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: " World".to_string(),
            },
        ),
        event(
            6,
            Some("item-completed:i1"),
            ReducerEvent::ItemCompleted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
            },
        ),
        event(
            7,
            Some("server-request:s1"),
            ReducerEvent::ServerRequestResolved {
                request_id: "s1".to_string(),
                item_id: Some("i1".to_string()),
                decision: ServerRequestDecision::Accept,
            },
        ),
        event(
            8,
            Some("turn-completed:r1"),
            ReducerEvent::TurnCompleted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
    ]);

    let thread = state.threads.get("t1").expect("thread must exist");
    assert_eq!(thread.cwd, "/repo");
    assert_eq!(thread.status, ThreadLifecycleStatus::Idle);
    assert_eq!(thread.created_at, 150);

    let turn = find_turn(&state, "t1", "r1");
    assert_eq!(turn.status, TurnStatus::Completed);

    let item = find_item(&state, "t1", "r1", "i1");
    assert_eq!(item.kind, "agentMessage");
    assert_eq!(item.content, "Hello World");
    assert_eq!(item.status, ItemStatus::Completed);

    let server_request = state
        .server_requests
        .get("s1")
        .expect("server request must exist");
    assert_eq!(server_request.item_id.as_deref(), Some("i1"));
}

#[test]
fn turn_plan_updates_replace_steps_and_preserve_creation_sequence() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: Some(10),
                updated_at: Some(10),
            },
        ),
        event(
            2,
            Some("turn-start:r1"),
            ReducerEvent::TurnStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
        event(
            3,
            Some("turn-plan:r1:1"),
            ReducerEvent::TurnPlanUpdated {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                explanation: Some("First pass".to_string()),
                steps: vec![
                    TurnPlanStepSummary {
                        step: "Inspect the reducer".to_string(),
                        status: TurnPlanStepStatus::InProgress,
                    },
                    TurnPlanStepSummary {
                        step: "Render the checklist".to_string(),
                        status: TurnPlanStepStatus::Pending,
                    },
                ],
            },
        ),
        event(
            4,
            Some("turn-plan:r1:2"),
            ReducerEvent::TurnPlanUpdated {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                explanation: Some("Second pass".to_string()),
                steps: vec![
                    TurnPlanStepSummary {
                        step: "Inspect the reducer".to_string(),
                        status: TurnPlanStepStatus::Completed,
                    },
                    TurnPlanStepSummary {
                        step: "Render the checklist".to_string(),
                        status: TurnPlanStepStatus::InProgress,
                    },
                ],
            },
        ),
    ]);

    let plan = state
        .turn_plans
        .get(turn_storage_key("t1", "r1").as_str())
        .expect("turn plan should exist");
    assert_eq!(plan.explanation.as_deref(), Some("Second pass"));
    assert_eq!(plan.created_sequence, 3);
    assert_eq!(plan.last_sequence, 4);
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.steps[0].status, TurnPlanStepStatus::Completed);
    assert_eq!(plan.steps[1].status, TurnPlanStepStatus::InProgress);
}

#[test]
fn thread_token_usage_updates_track_latest_summary_and_ignore_stale_sequences() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: Some(10),
                updated_at: Some(10),
            },
        ),
        event(
            2,
            Some("token-usage:t1:1"),
            ReducerEvent::ThreadTokenUsageUpdated {
                thread_id: "t1".to_string(),
                turn_id: "turn-1".to_string(),
                total: hunk_codex::state::TokenUsageBreakdownSummary {
                    total_tokens: 72_889,
                    input_tokens: 48_200,
                    cached_input_tokens: 14_400,
                    output_tokens: 7_800,
                    reasoning_output_tokens: 2_489,
                },
                last: hunk_codex::state::TokenUsageBreakdownSummary {
                    total_tokens: 6_400,
                    input_tokens: 4_500,
                    cached_input_tokens: 900,
                    output_tokens: 700,
                    reasoning_output_tokens: 300,
                },
                model_context_window: Some(258_000),
            },
        ),
    ]);

    let stale = state.apply_stream_event(event(
        1,
        Some("token-usage:t1:stale"),
        ReducerEvent::ThreadTokenUsageUpdated {
            thread_id: "t1".to_string(),
            turn_id: "turn-0".to_string(),
            total: hunk_codex::state::TokenUsageBreakdownSummary {
                total_tokens: 9_999,
                input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
            },
            last: hunk_codex::state::TokenUsageBreakdownSummary::default(),
            model_context_window: Some(128_000),
        },
    ));

    assert_eq!(stale, ApplyOutcome::Stale);

    let summary = state
        .thread_token_usage
        .get("t1")
        .expect("thread token usage should exist");
    assert_eq!(summary.turn_id, "turn-1");
    assert_eq!(summary.total.total_tokens, 72_889);
    assert_eq!(summary.last.total_tokens, 6_400);
    assert_eq!(summary.model_context_window, Some(258_000));
    assert_eq!(summary.last_sequence, 2);
}

#[test]
fn out_of_order_and_duplicate_events_are_idempotent() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            30,
            Some("item-start:i1"),
            ReducerEvent::ItemStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "agentMessage".to_string(),
            },
        ),
        event(
            10,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: Some(90),
                updated_at: Some(100),
            },
        ),
        event(
            20,
            Some("turn-start:r1"),
            ReducerEvent::TurnStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
        event(
            40,
            Some("item-delta:i1:1"),
            ReducerEvent::ItemDelta {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: "A".to_string(),
            },
        ),
        event(
            41,
            Some("item-delta:i1:1"),
            ReducerEvent::ItemDelta {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: "A".to_string(),
            },
        ),
        event(
            42,
            Some("item-delta:i1:2"),
            ReducerEvent::ItemDelta {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: "B".to_string(),
            },
        ),
        event(
            43,
            Some("item-completed:i1"),
            ReducerEvent::ItemCompleted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
            },
        ),
        event(
            5,
            Some("thread-status:t1:closed"),
            ReducerEvent::ThreadStatusChanged {
                thread_id: "t1".to_string(),
                status: ThreadLifecycleStatus::Closed,
            },
        ),
    ]);

    let thread = state.threads.get("t1").expect("thread must exist");
    assert_eq!(thread.status, ThreadLifecycleStatus::Closed);
    assert_eq!(thread.created_at, 90);

    let item = find_item(&state, "t1", "r1", "i1");
    assert_eq!(item.content, "AB");
    assert_eq!(item.status, ItemStatus::Completed);

    let duplicate_result = state.apply_stream_event(event(
        60,
        Some("item-delta:i1:2"),
        ReducerEvent::ItemDelta {
            thread_id: "t1".to_string(),
            turn_id: "r1".to_string(),
            item_id: "i1".to_string(),
            delta: "ignored".to_string(),
        },
    ));
    assert_eq!(duplicate_result, ApplyOutcome::Duplicate);
}

#[test]
fn thread_started_does_not_retarget_existing_thread_cwd() {
    let mut state = AiState::default();

    let first = state.apply_stream_event(event(
        1,
        Some("thread-start:t1"),
        ReducerEvent::ThreadStarted {
            thread_id: "t1".to_string(),
            cwd: "/repo/main".to_string(),
            title: Some("Main".to_string()),
            created_at: Some(10),
            updated_at: Some(10),
        },
    ));
    let second = state.apply_stream_event(event(
        2,
        Some("thread-start:t1:retry"),
        ReducerEvent::ThreadStarted {
            thread_id: "t1".to_string(),
            cwd: "/repo/worktree".to_string(),
            title: Some("Retried".to_string()),
            created_at: Some(10),
            updated_at: Some(11),
        },
    ));

    assert_eq!(first, ApplyOutcome::Applied);
    assert_eq!(second, ApplyOutcome::Applied);
    let thread = state.threads.get("t1").expect("thread must exist");
    assert_eq!(thread.cwd, "/repo/main");
    assert_eq!(thread.title.as_deref(), Some("Retried"));
    assert_eq!(thread.updated_at, 11);
}

#[test]
fn thread_started_backfills_placeholder_thread_cwd() {
    let mut state = AiState::default();

    state.apply_stream_event(event(
        1,
        Some("turn-start:r1"),
        ReducerEvent::TurnStarted {
            thread_id: "t1".to_string(),
            turn_id: "r1".to_string(),
        },
    ));
    state.apply_stream_event(event(
        2,
        Some("thread-start:t1"),
        ReducerEvent::ThreadStarted {
            thread_id: "t1".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Recovered".to_string()),
            created_at: Some(10),
            updated_at: Some(20),
        },
    ));

    let thread = state.threads.get("t1").expect("thread must exist");
    assert_eq!(thread.cwd, "/repo");
    assert_eq!(thread.title.as_deref(), Some("Recovered"));
    assert_eq!(thread.created_at, 10);
    assert_eq!(thread.updated_at, 20);
}

#[test]
fn item_start_backfills_turn_association_after_delta_first() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: None,
                updated_at: None,
            },
        ),
        event(
            2,
            Some("turn-start:r1"),
            ReducerEvent::TurnStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
        event(
            3,
            Some("item-delta:i1"),
            ReducerEvent::ItemDelta {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: "partial".to_string(),
            },
        ),
        event(
            4,
            Some("item-start:i1"),
            ReducerEvent::ItemStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "agentMessage".to_string(),
            },
        ),
    ]);

    let item = find_item(&state, "t1", "r1", "i1");
    assert_eq!(item.turn_id, "r1");
    assert_eq!(item.kind, "agentMessage");
    assert_eq!(item.content, "partial");
}

#[test]
fn thread_scoped_turn_and_item_ids_do_not_collide_across_threads() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: None,
                updated_at: None,
            },
        ),
        event(
            2,
            Some("turn-start:t1:r1"),
            ReducerEvent::TurnStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
        event(
            3,
            Some("item-start:t1:r1:i1"),
            ReducerEvent::ItemStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "agentMessage".to_string(),
            },
        ),
        event(
            4,
            Some("item-delta:t1:r1:i1"),
            ReducerEvent::ItemDelta {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: "thread-one".to_string(),
            },
        ),
        event(
            5,
            Some("thread-start:t2"),
            ReducerEvent::ThreadStarted {
                thread_id: "t2".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: None,
                updated_at: None,
            },
        ),
        event(
            6,
            Some("turn-start:t2:r1"),
            ReducerEvent::TurnStarted {
                thread_id: "t2".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
        event(
            7,
            Some("item-start:t2:r1:i1"),
            ReducerEvent::ItemStarted {
                thread_id: "t2".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "agentMessage".to_string(),
            },
        ),
        event(
            8,
            Some("item-delta:t2:r1:i1"),
            ReducerEvent::ItemDelta {
                thread_id: "t2".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                delta: "thread-two".to_string(),
            },
        ),
    ]);

    assert_eq!(
        state.turns.values().filter(|turn| turn.id == "r1").count(),
        2
    );
    assert_eq!(
        state.items.values().filter(|item| item.id == "i1").count(),
        2
    );

    assert_eq!(find_item(&state, "t1", "r1", "i1").content, "thread-one");
    assert_eq!(find_item(&state, "t2", "r1", "i1").content, "thread-two");
}

#[test]
fn item_display_metadata_updates_are_recorded() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: None,
                updated_at: None,
            },
        ),
        event(
            2,
            Some("turn-start:r1"),
            ReducerEvent::TurnStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
            },
        ),
        event(
            3,
            Some("item-start:i1"),
            ReducerEvent::ItemStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "dynamicToolCall".to_string(),
            },
        ),
        event(
            4,
            Some("item-display-metadata:i1"),
            ReducerEvent::ItemDisplayMetadataUpdated {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                metadata: ItemDisplayMetadata {
                    summary: Some("Called tool".to_string()),
                    details_json: Some("{\"tool\":\"search\"}".to_string()),
                },
            },
        ),
    ]);

    let item = find_item(&state, "t1", "r1", "i1");
    let metadata = item
        .display_metadata
        .as_ref()
        .expect("display metadata should be present");
    assert_eq!(metadata.summary.as_deref(), Some("Called tool"));
    assert_eq!(
        metadata.details_json.as_deref(),
        Some("{\"tool\":\"search\"}")
    );
}

#[test]
fn thread_metadata_updates_change_title_without_resetting_thread_fields() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: Some(10),
                updated_at: Some(20),
            },
        ),
        event(
            2,
            Some("thread-metadata:t1"),
            ReducerEvent::ThreadMetadataUpdated {
                thread_id: "t1".to_string(),
                title: Some("Summarized title".to_string()),
                updated_at: Some(30),
            },
        ),
    ]);

    let thread = state.threads.get("t1").expect("thread must exist");
    assert_eq!(thread.cwd, "/repo");
    assert_eq!(thread.created_at, 10);
    assert_eq!(thread.updated_at, 30);
    assert_eq!(thread.title.as_deref(), Some("Summarized title"));
}

#[test]
fn stale_item_display_metadata_update_is_ignored() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("item-start:i1"),
            ReducerEvent::ItemStarted {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "dynamicToolCall".to_string(),
            },
        ),
        event(
            5,
            Some("item-display-metadata:new"),
            ReducerEvent::ItemDisplayMetadataUpdated {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                metadata: ItemDisplayMetadata {
                    summary: Some("Latest".to_string()),
                    details_json: Some("{\"a\":1}".to_string()),
                },
            },
        ),
        event(
            4,
            Some("item-display-metadata:stale"),
            ReducerEvent::ItemDisplayMetadataUpdated {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                metadata: ItemDisplayMetadata {
                    summary: Some("Stale".to_string()),
                    details_json: Some("{\"a\":0}".to_string()),
                },
            },
        ),
    ]);

    let item = find_item(&state, "t1", "r1", "i1");
    let metadata = item
        .display_metadata
        .as_ref()
        .expect("display metadata should be present");
    assert_eq!(metadata.summary.as_deref(), Some("Latest"));
    assert_eq!(metadata.details_json.as_deref(), Some("{\"a\":1}"));
}

#[test]
fn stale_turn_diff_update_is_ignored() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            1,
            Some("thread-start:t1"),
            ReducerEvent::ThreadStarted {
                thread_id: "t1".to_string(),
                cwd: "/repo".to_string(),
                title: None,
                created_at: None,
                updated_at: None,
            },
        ),
        event(
            5,
            Some("turn-diff:new"),
            ReducerEvent::TurnDiffUpdated {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                diff: "latest diff".to_string(),
            },
        ),
        event(
            4,
            Some("turn-diff:stale"),
            ReducerEvent::TurnDiffUpdated {
                thread_id: "t1".to_string(),
                turn_id: "r1".to_string(),
                diff: "stale diff".to_string(),
            },
        ),
    ]);

    assert_eq!(
        state
            .turn_diffs
            .get(turn_storage_key("t1", "r1").as_str())
            .map(String::as_str),
        Some("latest diff")
    );
}

#[test]
fn later_unknown_server_request_resolution_does_not_allow_older_decision_regression() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            10,
            Some("server-request:accept"),
            ReducerEvent::ServerRequestResolved {
                request_id: "s1".to_string(),
                item_id: Some("i1".to_string()),
                decision: ServerRequestDecision::Accept,
            },
        ),
        event(
            20,
            Some("server-request:unknown"),
            ReducerEvent::ServerRequestResolved {
                request_id: "s1".to_string(),
                item_id: Some("i1".to_string()),
                decision: ServerRequestDecision::Unknown,
            },
        ),
    ]);

    let stale = state.apply_stream_event(event(
        15,
        Some("server-request:decline"),
        ReducerEvent::ServerRequestResolved {
            request_id: "s1".to_string(),
            item_id: Some("i1".to_string()),
            decision: ServerRequestDecision::Decline,
        },
    ));

    assert_eq!(stale, ApplyOutcome::Stale);
    let request = state
        .server_requests
        .get("s1")
        .expect("server request must exist");
    assert_eq!(request.decision, ServerRequestDecision::Accept);
    assert_eq!(request.sequence, 20);
}

#[test]
fn active_thread_persistence_hooks_round_trip() {
    let mut state = AiState::default();
    let mut store = InMemoryThreadStore::default();
    store
        .records
        .insert("/repo".to_string(), "thread-from-store".to_string());

    let loaded = state
        .hydrate_active_thread_for_cwd(&store, "/repo")
        .expect("load should succeed");
    assert_eq!(loaded.as_deref(), Some("thread-from-store"));
    assert_eq!(
        state.active_thread_for_cwd("/repo"),
        Some("thread-from-store")
    );

    state
        .persist_active_thread_for_cwd(&mut store, "/repo".to_string(), "thread-new".to_string())
        .expect("save should succeed");

    assert_eq!(state.active_thread_for_cwd("/repo"), Some("thread-new"));
    assert_eq!(
        store.records.get("/repo").map(String::as_str),
        Some("thread-new")
    );
}

#[derive(Default)]
struct InMemoryThreadStore {
    records: BTreeMap<String, String>,
}

impl ActiveThreadStore for InMemoryThreadStore {
    type Error = String;

    fn load_active_thread(&self, cwd: &str) -> Result<Option<String>, Self::Error> {
        Ok(self.records.get(cwd).cloned())
    }

    fn save_active_thread(&mut self, cwd: &str, thread_id: &str) -> Result<(), Self::Error> {
        self.records.insert(cwd.to_string(), thread_id.to_string());
        Ok(())
    }
}

fn event(sequence: u64, dedupe_key: Option<&str>, payload: ReducerEvent) -> StreamEvent {
    StreamEvent {
        sequence,
        dedupe_key: dedupe_key.map(ToOwned::to_owned),
        payload,
    }
}

fn find_turn<'a>(
    state: &'a AiState,
    thread_id: &str,
    turn_id: &str,
) -> &'a hunk_codex::state::TurnSummary {
    state
        .turns
        .values()
        .find(|turn| turn.thread_id == thread_id && turn.id == turn_id)
        .expect("turn must exist")
}

fn find_item<'a>(
    state: &'a AiState,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
) -> &'a hunk_codex::state::ItemSummary {
    state
        .items
        .values()
        .find(|item| item.thread_id == thread_id && item.turn_id == turn_id && item.id == item_id)
        .expect("item must exist")
}
