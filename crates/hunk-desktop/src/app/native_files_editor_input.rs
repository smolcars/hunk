use std::cmp::min;

use gpui::{Keystroke, Pixels, Point};
use hunk_editor::{EditorCommand, Viewport};
use hunk_text::{Selection, TextPosition, TextSnapshot};

use super::paint::{
    current_line_text, last_position, raw_column_for_display, uses_primary_shortcut,
};
use super::{
    EditorLayout, FilesEditor, FilesEditorSecondaryClickTarget, PointerSelectionMode,
    PointerSelectionState, ScrollDirection,
};

impl FilesEditor {
    pub(crate) fn apply_motion_action(&mut self, apply: impl FnOnce(&mut Self) -> bool) -> bool {
        if self.active_path().is_none() {
            return false;
        }
        apply(self)
    }

    pub(crate) fn handle_keystroke(&mut self, keystroke: &Keystroke) -> bool {
        if self.active_path().is_none() {
            return false;
        }

        if self.handle_shortcut(keystroke) || self.handle_navigation_shortcuts(keystroke) {
            return true;
        }

        match keystroke.key.as_str() {
            "left" => self.move_horizontally(false, keystroke.modifiers.shift),
            "right" => self.move_horizontally(true, keystroke.modifiers.shift),
            "up" => self.move_vertically(false, keystroke.modifiers.shift),
            "down" => self.move_vertically(true, keystroke.modifiers.shift),
            "home" => self.move_to_line_boundary(true, keystroke.modifiers.shift),
            "end" => self.move_to_line_boundary(false, keystroke.modifiers.shift),
            "pageup" => {
                self.page_scroll(ScrollDirection::Backward);
                true
            }
            "pagedown" => {
                self.page_scroll(ScrollDirection::Forward);
                true
            }
            "backspace" => {
                self.apply_editor_command(EditorCommand::DeleteBackward)
                    .document_changed
            }
            "delete" => {
                self.apply_editor_command(EditorCommand::DeleteForward)
                    .document_changed
            }
            "escape" => self.collapse_selection_to_head(),
            "enter" => self.insert_newline_with_indent(),
            "tab" if !keystroke.modifiers.control && !keystroke.modifiers.platform => {
                self.insert_text("    ")
            }
            _ => self.insert_key_char(keystroke),
        }
    }

    pub(crate) fn scroll_lines(&mut self, line_count: usize, direction: ScrollDirection) {
        let snapshot = self.editor.display_snapshot();
        let max_first_row = snapshot
            .total_display_rows
            .saturating_sub(snapshot.viewport.visible_row_count);
        let next_first_row = match direction {
            ScrollDirection::Backward => snapshot
                .viewport
                .first_visible_row
                .saturating_sub(line_count),
            ScrollDirection::Forward => min(
                snapshot
                    .viewport
                    .first_visible_row
                    .saturating_add(line_count),
                max_first_row,
            ),
        };
        self.editor.apply(EditorCommand::SetViewport(Viewport {
            first_visible_row: next_first_row,
            visible_row_count: snapshot.viewport.visible_row_count,
            horizontal_offset: 0,
        }));
    }

    fn page_scroll(&mut self, direction: ScrollDirection) {
        let snapshot = self.editor.display_snapshot();
        let page = snapshot.viewport.visible_row_count.max(1);
        self.scroll_lines(page, direction);
    }

    fn handle_shortcut(&mut self, keystroke: &Keystroke) -> bool {
        if !uses_primary_shortcut(keystroke) {
            return false;
        }

        match keystroke.key.as_str() {
            "a" if !keystroke.modifiers.shift => self.select_all(),
            "z" if !keystroke.modifiers.shift => {
                self.editor.apply(EditorCommand::Undo).document_changed
            }
            "z" if keystroke.modifiers.shift => {
                self.editor.apply(EditorCommand::Redo).document_changed
            }
            "y" if !cfg!(target_os = "macos") => {
                self.editor.apply(EditorCommand::Redo).document_changed
            }
            _ => false,
        }
    }

    fn handle_navigation_shortcuts(&mut self, keystroke: &Keystroke) -> bool {
        if cfg!(target_os = "macos") {
            if keystroke.modifiers.platform {
                return match keystroke.key.as_str() {
                    "left" => self.move_to_line_boundary_action(true, keystroke.modifiers.shift),
                    "right" => self.move_to_line_boundary_action(false, keystroke.modifiers.shift),
                    _ => false,
                };
            }

            if keystroke.modifiers.alt {
                return match keystroke.key.as_str() {
                    "left" => self.move_word_action(false, keystroke.modifiers.shift),
                    "right" => self.move_word_action(true, keystroke.modifiers.shift),
                    _ => false,
                };
            }

            return false;
        }

        if !uses_primary_shortcut(keystroke) {
            return false;
        }

        match keystroke.key.as_str() {
            "left" => self.move_word_action(false, keystroke.modifiers.shift),
            "right" => self.move_word_action(true, keystroke.modifiers.shift),
            _ => false,
        }
    }

    pub(crate) fn move_horizontal_action(&mut self, forward: bool, extend: bool) -> bool {
        self.move_horizontally(forward, extend)
    }

    fn move_horizontally(&mut self, forward: bool, extend: bool) -> bool {
        let selection = self.editor.selection();
        if !extend && !selection.is_caret() {
            let target = if forward {
                selection.range().end
            } else {
                selection.range().start
            };
            return self
                .editor
                .apply(EditorCommand::SetSelection(Selection::caret(target)))
                .selection_changed;
        }

        let anchor = selection.anchor;
        let output = if forward {
            self.editor.apply(EditorCommand::MoveRight)
        } else {
            self.editor.apply(EditorCommand::MoveLeft)
        };
        if !extend || !output.selection_changed {
            return output.selection_changed;
        }

        let head = self.editor.selection().head;
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(anchor, head)))
            .selection_changed
    }

    pub(crate) fn move_vertical_action(&mut self, forward: bool, extend: bool) -> bool {
        self.move_vertically(forward, extend)
    }

    pub(crate) fn move_to_line_boundary_action(&mut self, start: bool, extend: bool) -> bool {
        self.move_to_line_boundary(start, extend)
    }

    pub(crate) fn move_to_document_boundary_action(&mut self, start: bool, extend: bool) -> bool {
        let snapshot = self.editor.buffer().snapshot();
        let Some(end_position) = last_position(&snapshot) else {
            return false;
        };
        let target = if start {
            TextPosition::default()
        } else {
            end_position
        };
        let selection = self.editor.selection();
        let next_selection = if extend {
            Selection::new(selection.anchor, target)
        } else {
            Selection::caret(target)
        };
        self.editor
            .apply(EditorCommand::SetSelection(next_selection))
            .selection_changed
    }

    pub(crate) fn move_word_action(&mut self, forward: bool, extend: bool) -> bool {
        let selection = self.editor.selection();
        if !extend && !selection.is_caret() {
            let target = if forward {
                selection.range().end
            } else {
                selection.range().start
            };
            return self
                .editor
                .apply(EditorCommand::SetSelection(Selection::caret(target)))
                .selection_changed;
        }

        let snapshot = self.editor.buffer().snapshot();
        let target = if forward {
            next_word_end(&snapshot, selection.head)
        } else {
            previous_word_start(&snapshot, selection.head)
        };
        let next_selection = if extend {
            Selection::new(selection.anchor, target)
        } else {
            Selection::caret(target)
        };
        self.editor
            .apply(EditorCommand::SetSelection(next_selection))
            .selection_changed
    }

    pub(crate) fn page_scroll_action(&mut self, direction: ScrollDirection) -> bool {
        self.page_scroll(direction);
        true
    }

    fn move_vertically(&mut self, forward: bool, extend: bool) -> bool {
        let selection = self.editor.selection();
        if !extend && !selection.is_caret() {
            let target = if forward {
                selection.range().end
            } else {
                selection.range().start
            };
            return self
                .editor
                .apply(EditorCommand::SetSelection(Selection::caret(target)))
                .selection_changed;
        }

        let anchor = selection.anchor;
        let output = if forward {
            self.editor.apply(EditorCommand::MoveDown)
        } else {
            self.editor.apply(EditorCommand::MoveUp)
        };
        if !extend || !output.selection_changed {
            return output.selection_changed;
        }

        let head = self.editor.selection().head;
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(anchor, head)))
            .selection_changed
    }

    fn move_to_line_boundary(&mut self, start: bool, extend: bool) -> bool {
        let selection = self.editor.selection();
        let snapshot = self.editor.buffer().snapshot();
        let line_text = current_line_text(&snapshot, selection.head.line);
        let column = if start { 0 } else { line_text.chars().count() };
        let target = TextPosition::new(selection.head.line, column);
        let next_selection = if extend {
            Selection::new(selection.anchor, target)
        } else {
            Selection::caret(target)
        };
        self.editor
            .apply(EditorCommand::SetSelection(next_selection))
            .selection_changed
    }

    fn collapse_selection_to_head(&mut self) -> bool {
        let head = self.editor.selection().head;
        self.editor
            .apply(EditorCommand::SetSelection(Selection::caret(head)))
            .selection_changed
    }

    fn select_all(&mut self) -> bool {
        let snapshot = self.editor.buffer().snapshot();
        let Some(end_position) = last_position(&snapshot) else {
            return false;
        };
        self.editor
            .apply(EditorCommand::SetSelection(Selection::new(
                TextPosition::default(),
                end_position,
            )))
            .selection_changed
    }

    pub(crate) fn select_all_action(&mut self) -> bool {
        self.select_all()
    }

    fn insert_key_char(&mut self, keystroke: &Keystroke) -> bool {
        if keystroke.modifiers.control || keystroke.modifiers.platform {
            return false;
        }

        let Some(text) = keystroke.key_char.as_deref() else {
            return false;
        };
        if text.is_empty() || matches!(keystroke.key.as_str(), "enter" | "tab") {
            return false;
        }

        self.insert_text(text)
    }

    fn insert_newline_with_indent(&mut self) -> bool {
        let selection = self.editor.selection();
        let snapshot = self.editor.buffer().snapshot();
        let line_text = current_line_text(&snapshot, selection.head.line);
        let indent: String = line_text
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();
        self.insert_text(format!("\n{indent}").as_str())
    }

    fn insert_text(&mut self, text: &str) -> bool {
        self.apply_editor_command(EditorCommand::InsertText(text.to_string()))
            .document_changed
    }

    pub(crate) fn handle_mouse_down(
        &mut self,
        position: Point<Pixels>,
        layout: &EditorLayout,
        shift_held: bool,
        click_count: usize,
    ) -> bool {
        if self.handle_fold_toggle_click(position, layout) {
            return true;
        }
        let Some(next_position) = self.position_for_point(position, layout) else {
            return false;
        };
        self.begin_pointer_selection(next_position, shift_held, click_count)
    }

    fn begin_pointer_selection(
        &mut self,
        position: TextPosition,
        shift_held: bool,
        click_count: usize,
    ) -> bool {
        let selection = if shift_held {
            let anchor = self
                .pointer_selection
                .map(|state| state.anchor)
                .unwrap_or(self.editor.selection().anchor);
            self.pointer_selection = Some(PointerSelectionState {
                anchor,
                mode: PointerSelectionMode::Character,
            });
            Selection::new(anchor, position)
        } else if click_count >= 3 {
            let selection = self.line_selection(position);
            self.pointer_selection = Some(PointerSelectionState {
                anchor: position,
                mode: PointerSelectionMode::Line,
            });
            selection
        } else if click_count == 2 {
            let selection = self.word_selection(position);
            self.pointer_selection = Some(PointerSelectionState {
                anchor: position,
                mode: PointerSelectionMode::Word,
            });
            selection
        } else {
            self.pointer_selection = Some(PointerSelectionState {
                anchor: position,
                mode: PointerSelectionMode::Character,
            });
            Selection::caret(position)
        };

        self.editor
            .apply(EditorCommand::SetSelection(selection))
            .selection_changed
    }

    pub(crate) fn handle_mouse_drag(
        &mut self,
        position: Point<Pixels>,
        layout: &EditorLayout,
    ) -> bool {
        let Some(pointer_selection) = self.pointer_selection else {
            return false;
        };
        let Some(next_position) = self.position_for_point(position, layout) else {
            return false;
        };
        let snapshot = self.editor.buffer().snapshot();
        let selection = selection_for_pointer_drag(
            &snapshot,
            pointer_selection.anchor,
            next_position,
            pointer_selection.mode,
        );
        self.editor
            .apply(EditorCommand::SetSelection(selection))
            .selection_changed
    }

    pub(crate) fn handle_mouse_up(&mut self) -> bool {
        self.pointer_selection.take().is_some()
    }

    pub(crate) fn prepare_context_menu_target(
        &mut self,
        position: Point<Pixels>,
        layout: &EditorLayout,
    ) -> Option<FilesEditorSecondaryClickTarget> {
        let target = self.position_for_point(position, layout)?;
        if !self.selection_contains_position(target) {
            self.pointer_selection = None;
            self.editor
                .apply(EditorCommand::SetSelection(Selection::caret(target)));
        }
        Some(FilesEditorSecondaryClickTarget {
            can_cut: self.active_path().is_some() && self.has_selection(),
            can_copy: self.has_selection(),
            can_paste: self.active_path().is_some(),
            can_select_all: self.active_path().is_some(),
        })
    }

    fn position_for_point(
        &self,
        position: Point<Pixels>,
        layout: &EditorLayout,
    ) -> Option<TextPosition> {
        if !layout.hitbox.bounds.contains(&position) {
            return None;
        }
        let row = ((position.y - layout.hitbox.bounds.origin.y) / layout.line_height)
            .floor()
            .max(0.0) as usize;
        let display_row = layout.display_snapshot.visible_rows.get(row)?;
        let display_column = if position.x <= layout.content_origin_x() {
            0
        } else {
            ((position.x - layout.content_origin_x()) / layout.cell_width)
                .floor()
                .max(0.0) as usize
        };
        let raw_column = raw_column_for_display(display_row, display_column);
        Some(TextPosition::new(display_row.source_line, raw_column))
    }

    fn line_selection(&self, position: TextPosition) -> Selection {
        let snapshot = self.editor.buffer().snapshot();
        line_selection_for_position(&snapshot, position)
    }

    fn word_selection(&self, position: TextPosition) -> Selection {
        let snapshot = self.editor.buffer().snapshot();
        word_selection_for_position(&snapshot, position)
    }

    fn has_selection(&self) -> bool {
        !self.editor.selection().range().is_empty()
    }

    fn selection_contains_position(&self, position: TextPosition) -> bool {
        let selection = self.editor.selection();
        if selection.is_caret() {
            return false;
        }
        let range = selection.range();
        range.start <= position && position < range.end
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn begin_pointer_selection_for_test(
        &mut self,
        position: TextPosition,
        shift_held: bool,
        click_count: usize,
    ) -> bool {
        self.begin_pointer_selection(position, shift_held, click_count)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn drag_pointer_selection_for_test(&mut self, position: TextPosition) -> bool {
        let Some(pointer_selection) = self.pointer_selection else {
            return false;
        };
        let snapshot = self.editor.buffer().snapshot();
        let selection = selection_for_pointer_drag(
            &snapshot,
            pointer_selection.anchor,
            position,
            pointer_selection.mode,
        );
        self.editor
            .apply(EditorCommand::SetSelection(selection))
            .selection_changed
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenClass {
    Identifier,
    Whitespace,
    Punctuation,
}

fn word_selection_for_position(snapshot: &TextSnapshot, position: TextPosition) -> Selection {
    let line_text = current_line_text(snapshot, position.line);
    let characters: Vec<char> = line_text.chars().collect();
    if characters.is_empty() {
        return Selection::caret(TextPosition::new(position.line, 0));
    }

    let mut index = position.column.min(characters.len().saturating_sub(1));
    if position.column == characters.len() && !characters.is_empty() {
        index = characters.len() - 1;
    }

    let class = classify_character(characters[index]);
    let mut start = index;
    while start > 0 && classify_character(characters[start - 1]) == class {
        start -= 1;
    }

    let mut end = index + 1;
    while end < characters.len() && classify_character(characters[end]) == class {
        end += 1;
    }

    Selection::new(
        TextPosition::new(position.line, start),
        TextPosition::new(position.line, end),
    )
}

fn classify_character(character: char) -> TokenClass {
    if character.is_whitespace() {
        TokenClass::Whitespace
    } else if character.is_alphanumeric() || matches!(character, '_' | '$') {
        TokenClass::Identifier
    } else {
        TokenClass::Punctuation
    }
}

fn selection_for_pointer_drag(
    snapshot: &TextSnapshot,
    anchor: TextPosition,
    head: TextPosition,
    mode: PointerSelectionMode,
) -> Selection {
    match mode {
        PointerSelectionMode::Character => Selection::new(anchor, head),
        PointerSelectionMode::Word => word_drag_selection(snapshot, anchor, head),
        PointerSelectionMode::Line => line_drag_selection(snapshot, anchor, head),
    }
}

fn word_drag_selection(
    snapshot: &TextSnapshot,
    anchor: TextPosition,
    head: TextPosition,
) -> Selection {
    let anchor_word = word_selection_for_position(snapshot, anchor);
    let head_word = word_selection_for_position(snapshot, head);
    if head >= anchor {
        Selection::new(anchor_word.range().start, head_word.range().end)
    } else {
        Selection::new(anchor_word.range().end, head_word.range().start)
    }
}

fn line_drag_selection(
    snapshot: &TextSnapshot,
    anchor: TextPosition,
    head: TextPosition,
) -> Selection {
    let anchor_line = line_selection_for_position(snapshot, anchor);
    let head_line = line_selection_for_position(snapshot, head);
    if head >= anchor {
        Selection::new(anchor_line.range().start, head_line.range().end)
    } else {
        Selection::new(anchor_line.range().end, head_line.range().start)
    }
}

fn line_selection_for_position(snapshot: &TextSnapshot, position: TextPosition) -> Selection {
    let line_length = current_line_text(snapshot, position.line).chars().count();
    Selection::new(
        TextPosition::new(position.line, 0),
        TextPosition::new(position.line, line_length),
    )
}

fn previous_word_start(snapshot: &TextSnapshot, position: TextPosition) -> TextPosition {
    let mut cursor = position;
    while let Some(previous) = previous_position(snapshot, cursor) {
        let class = character_class(snapshot, previous);
        cursor = previous;
        if class == TokenClass::Whitespace {
            continue;
        }

        while let Some(candidate) = previous_position(snapshot, cursor) {
            if character_class(snapshot, candidate) != class {
                break;
            }
            cursor = candidate;
        }
        return cursor;
    }
    TextPosition::default()
}

fn next_word_end(snapshot: &TextSnapshot, position: TextPosition) -> TextPosition {
    let mut cursor = position;
    loop {
        let Some(class) = class_at_cursor(snapshot, cursor) else {
            let Some(next) = next_position(snapshot, cursor) else {
                return last_position(snapshot).unwrap_or_default();
            };
            cursor = next;
            continue;
        };

        if class == TokenClass::Whitespace {
            if let Some(next) = next_position(snapshot, cursor) {
                cursor = next;
                continue;
            }
            return last_position(snapshot).unwrap_or_default();
        }

        let mut end = cursor;
        while let Some(next) = next_position(snapshot, end) {
            if character_class(snapshot, next) != class {
                return next;
            }
            end = next;
        }
        return last_position(snapshot).unwrap_or_default();
    }
}

fn class_at_cursor(snapshot: &TextSnapshot, position: TextPosition) -> Option<TokenClass> {
    character_at(snapshot, position).map(classify_character)
}

fn character_class(snapshot: &TextSnapshot, position: TextPosition) -> TokenClass {
    character_at(snapshot, position)
        .map(classify_character)
        .unwrap_or(TokenClass::Whitespace)
}

fn character_at(snapshot: &TextSnapshot, position: TextPosition) -> Option<char> {
    current_line_text(snapshot, position.line)
        .chars()
        .nth(position.column)
}

fn previous_position(snapshot: &TextSnapshot, position: TextPosition) -> Option<TextPosition> {
    if position.column > 0 {
        return Some(TextPosition::new(position.line, position.column - 1));
    }

    if position.line == 0 {
        return None;
    }

    let previous_line = position.line - 1;
    let previous_line_len = current_line_text(snapshot, previous_line).chars().count();
    Some(TextPosition::new(previous_line, previous_line_len))
}

fn next_position(snapshot: &TextSnapshot, position: TextPosition) -> Option<TextPosition> {
    let line_len = current_line_text(snapshot, position.line).chars().count();
    if position.column < line_len {
        return Some(TextPosition::new(position.line, position.column + 1));
    }

    if position.line + 1 >= snapshot.line_count() {
        return None;
    }

    Some(TextPosition::new(position.line + 1, 0))
}
