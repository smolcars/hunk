use std::collections::BTreeMap;

use hunk_codex::state::ActiveThreadStore;
use hunk_codex::state::AiState;
use hunk_codex::state::ApplyOutcome;
use hunk_codex::state::ItemStatus;
use hunk_codex::state::ReducerEvent;
use hunk_codex::state::ServerRequestDecision;
use hunk_codex::state::StreamEvent;
use hunk_codex::state::ThreadLifecycleStatus;
use hunk_codex::state::TurnStatus;

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
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "agentMessage".to_string(),
            },
        ),
        event(
            4,
            Some("item-delta:i1:1"),
            ReducerEvent::ItemDelta {
                item_id: "i1".to_string(),
                delta: "Hello".to_string(),
            },
        ),
        event(
            5,
            Some("item-delta:i1:2"),
            ReducerEvent::ItemDelta {
                item_id: "i1".to_string(),
                delta: " World".to_string(),
            },
        ),
        event(
            6,
            Some("item-completed:i1"),
            ReducerEvent::ItemCompleted {
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
                turn_id: "r1".to_string(),
            },
        ),
    ]);

    let thread = state.threads.get("t1").expect("thread must exist");
    assert_eq!(thread.cwd, "/repo");
    assert_eq!(thread.status, ThreadLifecycleStatus::Idle);

    let turn = state.turns.get("r1").expect("turn must exist");
    assert_eq!(turn.status, TurnStatus::Completed);

    let item = state.items.get("i1").expect("item must exist");
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
fn out_of_order_and_duplicate_events_are_idempotent() {
    let mut state = AiState::default();

    state.apply_stream_events(vec![
        event(
            30,
            Some("item-start:i1"),
            ReducerEvent::ItemStarted {
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
                item_id: "i1".to_string(),
                delta: "A".to_string(),
            },
        ),
        event(
            41,
            Some("item-delta:i1:1"),
            ReducerEvent::ItemDelta {
                item_id: "i1".to_string(),
                delta: "A".to_string(),
            },
        ),
        event(
            42,
            Some("item-delta:i1:2"),
            ReducerEvent::ItemDelta {
                item_id: "i1".to_string(),
                delta: "B".to_string(),
            },
        ),
        event(
            43,
            Some("item-completed:i1"),
            ReducerEvent::ItemCompleted {
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

    let item = state.items.get("i1").expect("item must exist");
    assert_eq!(item.content, "AB");
    assert_eq!(item.status, ItemStatus::Completed);

    let duplicate_result = state.apply_stream_event(event(
        60,
        Some("item-delta:i1:2"),
        ReducerEvent::ItemDelta {
            item_id: "i1".to_string(),
            delta: "ignored".to_string(),
        },
    ));
    assert_eq!(duplicate_result, ApplyOutcome::Duplicate);
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
                item_id: "i1".to_string(),
                delta: "partial".to_string(),
            },
        ),
        event(
            4,
            Some("item-start:i1"),
            ReducerEvent::ItemStarted {
                turn_id: "r1".to_string(),
                item_id: "i1".to_string(),
                kind: "agentMessage".to_string(),
            },
        ),
    ]);

    let item = state.items.get("i1").expect("item should exist");
    assert_eq!(item.turn_id, "r1");
    assert_eq!(item.kind, "agentMessage");
    assert_eq!(item.content, "partial");
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
