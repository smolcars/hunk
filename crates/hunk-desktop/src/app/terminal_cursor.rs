use std::time::Duration;

use hunk_terminal::TerminalCursorShapeSnapshot;

pub(crate) const AI_TERMINAL_CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

pub(crate) fn ai_terminal_cursor_shape_blinks(shape: TerminalCursorShapeSnapshot) -> bool {
    !matches!(shape, TerminalCursorShapeSnapshot::Hidden)
}

pub(crate) fn ai_terminal_cursor_visible_for_paint(
    shape: TerminalCursorShapeSnapshot,
    surface_focused: bool,
    blink_visible: bool,
) -> bool {
    !surface_focused || !ai_terminal_cursor_shape_blinks(shape) || blink_visible
}
