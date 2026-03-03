use super::{DiffDocument, DiffHunk, DiffLine, DiffLineKind};

pub fn parse_patch_document(patch: &str) -> DiffDocument {
    let mut document = DiffDocument::default();
    if patch.trim().is_empty() {
        return document;
    }

    let lines = patch.lines().collect::<Vec<_>>();
    let mut ix = 0_usize;

    while ix < lines.len() {
        let line = lines[ix];

        if line.starts_with("@@") {
            let (old_start, new_start) = parse_hunk_header(line)
                .map_or((None, None), |(old_start, new_start)| {
                    (Some(old_start), Some(new_start))
                });
            ix += 1;

            let mut old_line = old_start;
            let mut new_line = new_start;
            let mut hunk_lines = Vec::new();
            let mut trailing_meta = Vec::new();

            while ix < lines.len() {
                let hunk_line = lines[ix];

                if hunk_line.starts_with("@@") || hunk_line.starts_with("diff --git") {
                    break;
                }

                match hunk_line.chars().next() {
                    Some(' ') => {
                        hunk_lines.push(DiffLine::new(
                            DiffLineKind::Context,
                            old_line,
                            new_line,
                            hunk_line.strip_prefix(' ').unwrap_or(hunk_line),
                        ));
                        old_line = old_line.map(|line| line.saturating_add(1));
                        new_line = new_line.map(|line| line.saturating_add(1));
                    }
                    Some('-') => {
                        hunk_lines.push(DiffLine::new(
                            DiffLineKind::Removed,
                            old_line,
                            None,
                            hunk_line.strip_prefix('-').unwrap_or(hunk_line),
                        ));
                        old_line = old_line.map(|line| line.saturating_add(1));
                    }
                    Some('+') => {
                        hunk_lines.push(DiffLine::new(
                            DiffLineKind::Added,
                            None,
                            new_line,
                            hunk_line.strip_prefix('+').unwrap_or(hunk_line),
                        ));
                        new_line = new_line.map(|line| line.saturating_add(1));
                    }
                    Some('\\') => trailing_meta.push(hunk_line.to_string()),
                    _ => {
                        if is_meta_line(hunk_line) {
                            break;
                        }
                        trailing_meta.push(hunk_line.to_string());
                    }
                }

                ix += 1;
            }

            document.hunks.push(DiffHunk {
                header: line.to_string(),
                old_start,
                new_start,
                lines: hunk_lines,
                trailing_meta,
            });
            continue;
        }

        if document.hunks.is_empty() {
            document.prelude.push(line.to_string());
        } else {
            document.epilogue.push(line.to_string());
        }
        ix += 1;
    }

    document
}

fn is_meta_line(line: &str) -> bool {
    line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
        || line.starts_with("new file mode")
        || line.starts_with("deleted file mode")
        || line.starts_with("rename from")
        || line.starts_with("rename to")
        || line.starts_with("Binary files")
        || line.starts_with("\\ No newline at end of file")
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let left_marker = line.find('-')?;
    let right_marker = line.find('+')?;

    let left_part = line[left_marker + 1..].split_whitespace().next()?;
    let right_part = line[right_marker + 1..].split_whitespace().next()?;

    let left_start = parse_range_start(left_part)?;
    let right_start = parse_range_start(right_part)?;

    Some((left_start, right_start))
}

fn parse_range_start(range: &str) -> Option<u32> {
    range.split(',').next()?.parse::<u32>().ok()
}
