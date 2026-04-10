fn followup_thread(thread_id: &str) -> ThreadSummary {
    ThreadSummary {
        id: thread_id.to_string(),
        cwd: "/repo".to_string(),
        title: Some("Thread".to_string()),
        status: ThreadLifecycleStatus::Idle,
        created_at: 0,
        updated_at: 0,
        last_sequence: 0,
    }
}

fn followup_state(thread_id: &str) -> AiState {
    let mut state = AiState::default();
    state
        .threads
        .insert(thread_id.to_string(), followup_thread(thread_id));
    state
}

fn insert_plan(
    state: &mut AiState,
    thread_id: &str,
    turn_id: &str,
    last_sequence: u64,
) {
    state.turns.insert(
        hunk_codex::state::turn_storage_key(thread_id, turn_id),
        hunk_codex::state::TurnSummary {
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: hunk_codex::state::TurnStatus::Completed,
            last_sequence,
        },
    );
    state.turn_plans.insert(
        hunk_codex::state::turn_storage_key(thread_id, turn_id),
        hunk_codex::state::TurnPlanSummary {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            explanation: Some("Implement the popup".to_string()),
            steps: vec![hunk_codex::state::TurnPlanStepSummary {
                step: "Render follow-up actions".to_string(),
                status: hunk_codex::state::TurnPlanStepStatus::Completed,
            }],
            created_sequence: last_sequence,
            last_sequence,
        },
    );
}

fn insert_plan_item(
    state: &mut AiState,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    last_sequence: u64,
) {
    state.turns.insert(
        hunk_codex::state::turn_storage_key(thread_id, turn_id),
        hunk_codex::state::TurnSummary {
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: hunk_codex::state::TurnStatus::Completed,
            last_sequence,
        },
    );
    state.items.insert(
        hunk_codex::state::item_storage_key(thread_id, turn_id, item_id),
        hunk_codex::state::ItemSummary {
            id: item_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: "plan".to_string(),
            status: hunk_codex::state::ItemStatus::Completed,
            content: "# Proposed Plan\n\n- Ship the popup".to_string(),
            display_metadata: None,
            last_sequence,
        },
    );
}

fn insert_review_exit_item(
    state: &mut AiState,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    last_sequence: u64,
) {
    state.turns.insert(
        hunk_codex::state::turn_storage_key(thread_id, turn_id),
        hunk_codex::state::TurnSummary {
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: hunk_codex::state::TurnStatus::Completed,
            last_sequence,
        },
    );
    state.items.insert(
        hunk_codex::state::item_storage_key(thread_id, turn_id, item_id),
        hunk_codex::state::ItemSummary {
            id: item_id.to_string(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: "exitedReviewMode".to_string(),
            status: hunk_codex::state::ItemStatus::Completed,
            content: "Fix the mode reset".to_string(),
            display_metadata: None,
            last_sequence,
        },
    );
}

#[test]
fn ai_cycle_composer_mode_target_cycles_code_plan_review() {
    assert_eq!(
        ai_cycle_composer_mode_target(false, AiCollaborationModeSelection::Default),
        AiComposerModeTarget::Plan
    );
    assert_eq!(
        ai_cycle_composer_mode_target(false, AiCollaborationModeSelection::Plan),
        AiComposerModeTarget::Review
    );
    assert_eq!(
        ai_cycle_composer_mode_target(true, AiCollaborationModeSelection::Default),
        AiComposerModeTarget::Code
    );
}

#[test]
fn ai_followup_prompt_action_for_keystroke_matches_navigation_keys() {
    assert_eq!(
        ai_followup_prompt_action_for_keystroke(
            &Keystroke::parse("left").expect("valid keystroke")
        ),
        Some(super::AiFollowupPromptKeystrokeAction::SelectPrevious)
    );
    assert_eq!(
        ai_followup_prompt_action_for_keystroke(
            &Keystroke::parse("down").expect("valid keystroke")
        ),
        Some(super::AiFollowupPromptKeystrokeAction::SelectNext)
    );
    assert_eq!(
        ai_followup_prompt_action_for_keystroke(
            &Keystroke::parse("enter").expect("valid keystroke")
        ),
        Some(super::AiFollowupPromptKeystrokeAction::Accept)
    );
}

#[test]
fn ai_followup_prompt_for_thread_returns_latest_plan_in_plan_mode() {
    let mut state = followup_state("thread-1");
    insert_plan(&mut state, "thread-1", "turn-1", 9);

    assert_eq!(
        ai_followup_prompt_for_thread(
            &state,
            "thread-1",
            AiCollaborationModeSelection::Plan,
            AiThreadFollowupPromptState::default(),
        ),
        Some(AiFollowupPrompt {
            kind: AiFollowupPromptKind::Plan,
            source_sequence: 9,
        })
    );
}

#[test]
fn ai_followup_prompt_for_thread_uses_plan_items_when_turn_plan_updates_are_absent() {
    let mut state = followup_state("thread-1");
    insert_plan_item(&mut state, "thread-1", "turn-1", "plan-item", 9);

    assert_eq!(
        ai_followup_prompt_for_thread(
            &state,
            "thread-1",
            AiCollaborationModeSelection::Plan,
            AiThreadFollowupPromptState::default(),
        ),
        Some(AiFollowupPrompt {
            kind: AiFollowupPromptKind::Plan,
            source_sequence: 9,
        })
    );
}

#[test]
fn ai_followup_prompt_for_thread_hides_plan_outside_plan_mode() {
    let mut state = followup_state("thread-1");
    insert_plan(&mut state, "thread-1", "turn-1", 9);

    assert_eq!(
        ai_followup_prompt_for_thread(
            &state,
            "thread-1",
            AiCollaborationModeSelection::Default,
            AiThreadFollowupPromptState::default(),
        ),
        None
    );
}

#[test]
fn ai_followup_prompt_for_thread_ignores_review_exit_items() {
    let mut state = followup_state("thread-1");
    insert_review_exit_item(&mut state, "thread-1", "turn-2", "review-exit", 14);

    assert_eq!(
        ai_followup_prompt_for_thread(
            &state,
            "thread-1",
            AiCollaborationModeSelection::Plan,
            AiThreadFollowupPromptState::default(),
        ),
        None
    );
}

#[test]
fn sync_ai_review_mode_threads_after_snapshot_clears_completed_review_threads() {
    let mut state = followup_state("thread-1");
    insert_review_exit_item(&mut state, "thread-1", "turn-2", "review-exit", 14);
    let mut review_mode_thread_ids = BTreeSet::from([String::from("thread-1")]);

    sync_ai_review_mode_threads_after_snapshot(&mut review_mode_thread_ids, &state);

    assert!(review_mode_thread_ids.is_empty());
}

#[test]
fn sync_ai_followup_prompt_ui_state_resets_selection_for_new_prompt() {
    let mut state = followup_state("thread-1");
    insert_plan(&mut state, "thread-1", "turn-1", 11);
    let mut prompt_states = BTreeMap::from([(
        String::from("thread-1"),
        AiThreadFollowupPromptState {
            plan_acknowledged_sequence: 0,
            prompt_source_sequence: 3,
            selected_action: AiFollowupPromptAction::Secondary,
        },
    )]);

    sync_ai_followup_prompt_ui_state(
        &mut prompt_states,
        &state,
        Some("thread-1"),
        AiCollaborationModeSelection::Plan,
    );

    assert_eq!(
        prompt_states.get("thread-1"),
        Some(&AiThreadFollowupPromptState {
            plan_acknowledged_sequence: 0,
            prompt_source_sequence: 11,
            selected_action: AiFollowupPromptAction::Primary,
        })
    );
}
