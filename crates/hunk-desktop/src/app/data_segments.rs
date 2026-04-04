use std::ops::Range;
use std::path::Path;

use gpui::SharedString;

use super::data::CachedStyledSegment;
use super::highlight::SyntaxTokenKind;

pub(super) fn compact_cached_segments_for_render(
    segments: Vec<CachedStyledSegment>,
    max_segments: usize,
) -> Vec<CachedStyledSegment> {
    if max_segments == 0 || segments.len() <= max_segments {
        return segments;
    }

    let chunk_size = segments.len().div_ceil(max_segments);
    let mut compacted = Vec::with_capacity(max_segments);
    for chunk in segments.chunks(chunk_size) {
        if chunk.is_empty() {
            continue;
        }

        let plain_capacity = chunk
            .iter()
            .map(|segment| segment.plain_text.as_ref().len())
            .sum::<usize>();
        let mut plain_text = String::with_capacity(plain_capacity);

        let first_syntax = chunk[0].syntax;
        let mut mixed_syntax = false;
        let mut changed = false;
        let mut search_match = false;
        for segment in chunk {
            plain_text.push_str(segment.plain_text.as_ref());
            changed |= segment.changed;
            search_match |= segment.search_match;
            if segment.syntax != first_syntax {
                mixed_syntax = true;
            }
        }

        compacted.push(CachedStyledSegment {
            plain_text: SharedString::from(plain_text),
            syntax: if mixed_syntax {
                SyntaxTokenKind::Plain
            } else {
                first_syntax
            },
            changed,
            search_match,
        });
    }

    compacted
}

pub(super) fn cached_runtime_fallback_segments(text: &str) -> Vec<CachedStyledSegment> {
    if text.is_empty() {
        return Vec::new();
    }

    vec![CachedStyledSegment {
        plain_text: SharedString::from(text.to_string()),
        syntax: SyntaxTokenKind::Plain,
        changed: false,
        search_match: false,
    }]
}

pub(super) fn apply_search_highlights_to_cached_segments(
    segments: Vec<CachedStyledSegment>,
    highlight_columns: &[Range<usize>],
) -> Vec<CachedStyledSegment> {
    let Some(highlight_columns) = normalize_highlight_columns(highlight_columns) else {
        return segments;
    };

    let mut decorated = Vec::new();
    let mut segment_start_column = 0usize;
    let mut highlight_ix = 0usize;

    for segment in segments {
        let segment_len = segment.plain_text.chars().count();
        let segment_end_column = segment_start_column.saturating_add(segment_len);
        if segment_len == 0 {
            segment_start_column = segment_end_column;
            continue;
        }

        while highlight_ix < highlight_columns.len()
            && highlight_columns[highlight_ix].end <= segment_start_column
        {
            highlight_ix += 1;
        }

        if highlight_ix >= highlight_columns.len()
            || highlight_columns[highlight_ix].start >= segment_end_column
        {
            decorated.push(segment);
            segment_start_column = segment_end_column;
            continue;
        }

        let mut cursor = segment_start_column;
        let mut local_highlight_ix = highlight_ix;
        while cursor < segment_end_column {
            while local_highlight_ix < highlight_columns.len()
                && highlight_columns[local_highlight_ix].end <= cursor
            {
                local_highlight_ix += 1;
            }

            let next_boundary = if let Some(highlight) = highlight_columns.get(local_highlight_ix) {
                if highlight.start <= cursor {
                    segment_end_column.min(highlight.end)
                } else {
                    segment_end_column.min(highlight.start)
                }
            } else {
                segment_end_column
            };

            if next_boundary <= cursor {
                break;
            }

            let search_match = highlight_columns
                .get(local_highlight_ix)
                .is_some_and(|highlight| highlight.start <= cursor && cursor < highlight.end);
            decorated.push(CachedStyledSegment {
                plain_text: SharedString::from(segment_slice(
                    segment.plain_text.as_ref(),
                    cursor.saturating_sub(segment_start_column),
                    next_boundary.saturating_sub(segment_start_column),
                )),
                syntax: segment.syntax,
                changed: segment.changed,
                search_match,
            });
            cursor = next_boundary;
        }

        segment_start_column = segment_end_column;
        highlight_ix = local_highlight_ix;
    }

    decorated
}

pub(super) fn merge_cached_segments_with_changed_flags(
    syntax_segments: Vec<CachedStyledSegment>,
    changed_segments: Option<&Vec<CachedStyledSegment>>,
    text: &str,
) -> Vec<CachedStyledSegment> {
    let Some(changed_segments) = changed_segments else {
        return syntax_segments;
    };
    if syntax_segments.is_empty() {
        return syntax_segments;
    }

    let total_columns = text.chars().count();
    let mut changed_by_column = Vec::with_capacity(total_columns);
    for segment in changed_segments {
        changed_by_column.extend(std::iter::repeat_n(
            segment.changed,
            segment.plain_text.chars().count(),
        ));
    }
    changed_by_column.resize(total_columns, false);

    let mut merged = Vec::new();
    let mut column = 0usize;
    for segment in syntax_segments {
        let column_end = (column + segment.plain_text.chars().count()).min(total_columns);
        if column >= column_end {
            continue;
        }

        let mut run_start = column;
        while run_start < column_end {
            let run_changed = changed_by_column[run_start];
            let mut run_end = run_start + 1;
            while run_end < column_end && changed_by_column[run_end] == run_changed {
                run_end += 1;
            }

            merged.push(CachedStyledSegment {
                plain_text: SharedString::from(segment_slice(
                    segment.plain_text.as_ref(),
                    run_start.saturating_sub(column),
                    run_end.saturating_sub(column),
                )),
                syntax: segment.syntax,
                changed: run_changed,
                search_match: false,
            });
            run_start = run_end;
        }

        column = column_end;
    }

    merged
}

fn normalize_highlight_columns(highlight_columns: &[Range<usize>]) -> Option<Vec<Range<usize>>> {
    let mut sorted = highlight_columns
        .iter()
        .filter(|range| range.start < range.end)
        .cloned()
        .collect::<Vec<_>>();
    if sorted.is_empty() {
        return None;
    }

    sorted.sort_by_key(|range| (range.start, range.end));
    let mut normalized: Vec<Range<usize>> = Vec::with_capacity(sorted.len());
    for range in sorted {
        if let Some(previous) = normalized.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        normalized.push(range);
    }

    Some(normalized)
}

fn segment_slice(text: &str, start_column: usize, end_column: usize) -> String {
    if start_column >= end_column {
        return String::new();
    }

    text.chars()
        .skip(start_column)
        .take(end_column.saturating_sub(start_column))
        .collect()
}

pub(super) fn is_probably_binary_extension(path: &str) -> bool {
    let Some(extension) = Path::new(path).extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    let extension = extension.to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "7z" | "a"
            | "apk"
            | "bin"
            | "bmp"
            | "class"
            | "dll"
            | "dmg"
            | "doc"
            | "docx"
            | "ear"
            | "eot"
            | "exe"
            | "gif"
            | "gz"
            | "ico"
            | "jar"
            | "jpeg"
            | "jpg"
            | "lib"
            | "lockb"
            | "mov"
            | "mp3"
            | "mp4"
            | "o"
            | "obj"
            | "otf"
            | "pdf"
            | "png"
            | "pyc"
            | "so"
            | "tar"
            | "tif"
            | "tiff"
            | "ttf"
            | "war"
            | "wasm"
            | "webm"
            | "webp"
            | "woff"
            | "woff2"
            | "xls"
            | "xlsx"
            | "zip"
    )
}

pub(super) fn is_binary_patch(patch: &str) -> bool {
    patch.contains('\0')
        || patch.contains("\nGIT binary patch\n")
        || patch
            .lines()
            .any(|line| line.starts_with("Binary files ") && line.ends_with(" differ"))
}
