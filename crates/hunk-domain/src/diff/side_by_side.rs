use super::parser::parse_patch_document;
use super::{DiffCell, DiffCellKind, DiffHunk, DiffLineKind, DiffRowKind, SideBySideRow};

pub fn parse_patch_side_by_side(patch: &str) -> Vec<SideBySideRow> {
    if patch.trim().is_empty() {
        return vec![SideBySideRow::meta(
            DiffRowKind::Empty,
            "No diff for this file.",
        )];
    }

    let mut rows = Vec::new();
    let document = parse_patch_document(patch);

    for hunk in document.hunks {
        append_hunk_rows(&hunk, &mut rows);
    }

    if rows.is_empty() {
        rows.push(SideBySideRow::meta(
            DiffRowKind::Empty,
            "No diff for this file.",
        ));
    }

    rows
}

fn append_hunk_rows(hunk: &DiffHunk, rows: &mut Vec<SideBySideRow>) {
    rows.push(SideBySideRow::meta(
        DiffRowKind::HunkHeader,
        hunk.header.clone(),
    ));

    let mut ix = 0_usize;
    while ix < hunk.lines.len() {
        let line = &hunk.lines[ix];
        match line.kind {
            DiffLineKind::Removed => {
                let removed_start = ix;
                while ix < hunk.lines.len() && hunk.lines[ix].kind == DiffLineKind::Removed {
                    ix += 1;
                }

                let added_start = ix;
                while ix < hunk.lines.len() && hunk.lines[ix].kind == DiffLineKind::Added {
                    ix += 1;
                }

                let removed = &hunk.lines[removed_start..added_start];
                let added = &hunk.lines[added_start..ix];
                let max_len = removed.len().max(added.len());

                for entry_ix in 0..max_len {
                    let left = removed.get(entry_ix).map_or_else(DiffCell::empty, |line| {
                        DiffCell::new(line.old_line, line.text.clone(), DiffCellKind::Removed)
                    });
                    let right = added.get(entry_ix).map_or_else(DiffCell::empty, |line| {
                        DiffCell::new(line.new_line, line.text.clone(), DiffCellKind::Added)
                    });
                    rows.push(SideBySideRow::code(left, right));
                }
            }
            DiffLineKind::Added => {
                rows.push(SideBySideRow::code(
                    DiffCell::empty(),
                    DiffCell::new(line.new_line, line.text.clone(), DiffCellKind::Added),
                ));
                ix += 1;
            }
            DiffLineKind::Context => {
                rows.push(SideBySideRow::code(
                    DiffCell::new(line.old_line, line.text.clone(), DiffCellKind::Context),
                    DiffCell::new(line.new_line, line.text.clone(), DiffCellKind::Context),
                ));
                ix += 1;
            }
        }
    }

    for meta_line in &hunk.trailing_meta {
        rows.push(SideBySideRow::meta(DiffRowKind::Meta, meta_line.clone()));
    }
}
