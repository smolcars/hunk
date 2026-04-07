#[path = "../src/app/ai_workspace_inline_diff.rs"]
mod ai_workspace_inline_diff;

use ai_workspace_inline_diff::{
    AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD, AiWorkspaceInlineDiffLineKind,
    AiWorkspaceInlineDiffOptions, ai_workspace_inline_diff_presentation_policy,
    ai_workspace_project_inline_diff,
};

#[test]
fn projection_splits_multi_file_diff_and_preserves_line_kinds() {
    let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@
 keep
-remove me
+replace me
+new line
diff --git a/README.md b/README.md
index 3333333..4444444 100644
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new"#;

    let projection =
        ai_workspace_project_inline_diff(diff, AiWorkspaceInlineDiffOptions::default());

    assert_eq!(projection.files.len(), 2);
    assert_eq!(projection.total_added, 3);
    assert_eq!(projection.total_removed, 2);

    let first = &projection.files[0];
    assert_eq!(first.display_path, "src/lib.rs");
    assert_eq!(first.added, 2);
    assert_eq!(first.removed, 1);
    assert_eq!(first.hunks.len(), 1);
    assert_eq!(first.hunks[0].header, "@@ -1,2 +1,3 @@");
    assert_eq!(first.hunks[0].lines.len(), 4);
    assert_eq!(
        first.hunks[0]
            .lines
            .iter()
            .map(|line| line.kind)
            .collect::<Vec<_>>(),
        vec![
            AiWorkspaceInlineDiffLineKind::Context,
            AiWorkspaceInlineDiffLineKind::Removed,
            AiWorkspaceInlineDiffLineKind::Added,
            AiWorkspaceInlineDiffLineKind::Added,
        ]
    );

    let second = &projection.files[1];
    assert_eq!(second.display_path, "README.md");
    assert_eq!(second.added, 1);
    assert_eq!(second.removed, 1);
}

#[test]
fn projection_keeps_rename_metadata_when_no_hunks_exist() {
    let diff = r#"diff --git a/old-name.txt b/new-name.txt
similarity index 100%
rename from old-name.txt
rename to new-name.txt"#;

    let projection =
        ai_workspace_project_inline_diff(diff, AiWorkspaceInlineDiffOptions::default());

    assert_eq!(projection.files.len(), 1);
    let file = &projection.files[0];
    assert_eq!(file.display_path, "new-name.txt");
    assert_eq!(file.old_path.as_deref(), Some("a/old-name.txt"));
    assert_eq!(file.new_path.as_deref(), Some("b/new-name.txt"));
    assert_eq!(
        file.prelude_meta,
        vec![
            "similarity index 100%".to_string(),
            "rename from old-name.txt".to_string(),
            "rename to new-name.txt".to_string(),
        ]
    );
    assert!(file.hunks.is_empty());
}

#[test]
fn projection_applies_file_hunk_and_line_truncation() {
    let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@
 keep 1
-remove 1
+add 1
 keep 2
@@ -10,2 +10,2 @@
-remove 2
+add 2
diff --git a/src/main.rs b/src/main.rs
index 3333333..4444444 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-old
+new"#;

    let projection = ai_workspace_project_inline_diff(
        diff,
        AiWorkspaceInlineDiffOptions {
            max_files: 1,
            max_hunks_per_file: 1,
            max_lines_per_hunk: 2,
        },
    );

    assert_eq!(projection.files.len(), 1);
    assert_eq!(projection.truncated_file_count, 1);
    let file = &projection.files[0];
    assert_eq!(file.display_path, "src/lib.rs");
    assert_eq!(file.truncated_hunk_count, 1);
    assert_eq!(file.hunks.len(), 1);
    assert_eq!(file.hunks[0].lines.len(), 2);
    assert_eq!(file.hunks[0].truncated_line_count, 2);
    assert_eq!(file.added, 2);
    assert_eq!(file.removed, 2);
}

#[test]
fn presentation_policy_keeps_inline_diffs_collapsed_by_default() {
    let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-old
+new"#;

    let options = AiWorkspaceInlineDiffOptions::default();
    let projection = ai_workspace_project_inline_diff(diff, options);
    let policy = ai_workspace_inline_diff_presentation_policy(&projection, options);

    assert!(policy.collapsed_by_default);
    assert!(!policy.recommend_open_in_review);
    assert_eq!(policy.truncation_notice, None);
}

#[test]
fn presentation_policy_recommends_review_when_preview_is_truncated() {
    let diff = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-old
+new
diff --git a/src/main.rs b/src/main.rs
index 3333333..4444444 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-old
+new"#;

    let options = AiWorkspaceInlineDiffOptions {
        max_files: 1,
        max_hunks_per_file: 6,
        max_lines_per_hunk: 80,
    };
    let projection = ai_workspace_project_inline_diff(diff, options);
    let policy = ai_workspace_inline_diff_presentation_policy(&projection, options);

    assert!(policy.recommend_open_in_review);
    assert_eq!(
        policy.truncation_notice.as_deref(),
        Some(
            "Preview truncated in the AI thread. Showing 1 of 2 files. Open in Review for the full diff."
        )
    );
}

#[test]
fn presentation_policy_recommends_review_for_large_non_truncated_diffs() {
    let removed = (0..=AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD)
        .map(|ix| format!("-old {ix}"))
        .collect::<Vec<_>>()
        .join("\n");
    let added = (0..=AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD)
        .map(|ix| format!("+new {ix}"))
        .collect::<Vec<_>>()
        .join("\n");
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{} +1,{} @@\n{}\n{}",
        AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD + 1,
        AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD + 1,
        removed,
        added
    );

    let options = AiWorkspaceInlineDiffOptions {
        max_files: 4,
        max_hunks_per_file: 8,
        max_lines_per_hunk: AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD * 2 + 4,
    };
    let projection = ai_workspace_project_inline_diff(diff.as_str(), options);
    let policy = ai_workspace_inline_diff_presentation_policy(&projection, options);

    assert!(policy.recommend_open_in_review);
    assert_eq!(policy.truncation_notice, None);
}
