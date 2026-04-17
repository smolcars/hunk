pub fn unified_diff_line_counts(diff_text: &str) -> (usize, usize) {
    let mut added = 0usize;
    let mut removed = 0usize;

    for line in diff_text.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added = added.saturating_add(1);
            continue;
        }
        if line.starts_with('-') {
            removed = removed.saturating_add(1);
        }
    }

    (added, removed)
}

pub fn file_update_change_line_counts(
    change: &crate::protocol::FileUpdateChange,
) -> (usize, usize) {
    match &change.kind {
        crate::protocol::PatchChangeKind::Add => (content_line_count(change.diff.as_str()), 0),
        crate::protocol::PatchChangeKind::Delete => (0, content_line_count(change.diff.as_str())),
        crate::protocol::PatchChangeKind::Update { .. } => {
            unified_diff_line_counts(change.diff.as_str())
        }
    }
}

fn content_line_count(content: &str) -> usize {
    content.lines().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_line_counts_ignore_file_headers() {
        let diff = "\
--- a/docs/file.md
+++ b/docs/file.md
@@ -1 +1,2 @@
-old
+new
+extra";

        assert_eq!(unified_diff_line_counts(diff), (2, 1));
    }

    #[test]
    fn file_update_change_line_counts_handle_add_delete_and_update() {
        let added =
            serde_json::from_value::<crate::protocol::FileUpdateChange>(serde_json::json!({
                "path": "docs/new.md",
                "kind": { "type": "add" },
                "diff": "first line\nsecond line\n"
            }))
            .expect("add change should deserialize");
        let deleted =
            serde_json::from_value::<crate::protocol::FileUpdateChange>(serde_json::json!({
                "path": "docs/old.md",
                "kind": { "type": "delete" },
                "diff": "gone line\n"
            }))
            .expect("delete change should deserialize");
        let updated =
            serde_json::from_value::<crate::protocol::FileUpdateChange>(serde_json::json!({
                "path": "docs/edit.md",
                "kind": { "type": "update", "movePath": null },
                "diff": "@@ -1 +1,2 @@\n-old\n+new\n+extra"
            }))
            .expect("update change should deserialize");

        assert_eq!(file_update_change_line_counts(&added), (2, 0));
        assert_eq!(file_update_change_line_counts(&deleted), (0, 1));
        assert_eq!(file_update_change_line_counts(&updated), (2, 1));
    }
}
