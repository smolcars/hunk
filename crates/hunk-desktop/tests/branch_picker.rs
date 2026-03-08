#[allow(dead_code)]
#[path = "../src/app/branch_picker.rs"]
mod branch_picker;

use branch_picker::{branch_match_score, matched_branch_names};
use hunk_git::git::LocalBranch;

fn branch(name: &str, is_current: bool, tip_unix_time: Option<i64>) -> LocalBranch {
    LocalBranch {
        name: name.to_string(),
        is_current,
        tip_unix_time,
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
