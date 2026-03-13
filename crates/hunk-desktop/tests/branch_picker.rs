#[allow(dead_code)]
#[path = "../src/app/branch_picker.rs"]
mod branch_picker;
#[allow(dead_code)]
#[path = "../src/app/fuzzy_match.rs"]
mod fuzzy_match;

use branch_picker::{branch_detail_labels, branch_match_score, matched_branch_names};
use hunk_git::git::LocalBranch;

fn branch(name: &str, is_current: bool, tip_unix_time: Option<i64>) -> LocalBranch {
    LocalBranch {
        name: name.to_string(),
        is_current,
        tip_unix_time,
        attached_workspace_target_id: None,
        attached_workspace_target_root: None,
        attached_workspace_target_label: None,
    }
}

#[test]
fn exact_and_prefix_matches_beat_segment_matches() {
    let exact = branch_match_score("auth", "auth").expect("exact match should score");
    let prefix = branch_match_score("auth", "auth-ui").expect("prefix match should score");
    let segment = branch_match_score("auth", "feature/auth").expect("segment match should score");

    assert!(exact > prefix);
    assert!(prefix > segment);
}

#[test]
fn fuzzy_matching_handles_scattered_branch_characters() {
    assert!(branch_match_score("fbui", "feature/branch-ui").is_some());
    assert!(branch_match_score("zzz", "feature/branch-ui").is_none());
}

#[test]
fn blank_query_preserves_existing_branch_order() {
    let branches = vec![
        branch("main", true, Some(300)),
        branch("feature/auth-ui", false, Some(200)),
        branch("bugfix/auth", false, Some(100)),
    ];

    assert_eq!(
        matched_branch_names(&branches, ""),
        vec![
            "main".to_string(),
            "feature/auth-ui".to_string(),
            "bugfix/auth".to_string(),
        ]
    );
}

#[test]
fn exact_then_prefix_then_segment_matches_are_sorted_first() {
    let branches = vec![
        branch("feature/auth", false, Some(200)),
        branch("auth", false, Some(100)),
        branch("auth-ui", false, Some(300)),
    ];

    assert_eq!(
        matched_branch_names(&branches, "auth"),
        vec![
            "auth".to_string(),
            "auth-ui".to_string(),
            "feature/auth".to_string(),
        ]
    );
}

#[test]
fn occupied_branch_detail_mentions_worktree_label() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_secs() as i64;
    let branches = vec![
        branch("main", true, Some(now - 5 * 60)),
        LocalBranch {
            name: "feature/auth".to_string(),
            is_current: false,
            tip_unix_time: Some(now - 3 * 60),
            attached_workspace_target_id: Some("worktree:worktree-2".to_string()),
            attached_workspace_target_root: None,
            attached_workspace_target_label: Some("worktree-2".to_string()),
        },
    ];

    let details = branch_detail_labels(&branches);
    assert_eq!(details[0], "5m ago");
    assert_eq!(details[1], "Checked out in worktree-2 • 3m ago");
}
