use hunk_domain::db::compute_comment_anchor_hash;

#[test]
fn anchor_hash_is_deterministic_for_same_input() {
    let file_path = "src/main.rs";
    let hunk_header = Some("@@ -12,6 +12,8 @@");
    let line_text = "+let total = subtotal + tax;";
    let context_before = " let subtotal = 10;\n let tax = 2;";
    let context_after = " return total;";

    let first = compute_comment_anchor_hash(
        file_path,
        hunk_header,
        line_text,
        context_before,
        context_after,
    );

    for _ in 0..8 {
        let next = compute_comment_anchor_hash(
            file_path,
            hunk_header,
            line_text,
            context_before,
            context_after,
        );
        assert_eq!(next, first);
    }

    assert_eq!(first.len(), 16);
    assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn anchor_hash_changes_when_anchor_content_changes() {
    let baseline = compute_comment_anchor_hash(
        "src/main.rs",
        Some("@@ -1,2 +1,2 @@"),
        "+let total = subtotal + tax;",
        " let subtotal = 10;",
        " return total;",
    );
    let changed_context = compute_comment_anchor_hash(
        "src/main.rs",
        Some("@@ -1,2 +1,2 @@"),
        "+let total = subtotal + tax;",
        " let subtotal = 11;",
        " return total;",
    );

    assert_ne!(baseline, changed_context);
}
