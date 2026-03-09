#[allow(dead_code)]
#[path = "../src/app/ai_thread_flow.rs"]
mod ai_thread_flow;

use ai_thread_flow::{ai_branch_name_for_prompt, ai_commit_subject_for_thread};
use hunk_codex::state::{AiState, ItemStatus, ItemSummary};

fn item(
    item_id: &str,
    thread_id: &str,
    kind: &str,
    content: &str,
    last_sequence: u64,
) -> ItemSummary {
    ItemSummary {
        id: item_id.to_string(),
        thread_id: thread_id.to_string(),
        turn_id: "turn-1".to_string(),
        kind: kind.to_string(),
        status: ItemStatus::Completed,
        content: content.to_string(),
        display_metadata: None,
        last_sequence,
    }
}

#[test]
fn branch_name_for_prompt_uses_mode_prefix() {
    let local = ai_branch_name_for_prompt("Add OAuth login flow", false);
    let worktree = ai_branch_name_for_prompt("Add OAuth login flow", true);

    assert!(local.starts_with("ai/local/"));
    assert!(worktree.starts_with("ai/worktree/"));
}

#[test]
fn branch_name_for_prompt_filters_noise_words() {
    let branch = ai_branch_name_for_prompt(
        "Implement the ability to add and remove reviewers in the PR panel",
        false,
    );
    assert!(branch.starts_with("ai/local/implement-ability-add-remove-reviewers-pr"));
}

#[test]
fn commit_subject_for_thread_prefers_latest_agent_message_line() {
    let mut state = AiState::default();
    state.items.insert(
        "item-1".to_string(),
        item(
            "item-1",
            "thread-1",
            "agentMessage",
            "Added branch/worktree selection to New.\nAlso removed dropdown.",
            12,
        ),
    );
    state.items.insert(
        "item-2".to_string(),
        item(
            "item-2",
            "thread-1",
            "agentMessage",
            "Refined timeline header.",
            22,
        ),
    );

    let subject = ai_commit_subject_for_thread(&state, "thread-1", "feature/old");
    assert_eq!(subject, "Refined timeline header");
}

#[test]
fn commit_subject_for_thread_falls_back_to_branch_name() {
    let state = AiState::default();
    let subject = ai_commit_subject_for_thread(&state, "thread-1", "feature/open-pr-flow");
    assert_eq!(subject, "Update open pr flow");
}
