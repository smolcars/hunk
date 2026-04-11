#[test]
fn composer_status_message_for_visible_target_prefers_thread_status() {
    let statuses = BTreeMap::from([(
        AiComposerDraftKey::Thread("thread-1".to_string()),
        "Thread warning".to_string(),
    )]);

    assert_eq!(
        composer_status_message_for_visible_target(
            &statuses,
            Some(&AiComposerDraftKey::Thread("thread-1".to_string())),
            Some("Workspace warning"),
        ),
        Some("Thread warning")
    );
}

#[test]
fn composer_status_message_for_visible_target_does_not_fall_back_to_workspace_for_threads() {
    let statuses = BTreeMap::new();

    assert_eq!(
        composer_status_message_for_visible_target(
            &statuses,
            Some(&AiComposerDraftKey::Thread("thread-2".to_string())),
            Some("Workspace warning"),
        ),
        None
    );
}

#[test]
fn composer_status_message_for_visible_target_uses_workspace_status_without_selected_thread() {
    let statuses = BTreeMap::new();

    assert_eq!(
        composer_status_message_for_visible_target(&statuses, None, Some("Workspace warning")),
        Some("Workspace warning")
    );
}
