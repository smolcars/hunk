#[test]
fn take_last_ai_queued_message_for_thread_uses_lifo_order() {
    let queued_at = Instant::now();
    let mut queued_messages = vec![
        AiQueuedUserMessage {
            thread_id: "thread-a".to_string(),
            prompt: "first".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-b".to_string(),
            prompt: "other".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-a".to_string(),
            prompt: "second".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
    ];

    let queued = take_last_ai_queued_message_for_thread(&mut queued_messages, "thread-a")
        .expect("latest queued message should be returned");

    assert_eq!(queued.prompt, "second");
    assert_eq!(queued_messages.len(), 2);
    assert_eq!(queued_messages[0].prompt, "first");
    assert_eq!(queued_messages[1].prompt, "other");
}

#[test]
fn ready_ai_queued_message_thread_ids_returns_fifo_threads_once_each() {
    let queued_at = Instant::now();
    let queued_messages = vec![
        AiQueuedUserMessage {
            thread_id: "thread-a".to_string(),
            prompt: "first".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-a".to_string(),
            prompt: "second".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-b".to_string(),
            prompt: "third".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
    ];
    let mut state = AiState::default();
    state.threads.insert(
        "thread-a".to_string(),
        ThreadSummary {
            id: "thread-a".to_string(),
            cwd: "/repo".to_string(),
            title: None,
            status: ThreadLifecycleStatus::Active,
            created_at: 1,
            updated_at: 1,
            last_sequence: 1,
        },
    );
    state.threads.insert(
        "thread-b".to_string(),
        ThreadSummary {
            id: "thread-b".to_string(),
            cwd: "/repo".to_string(),
            title: None,
            status: ThreadLifecycleStatus::Active,
            created_at: 2,
            updated_at: 2,
            last_sequence: 2,
        },
    );

    let ready =
        ready_ai_queued_message_thread_ids(queued_messages.as_slice(), &BTreeSet::new(), &state);

    assert_eq!(ready, vec!["thread-a".to_string(), "thread-b".to_string()]);
}

#[test]
fn ready_ai_queued_message_thread_ids_skips_in_progress_and_interrupt_restore_threads() {
    let queued_at = Instant::now();
    let queued_messages = vec![
        AiQueuedUserMessage {
            thread_id: "thread-a".to_string(),
            prompt: "first".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-b".to_string(),
            prompt: "second".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-c".to_string(),
            prompt: "third".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
    ];
    let mut state = AiState::default();
    for thread_id in ["thread-a", "thread-b", "thread-c"] {
        state.threads.insert(
            thread_id.to_string(),
            ThreadSummary {
                id: thread_id.to_string(),
                cwd: "/repo".to_string(),
                title: None,
                status: ThreadLifecycleStatus::Active,
                created_at: 1,
                updated_at: 1,
                last_sequence: 1,
            },
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
fn take_interrupted_ai_queued_messages_restores_messages_after_interrupt_finishes() {
    let queued_at = Instant::now();
    let mut queued_messages = vec![
        AiQueuedUserMessage {
            thread_id: "thread-a".to_string(),
            prompt: "first".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-b".to_string(),
            prompt: "second".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
        AiQueuedUserMessage {
            thread_id: "thread-a".to_string(),
            prompt: "third".to_string(),
            local_images: Vec::new(),
            queued_at,
        },
    ];
    let mut state = AiState::default();
    state.threads.insert(
        "thread-a".to_string(),
        ThreadSummary {
            id: "thread-a".to_string(),
            cwd: "/repo".to_string(),
            title: None,
            status: ThreadLifecycleStatus::Active,
            created_at: 1,
            updated_at: 1,
            last_sequence: 1,
        },
    );
    state.threads.insert(
        "thread-b".to_string(),
        ThreadSummary {
            id: "thread-b".to_string(),
            cwd: "/repo".to_string(),
            title: None,
            status: ThreadLifecycleStatus::Active,
            created_at: 2,
            updated_at: 2,
            last_sequence: 2,
        },
    );
    let mut interrupt_restore_thread_ids =
        ["thread-a".to_string()].into_iter().collect::<BTreeSet<_>>();

    let restored = take_interrupted_ai_queued_messages(
        &mut queued_messages,
        &mut interrupt_restore_thread_ids,
        &state,
    );

    assert_eq!(restored.len(), 2);
    assert_eq!(queued_messages.len(), 1);
    assert_eq!(queued_messages[0].thread_id, "thread-b");
    assert!(interrupt_restore_thread_ids.is_empty());
}
