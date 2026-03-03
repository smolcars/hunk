use hunk_domain::db::{
    CommentLineSide, CommentRecord, CommentStatus, format_comment_clipboard_blob,
};

fn sample_comment() -> CommentRecord {
    CommentRecord {
        id: "comment-1".to_string(),
        repo_root: "/repo".to_string(),
        bookmark_name: "main".to_string(),
        created_head_commit: Some("abc123".to_string()),
        status: CommentStatus::Open,
        file_path: "src/lib.rs".to_string(),
        line_side: CommentLineSide::Right,
        old_line: Some(14),
        new_line: Some(16),
        row_stable_id: Some(10),
        hunk_header: Some("@@ -14,3 +16,4 @@".to_string()),
        line_text: "+let value = parse(input)?;".to_string(),
        context_before: " let input = payload.trim();".to_string(),
        context_after: " return Ok(value);".to_string(),
        anchor_hash: "deadbeefcafebabe".to_string(),
        comment_text: "Handle invalid input here.".to_string(),
        stale_reason: None,
        created_at_unix_ms: 10,
        updated_at_unix_ms: 10,
        last_seen_at_unix_ms: None,
        resolved_at_unix_ms: None,
    }
}

#[test]
fn clipboard_blob_matches_expected_format() {
    let blob = format_comment_clipboard_blob(&sample_comment());
    let expected = concat!(
        "[Hunk Comment]\n",
        "File: src/lib.rs\n",
        "Lines: old 14 | new 16\n",
        "Comment:\n",
        "Handle invalid input here.\n",
        "Snippet:\n",
        " let input = payload.trim();\n",
        "+let value = parse(input)?;\n",
        " return Ok(value);",
    );

    assert_eq!(blob, expected);
}

#[test]
fn clipboard_blob_omits_empty_context_sections() {
    let mut comment = sample_comment();
    comment.context_before.clear();
    comment.context_after.clear();

    let blob = format_comment_clipboard_blob(&comment);
    assert!(blob.ends_with("Snippet:\n+let value = parse(input)?;"));
}

#[test]
fn clipboard_blob_keeps_tight_context_window() {
    let mut comment = sample_comment();
    comment.context_before = " first before\n second before\n third before".to_string();
    comment.context_after = " first after\n second after\n third after".to_string();

    let blob = format_comment_clipboard_blob(&comment);
    assert!(blob.contains("Snippet:\n third before\n+let value = parse(input)?;\n first after"));
    assert!(!blob.contains(" first before"));
    assert!(!blob.contains(" second after"));
}
