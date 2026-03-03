use hunk_domain::db::{CommentStatus, next_status_for_unmatched_anchor};

#[test]
fn unmatched_anchor_in_changed_file_becomes_stale() {
    let (status, reason) = next_status_for_unmatched_anchor(true);
    assert_eq!(status, CommentStatus::Stale);
    assert_eq!(reason, Some("anchor_not_found"));
}

#[test]
fn unmatched_anchor_in_unchanged_file_becomes_resolved() {
    let (status, reason) = next_status_for_unmatched_anchor(false);
    assert_eq!(status, CommentStatus::Resolved);
    assert_eq!(reason, None);
}
