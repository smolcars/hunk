#[test]
fn incremental_streaming_change_set_accepts_item_content_updates() {
    let item_key = hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-1");
    let turn_key = hunk_codex::state::turn_storage_key("thread-1", "turn-1");
    let mut previous = AiState::default();
    previous.threads.insert(
        "thread-1".to_string(),
        ThreadSummary {
            id: "thread-1".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Thread".to_string()),
            status: ThreadLifecycleStatus::Active,
            created_at: 1,
            updated_at: 1,
            last_sequence: 1,
        },
    );
    previous.turns.insert(
        turn_key.clone(),
        hunk_codex::state::TurnSummary {
            id: "turn-1".to_string(),
            thread_id: "thread-1".to_string(),
            collaboration_mode: None,
            status: hunk_codex::state::TurnStatus::InProgress,
            last_sequence: 1,
        },
    );
    previous.items.insert(
        item_key.clone(),
        timeline_tool_item(
            "item-1",
            "thread-1",
            "turn-1",
            "agentMessage",
            ItemStatus::Streaming,
            "Hello",
            "{}",
            1,
        ),
    );

    let mut next = previous.clone();
    next.threads.get_mut("thread-1").expect("thread").updated_at = 2;
    next.threads.get_mut("thread-1").expect("thread").last_sequence = 2;
    next.turns.get_mut(turn_key.as_str()).expect("turn").last_sequence = 2;
    let item = next.items.get_mut(item_key.as_str()).expect("item");
    item.content = "Hello world".to_string();
    item.last_sequence = 2;

    let changes = super::ai_incremental_streaming_change_set(&previous, &next)
        .expect("content-only update should use incremental streaming");

    assert_eq!(changes.changed_item_keys, BTreeSet::from([item_key]));
}

#[test]
fn incremental_streaming_change_set_rejects_new_item_keys() {
    let item_key = hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-1");
    let turn_key = hunk_codex::state::turn_storage_key("thread-1", "turn-1");
    let mut previous = AiState::default();
    previous.threads.insert(
        "thread-1".to_string(),
        ThreadSummary {
            id: "thread-1".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Thread".to_string()),
            status: ThreadLifecycleStatus::Active,
            created_at: 1,
            updated_at: 1,
            last_sequence: 1,
        },
    );
    previous.turns.insert(
        turn_key.clone(),
        hunk_codex::state::TurnSummary {
            id: "turn-1".to_string(),
            thread_id: "thread-1".to_string(),
            collaboration_mode: None,
            status: hunk_codex::state::TurnStatus::InProgress,
            last_sequence: 1,
        },
    );
    previous.items.insert(
        item_key,
        timeline_tool_item(
            "item-1",
            "thread-1",
            "turn-1",
            "agentMessage",
            ItemStatus::Streaming,
            "Hello",
            "{}",
            1,
        ),
    );

    let mut next = previous.clone();
    next.threads.get_mut("thread-1").expect("thread").updated_at = 2;
    next.threads.get_mut("thread-1").expect("thread").last_sequence = 2;
    next.turns.get_mut(turn_key.as_str()).expect("turn").last_sequence = 2;
    next.items.insert(
        hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-2"),
        timeline_tool_item(
            "item-2",
            "thread-1",
            "turn-1",
            "agentMessage",
            ItemStatus::Streaming,
            "Second",
            "{}",
            2,
        ),
    );

    assert!(super::ai_incremental_streaming_change_set(&previous, &next).is_none());
}

#[test]
fn incremental_streaming_change_set_rejects_renderability_flips() {
    let item_key = hunk_codex::state::item_storage_key("thread-1", "turn-1", "item-1");
    let turn_key = hunk_codex::state::turn_storage_key("thread-1", "turn-1");
    let mut previous = AiState::default();
    previous.threads.insert(
        "thread-1".to_string(),
        ThreadSummary {
            id: "thread-1".to_string(),
            cwd: "/repo".to_string(),
            title: Some("Thread".to_string()),
            status: ThreadLifecycleStatus::Active,
            created_at: 1,
            updated_at: 1,
            last_sequence: 1,
        },
    );
    previous.turns.insert(
        turn_key.clone(),
        hunk_codex::state::TurnSummary {
            id: "turn-1".to_string(),
            thread_id: "thread-1".to_string(),
            collaboration_mode: None,
            status: hunk_codex::state::TurnStatus::InProgress,
            last_sequence: 1,
        },
    );
    previous.items.insert(
        item_key.clone(),
        hunk_codex::state::ItemSummary {
            id: "item-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            kind: "reasoning".to_string(),
            status: ItemStatus::Streaming,
            content: String::new(),
            display_metadata: None,
            last_sequence: 1,
        },
    );

    let mut next = previous.clone();
    next.threads.get_mut("thread-1").expect("thread").updated_at = 2;
    next.threads.get_mut("thread-1").expect("thread").last_sequence = 2;
    next.turns.get_mut(turn_key.as_str()).expect("turn").last_sequence = 2;
    let item = next.items.get_mut(item_key.as_str()).expect("item");
    item.content = "Thinking".to_string();
    item.last_sequence = 2;

    assert!(super::ai_incremental_streaming_change_set(&previous, &next).is_none());
}
