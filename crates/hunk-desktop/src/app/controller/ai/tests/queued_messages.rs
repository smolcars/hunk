fn queued_message(thread_id: &str, prompt: &str) -> AiQueuedUserMessage {
    AiQueuedUserMessage {
        thread_id: thread_id.to_string(),
        prompt: prompt.to_string(),
        local_images: Vec::new(),
        queued_at: Instant::now(),
        status: AiQueuedUserMessageStatus::Queued,
    }
}

fn pending_queued_message(
    thread_id: &str,
    prompt: &str,
    accepted_after_sequence: u64,
) -> AiQueuedUserMessage {
    AiQueuedUserMessage {
        thread_id: thread_id.to_string(),
        prompt: prompt.to_string(),
        local_images: Vec::new(),
        queued_at: Instant::now(),
        status: AiQueuedUserMessageStatus::PendingConfirmation {
            accepted_after_sequence,
        },
    }
}

fn queued_thread(
    thread_id: &str,
    status: ThreadLifecycleStatus,
    last_sequence: u64,
) -> ThreadSummary {
    ThreadSummary {
        id: thread_id.to_string(),
        cwd: "/repo".to_string(),
        title: None,
        status,
        created_at: last_sequence as i64,
        updated_at: last_sequence as i64,
        last_sequence,
    }
}

#[test]
fn take_last_editable_ai_queued_message_for_thread_uses_lifo_order() {
    let mut queued_messages = vec![
        queued_message("thread-a", "first"),
        queued_message("thread-b", "other"),
        pending_queued_message("thread-a", "pending", 3),
        queued_message("thread-a", "second"),
    ];

    let queued =
        take_last_editable_ai_queued_message_for_thread(&mut queued_messages, "thread-a")
            .expect("latest editable queued message should be returned");

    assert_eq!(queued.prompt, "second");
    assert_eq!(queued_messages.len(), 3);
    assert_eq!(queued_messages[0].prompt, "first");
    assert_eq!(queued_messages[1].prompt, "other");
    assert_eq!(queued_messages[2].prompt, "pending");
}

#[test]
fn ready_ai_queued_message_thread_ids_returns_fifo_threads_once_each() {
    let queued_messages = vec![
        queued_message("thread-a", "first"),
        queued_message("thread-a", "second"),
        queued_message("thread-b", "third"),
    ];
    let mut state = AiState::default();
    state.threads.insert(
        "thread-a".to_string(),
        queued_thread("thread-a", ThreadLifecycleStatus::Active, 1),
    );
    state.threads.insert(
        "thread-b".to_string(),
        queued_thread("thread-b", ThreadLifecycleStatus::Active, 2),
    );

    let ready =
        ready_ai_queued_message_thread_ids(queued_messages.as_slice(), &BTreeSet::new(), &state);

    assert_eq!(ready, vec!["thread-a".to_string(), "thread-b".to_string()]);
}

#[test]
fn ready_ai_queued_message_thread_ids_skips_in_progress_and_interrupt_restore_threads() {
    let queued_messages = vec![
        queued_message("thread-a", "first"),
        queued_message("thread-b", "second"),
        queued_message("thread-c", "third"),
    ];
    let mut state = AiState::default();
    for thread_id in ["thread-a", "thread-b", "thread-c"] {
        state.threads.insert(
            thread_id.to_string(),
            queued_thread(thread_id, ThreadLifecycleStatus::Active, 1),
        );
    }
    state.turns.insert(
        "thread-a::turn-1".to_string(),
        hunk_codex::state::TurnSummary {
            id: "turn-1".to_string(),
            thread_id: "thread-a".to_string(),
            status: hunk_codex::state::TurnStatus::InProgress,
            last_sequence: 3,
        },
    );
    let interrupt_restore_thread_ids =
        ["thread-b".to_string()].into_iter().collect::<BTreeSet<_>>();

    let ready = ready_ai_queued_message_thread_ids(
        queued_messages.as_slice(),
        &interrupt_restore_thread_ids,
        &state,
    );

    assert_eq!(ready, vec!["thread-c".to_string()]);
}

#[test]
fn ready_ai_queued_message_thread_ids_skips_non_active_and_pending_confirmation_threads() {
    let queued_messages = vec![
        queued_message("thread-archived", "archived"),
        queued_message("thread-closed", "closed"),
        queued_message("thread-not-loaded", "not-loaded"),
        pending_queued_message("thread-pending", "pending", 5),
        queued_message("thread-active", "active"),
        queued_message("thread-idle", "idle"),
    ];
    let mut state = AiState::default();
    state.threads.insert(
        "thread-archived".to_string(),
        queued_thread("thread-archived", ThreadLifecycleStatus::Archived, 1),
    );
    state.threads.insert(
        "thread-closed".to_string(),
        queued_thread("thread-closed", ThreadLifecycleStatus::Closed, 2),
    );
    state.threads.insert(
        "thread-not-loaded".to_string(),
        queued_thread("thread-not-loaded", ThreadLifecycleStatus::NotLoaded, 3),
    );
    state.threads.insert(
        "thread-pending".to_string(),
        queued_thread("thread-pending", ThreadLifecycleStatus::Active, 4),
    );
    state.threads.insert(
        "thread-active".to_string(),
        queued_thread("thread-active", ThreadLifecycleStatus::Active, 5),
    );
    state.threads.insert(
        "thread-idle".to_string(),
        queued_thread("thread-idle", ThreadLifecycleStatus::Idle, 6),
    );

    let ready =
        ready_ai_queued_message_thread_ids(queued_messages.as_slice(), &BTreeSet::new(), &state);

    assert_eq!(
        ready,
        vec!["thread-active".to_string(), "thread-idle".to_string()]
    );
}

#[test]
fn reconcile_ai_queued_messages_after_snapshot_confirms_pending_messages() {
    let mut queued_messages = vec![
        pending_queued_message("thread-a", "first", 1),
        queued_message("thread-a", "second"),
    ];
    let mut state = AiState::default();
    state.threads.insert(
        "thread-a".to_string(),
        queued_thread("thread-a", ThreadLifecycleStatus::Active, 2),
    );
    state.items.insert(
        "thread-a::item-1".to_string(),
        timeline_tool_item(
            "item-1",
            "thread-a",
            "turn-1",
            "userMessage",
            ItemStatus::Completed,
            "first",
            "{}",
            2,
        ),
    );

    let restored =
        reconcile_ai_queued_messages_after_snapshot(&mut queued_messages, &mut BTreeSet::new(), &state);

    assert!(restored.is_empty());
    assert_eq!(queued_messages.len(), 1);
    assert_eq!(queued_messages[0].prompt, "second");
    assert_eq!(queued_messages[0].status, AiQueuedUserMessageStatus::Queued);
}

#[test]
fn reconcile_ai_queued_messages_after_snapshot_restores_unconfirmed_pending_messages() {
    let mut queued_messages = vec![pending_queued_message("thread-a", "first", 1)];
    let mut state = AiState::default();
    state.threads.insert(
        "thread-a".to_string(),
        queued_thread("thread-a", ThreadLifecycleStatus::Active, 2),
    );

    let restored =
        reconcile_ai_queued_messages_after_snapshot(&mut queued_messages, &mut BTreeSet::new(), &state);

    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].prompt, "first");
    assert!(queued_messages.is_empty());
}

#[test]
fn reconcile_ai_queued_messages_after_snapshot_restores_messages_after_interrupt_finishes() {
    let mut queued_messages = vec![
        queued_message("thread-a", "first"),
        queued_message("thread-b", "second"),
        queued_message("thread-a", "third"),
    ];
    let mut state = AiState::default();
    state.threads.insert(
        "thread-a".to_string(),
        queued_thread("thread-a", ThreadLifecycleStatus::Active, 1),
    );
    state.threads.insert(
        "thread-b".to_string(),
        queued_thread("thread-b", ThreadLifecycleStatus::Active, 2),
    );
    let mut interrupt_restore_thread_ids =
        ["thread-a".to_string()].into_iter().collect::<BTreeSet<_>>();

    let restored = reconcile_ai_queued_messages_after_snapshot(
        &mut queued_messages,
        &mut interrupt_restore_thread_ids,
        &state,
    );

    assert_eq!(restored.len(), 2);
    assert_eq!(queued_messages.len(), 1);
    assert_eq!(queued_messages[0].thread_id, "thread-b");
    assert!(interrupt_restore_thread_ids.is_empty());
}
