use std::cmp::min;
use std::ops::Range;

use hunk_text::{SearchMatch, TextSnapshot};

use crate::{
    DisplayRowKind, FoldRegion, OverlayDescriptor, SearchHighlight, WhitespaceKind,
    WhitespaceMarker,
};

#[derive(Debug, Clone)]
pub(crate) struct ExpandedLine {
    pub(crate) display_text: String,
    raw_to_display: Vec<usize>,
    display_to_raw: Vec<usize>,
    markers: Vec<WhitespaceMarker>,
}

impl ExpandedLine {
    pub(crate) fn from_line(line: String, tab_width: usize, show_whitespace: bool) -> Self {
        let mut display_text = String::new();
        let mut raw_to_display = Vec::new();
        let mut display_to_raw = Vec::new();
        let mut markers = Vec::new();

        for (raw_column, ch) in line.chars().enumerate() {
            raw_to_display.push(display_text.chars().count());
            match ch {
                '\t' => {
                    if show_whitespace {
                        markers.push(WhitespaceMarker {
                            column: display_text.chars().count(),
                            kind: WhitespaceKind::Tab,
                        });
                    }
                    let tab_stop = tab_width - (display_text.chars().count() % tab_width);
                    for _ in 0..tab_stop {
                        display_text.push(' ');
                        display_to_raw.push(raw_column);
                    }
                }
                ' ' => {
                    if show_whitespace {
                        markers.push(WhitespaceMarker {
                            column: display_text.chars().count(),
                            kind: WhitespaceKind::Space,
                        });
                    }
                    display_text.push(' ');
                    display_to_raw.push(raw_column);
                }
                other => {
                    display_text.push(other);
                    display_to_raw.push(raw_column);
                }
            }
        }

        raw_to_display.push(display_text.chars().count());
        display_to_raw.push(line.chars().count());

        Self {
            display_text,
            raw_to_display,
            display_to_raw,
            markers,
        }
    }

    pub(crate) fn display_len(&self) -> usize {
        self.display_text.chars().count()
    }

    pub(crate) fn raw_len(&self) -> usize {
        self.raw_to_display.len().saturating_sub(1)
    }

    pub(crate) fn segment(&self, start: usize, end: usize) -> String {
        self.display_text
            .chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    pub(crate) fn raw_to_display_column(&self, raw_column: usize) -> usize {
        let index = min(raw_column, self.raw_to_display.len().saturating_sub(1));
        self.raw_to_display[index]
    }

    pub(crate) fn display_to_raw_column(&self, display_column: usize) -> usize {
        let index = min(display_column, self.display_to_raw.len().saturating_sub(1));
        self.display_to_raw[index]
    }

    pub(crate) fn raw_offsets_in_range(&self, start: usize, end: usize) -> Vec<usize> {
        let raw_start = self.display_to_raw_column(start);
        let raw_end = self.display_to_raw_column(end);
        (raw_start..=raw_end)
            .map(|raw_column| self.raw_to_display_column(raw_column).saturating_sub(start))
            .collect()
    }

    pub(crate) fn markers_in_range(&self, start: usize, end: usize) -> Vec<WhitespaceMarker> {
        self.markers
            .iter()
            .copied()
            .filter(|marker| marker.column >= start && marker.column < end)
            .map(|marker| WhitespaceMarker {
                column: marker.column - start,
                kind: marker.kind,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VisualRow {
    pub(crate) row_index: usize,
    pub(crate) kind: DisplayRowKind,
    pub(crate) source_line: usize,
    pub(crate) raw_start_column: usize,
    pub(crate) raw_end_column: usize,
    pub(crate) raw_column_offsets: Vec<usize>,
    pub(crate) start_column: usize,
    pub(crate) end_column: usize,
    pub(crate) text: String,
    pub(crate) is_wrapped: bool,
    pub(crate) whitespace_markers: Vec<WhitespaceMarker>,
    pub(crate) search_highlights: Vec<SearchHighlight>,
    pub(crate) overlays: Vec<OverlayDescriptor>,
    pub(crate) expanded_line: ExpandedLine,
}

impl VisualRow {
    pub(crate) fn empty() -> Self {
        Self {
            row_index: 0,
            kind: DisplayRowKind::Text,
            source_line: 0,
            raw_start_column: 0,
            raw_end_column: 0,
            raw_column_offsets: vec![0],
            start_column: 0,
            end_column: 0,
            text: String::new(),
            is_wrapped: false,
            whitespace_markers: Vec::new(),
            search_highlights: Vec::new(),
            overlays: Vec::new(),
            expanded_line: ExpandedLine::from_line(String::new(), 4, false),
        }
    }
}

pub(crate) fn build_fold_placeholder(prefix: &str, region: FoldRegion) -> String {
    let hidden_line_count = region.end_line - region.start_line;
    if prefix.is_empty() {
        format!("… {} hidden lines", hidden_line_count)
    } else {
        format!("{prefix}  … {} hidden lines", hidden_line_count)
    }
}

pub(crate) fn overlays_for_line(
    overlays: &[OverlayDescriptor],
    line: usize,
) -> Vec<OverlayDescriptor> {
    overlays
        .iter()
        .filter(|overlay| overlay.line == line)
        .cloned()
        .collect()
}

pub(crate) fn search_matches_for_line(
    snapshot: &TextSnapshot,
    matches: &[SearchMatch],
    line: usize,
) -> Vec<Range<usize>> {
    let line_start = snapshot.line_to_byte(line).unwrap_or(0);
    let line_end = if line + 1 < snapshot.line_count() {
        snapshot
            .line_to_byte(line + 1)
            .unwrap_or(snapshot.byte_len())
    } else {
        snapshot.byte_len()
    };

    matches
        .iter()
        .filter_map(|found| {
            let start = found.byte_range.start.max(line_start);
            let end = found.byte_range.end.min(line_end);
            (start < end).then_some(start..end)
        })
        .collect()
}

pub(crate) fn project_search_matches(
    expanded_line: &ExpandedLine,
    matches: &[Range<usize>],
    start_column: usize,
    end_column: usize,
) -> Vec<SearchHighlight> {
    matches
        .iter()
        .filter_map(|range| {
            let start = expanded_line.raw_to_display_column(range.start);
            let end = expanded_line.raw_to_display_column(range.end);
            let projected_start = start.max(start_column);
            let projected_end = end.min(end_column);
            (projected_start < projected_end).then_some(SearchHighlight {
                start_column: projected_start - start_column,
                end_column: projected_end - start_column,
            })
        })
        .collect()
}

pub(crate) fn line_text(snapshot: &TextSnapshot, line: usize) -> String {
    let start = snapshot.line_to_byte(line).unwrap_or(0);
    let end = if line + 1 < snapshot.line_count() {
        snapshot
            .line_to_byte(line + 1)
            .unwrap_or(snapshot.byte_len())
    } else {
        snapshot.byte_len()
    };
    snapshot
        .slice(start..end)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}
