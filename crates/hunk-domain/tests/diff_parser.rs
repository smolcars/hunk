use hunk_domain::diff::{
    DiffCellKind, DiffLineKind, DiffRowKind, parse_patch_document, parse_patch_side_by_side,
};

#[test]
fn pairs_multiple_removed_and_added_lines_in_one_block() {
    let patch = "\
diff --git a/file.txt b/file.txt
index 123..456 100644
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
-one
-two
+alpha
+beta
 three";

    let rows = parse_patch_side_by_side(patch);
    let code_rows = rows
        .iter()
        .filter(|row| matches!(row.kind, DiffRowKind::Code))
        .collect::<Vec<_>>();

    assert_eq!(code_rows.len(), 3);

    assert_eq!(code_rows[0].left.kind, DiffCellKind::Removed);
    assert_eq!(code_rows[0].left.text, "one");
    assert_eq!(code_rows[0].right.kind, DiffCellKind::Added);
    assert_eq!(code_rows[0].right.text, "alpha");

    assert_eq!(code_rows[1].left.kind, DiffCellKind::Removed);
    assert_eq!(code_rows[1].left.text, "two");
    assert_eq!(code_rows[1].right.kind, DiffCellKind::Added);
    assert_eq!(code_rows[1].right.text, "beta");

    assert_eq!(code_rows[2].left.kind, DiffCellKind::Context);
    assert_eq!(code_rows[2].right.kind, DiffCellKind::Context);
}

#[test]
fn keeps_unbalanced_change_block_aligned() {
    let patch = "\
@@ -10,3 +10,2 @@
-one
-two
-three
+uno
+dos";

    let rows = parse_patch_side_by_side(patch);
    let code_rows = rows
        .iter()
        .filter(|row| matches!(row.kind, DiffRowKind::Code))
        .collect::<Vec<_>>();

    assert_eq!(code_rows.len(), 3);

    assert_eq!(code_rows[0].left.kind, DiffCellKind::Removed);
    assert_eq!(code_rows[0].right.kind, DiffCellKind::Added);

    assert_eq!(code_rows[1].left.kind, DiffCellKind::Removed);
    assert_eq!(code_rows[1].right.kind, DiffCellKind::Added);

    assert_eq!(code_rows[2].left.kind, DiffCellKind::Removed);
    assert_eq!(code_rows[2].right.kind, DiffCellKind::None);
}

#[test]
fn parses_structured_document_hunks_with_line_numbers() {
    let patch = "\
diff --git a/file.txt b/file.txt
index 123..456 100644
--- a/file.txt
+++ b/file.txt
@@ -10,2 +10,3 @@
-old one
 old two
+new one
+new two
\\ No newline at end of file";

    let document = parse_patch_document(patch);

    assert_eq!(document.prelude.len(), 4);
    assert_eq!(document.hunks.len(), 1);

    let hunk = &document.hunks[0];
    assert_eq!(hunk.header, "@@ -10,2 +10,3 @@");
    assert_eq!(hunk.old_start, Some(10));
    assert_eq!(hunk.new_start, Some(10));
    assert_eq!(hunk.lines.len(), 4);

    assert_eq!(hunk.lines[0].kind, DiffLineKind::Removed);
    assert_eq!(hunk.lines[0].old_line, Some(10));
    assert_eq!(hunk.lines[0].new_line, None);
    assert_eq!(hunk.lines[0].text, "old one");

    assert_eq!(hunk.lines[1].kind, DiffLineKind::Context);
    assert_eq!(hunk.lines[1].old_line, Some(11));
    assert_eq!(hunk.lines[1].new_line, Some(10));

    assert_eq!(hunk.lines[2].kind, DiffLineKind::Added);
    assert_eq!(hunk.lines[2].old_line, None);
    assert_eq!(hunk.lines[2].new_line, Some(11));

    assert_eq!(hunk.trailing_meta, vec!["\\ No newline at end of file"]);
}

#[test]
fn side_by_side_includes_hunk_trailing_meta_rows() {
    let patch = "\
@@ -1,2 +1,2 @@
-old
+new
 keep
\\ No newline at end of file";

    let rows = parse_patch_side_by_side(patch);
    assert!(rows.iter().any(|row| {
        row.kind == DiffRowKind::Meta && row.text == "\\ No newline at end of file"
    }));
}

#[test]
fn keeps_multiple_hunks_as_separate_structures() {
    let patch = "\
@@ -1,2 +1,2 @@
-one
+uno
 two
@@ -10,1 +10,2 @@
 ten
+diez";

    let document = parse_patch_document(patch);
    assert_eq!(document.hunks.len(), 2);
    assert_eq!(document.hunks[0].old_start, Some(1));
    assert_eq!(document.hunks[0].new_start, Some(1));
    assert_eq!(document.hunks[1].old_start, Some(10));
    assert_eq!(document.hunks[1].new_start, Some(10));
}

#[test]
fn handles_empty_patch_for_document_and_rows() {
    let document = parse_patch_document("");
    assert!(document.prelude.is_empty());
    assert!(document.hunks.is_empty());
    assert!(document.epilogue.is_empty());

    let rows = parse_patch_side_by_side("");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, DiffRowKind::Empty);
    assert_eq!(rows[0].text, "No diff for this file.");
}

#[test]
fn side_by_side_hides_patch_metadata_rows() {
    let patch = "\
diff --git a/file.txt b/file.txt
index 123..456 100644
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-old
+new
 keep";

    let rows = parse_patch_side_by_side(patch);

    assert!(rows.iter().all(|row| {
        matches!(
            row.kind,
            DiffRowKind::Code | DiffRowKind::HunkHeader | DiffRowKind::Empty
        )
    }));
    assert!(rows.iter().any(|row| row.kind == DiffRowKind::HunkHeader));
    assert!(rows.iter().all(|row| !row.text.starts_with("diff --git")));
}

#[test]
fn keeps_meta_lines_before_and_after_hunk() {
    let patch = "\
diff --git a/a.rs b/a.rs
index 123..456 100644
--- a/a.rs
+++ b/a.rs
@@ -1,1 +1,1 @@
-old
+new
rename from a.rs
rename to b.rs";

    let document = parse_patch_document(patch);
    assert_eq!(document.prelude.len(), 4);
    assert_eq!(document.hunks.len(), 1);
    assert_eq!(
        document.epilogue,
        vec!["rename from a.rs", "rename to b.rs"]
    );
}

#[test]
fn preserves_leading_diff_marker_characters_in_payload() {
    let patch = "\
@@ -1,3 +1,3 @@
-++left-plus
- --left-space
+--right-minus
+  right-space
 shared";

    let document = parse_patch_document(patch);
    assert_eq!(document.hunks.len(), 1);
    let lines = &document.hunks[0].lines;
    assert_eq!(lines.len(), 5);

    assert_eq!(lines[0].kind, DiffLineKind::Removed);
    assert_eq!(lines[0].text, "++left-plus");

    assert_eq!(lines[1].kind, DiffLineKind::Removed);
    assert_eq!(lines[1].text, " --left-space");

    assert_eq!(lines[2].kind, DiffLineKind::Added);
    assert_eq!(lines[2].text, "--right-minus");

    assert_eq!(lines[3].kind, DiffLineKind::Added);
    assert_eq!(lines[3].text, "  right-space");

    assert_eq!(lines[4].kind, DiffLineKind::Context);
    assert_eq!(lines[4].text, "shared");
}

#[test]
fn keeps_payload_lines_that_look_like_file_header_markers() {
    let patch = "\
@@ -1,3 +1,3 @@
---- removed payload that starts with three dashes
-keep removed
++++ added payload that starts with three pluses
+keep added
 context";

    let document = parse_patch_document(patch);
    assert_eq!(document.hunks.len(), 1);
    let lines = &document.hunks[0].lines;
    assert_eq!(lines.len(), 5);

    assert_eq!(lines[0].kind, DiffLineKind::Removed);
    assert_eq!(
        lines[0].text,
        "--- removed payload that starts with three dashes"
    );

    assert_eq!(lines[1].kind, DiffLineKind::Removed);
    assert_eq!(lines[1].text, "keep removed");

    assert_eq!(lines[2].kind, DiffLineKind::Added);
    assert_eq!(
        lines[2].text,
        "+++ added payload that starts with three pluses"
    );

    assert_eq!(lines[3].kind, DiffLineKind::Added);
    assert_eq!(lines[3].text, "keep added");

    assert_eq!(lines[4].kind, DiffLineKind::Context);
    assert_eq!(lines[4].text, "context");
}

#[test]
fn parses_new_file_patch_with_only_added_lines() {
    let patch = "\
diff --git a/src/new_file.rs b/src/new_file.rs
new file mode 100644
index 0000000..1111111
--- /dev/null
+++ b/src/new_file.rs
@@ -0,0 +1,2 @@
+fn main() {}
+println!(\"hello\");";

    let document = parse_patch_document(patch);
    assert_eq!(document.hunks.len(), 1);
    assert_eq!(document.hunks[0].lines.len(), 2);
    assert!(
        document.hunks[0]
            .lines
            .iter()
            .all(|line| line.kind == DiffLineKind::Added)
    );

    let rows = parse_patch_side_by_side(patch);
    let code_rows = rows
        .iter()
        .filter(|row| row.kind == DiffRowKind::Code)
        .collect::<Vec<_>>();
    assert_eq!(code_rows.len(), 2);
    assert!(
        code_rows
            .iter()
            .all(|row| row.left.kind == DiffCellKind::None)
    );
    assert!(
        code_rows
            .iter()
            .all(|row| row.right.kind == DiffCellKind::Added)
    );
}

#[test]
fn malformed_hunk_header_does_not_fabricate_zero_line_numbers() {
    let patch = "\
@@ -x,y +z,w @@
-old
+new";

    let document = parse_patch_document(patch);
    assert_eq!(document.hunks.len(), 1);
    let hunk = &document.hunks[0];
    assert_eq!(hunk.old_start, None);
    assert_eq!(hunk.new_start, None);
    assert_eq!(hunk.lines.len(), 2);
    assert_eq!(hunk.lines[0].old_line, None);
    assert_eq!(hunk.lines[0].new_line, None);
    assert_eq!(hunk.lines[1].old_line, None);
    assert_eq!(hunk.lines[1].new_line, None);
}
