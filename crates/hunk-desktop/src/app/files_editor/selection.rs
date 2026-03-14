use helix_core::Range;
use helix_core::textobject::{TextObject, textobject_word};

pub(super) fn word_selection_range(text: helix_core::ropey::RopeSlice<'_>, pos: usize) -> Range {
    textobject_word(text, Range::point(pos), TextObject::Inside, 1, false)
}

pub(super) fn line_selection_range(
    text: helix_core::ropey::RopeSlice<'_>,
    current: &Range,
    pos: usize,
    extend: bool,
) -> Range {
    let target_line = text.char_to_line(pos.min(text.len_chars()));
    let target_start = text.line_to_char(target_line);
    let target_end = line_end_char(text, target_line);
    if !extend {
        return Range::new(target_start, target_end);
    }

    let anchor_line = text.char_to_line(current.anchor.min(text.len_chars()));
    let anchor_start = text.line_to_char(anchor_line);
    let anchor_end = line_end_char(text, anchor_line);
    if target_line >= anchor_line {
        Range::new(anchor_start, target_end)
    } else {
        Range::new(anchor_end, target_start)
    }
}

fn line_end_char(text: helix_core::ropey::RopeSlice<'_>, line: usize) -> usize {
    let next_line = (line + 1).min(text.len_lines());
    text.line_to_char(next_line).min(text.len_chars())
}
