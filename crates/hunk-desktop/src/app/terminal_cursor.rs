use std::time::Duration;

use hunk_terminal::TerminalCursorShapeSnapshot;

pub(crate) const AI_TERMINAL_CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);
pub(crate) const AI_TERMINAL_CURSOR_OUTPUT_QUIET_INTERVAL: Duration = Duration::from_millis(180);

pub(crate) fn ai_terminal_cursor_shape_blinks(shape: TerminalCursorShapeSnapshot) -> bool {
    !matches!(shape, TerminalCursorShapeSnapshot::Hidden)
}

pub(crate) fn ai_terminal_effective_cursor_shape(
    shape: TerminalCursorShapeSnapshot,
    surface_focused: bool,
    alt_screen: bool,
) -> TerminalCursorShapeSnapshot {
    if surface_focused && !alt_screen && ai_terminal_cursor_shape_blinks(shape) {
        TerminalCursorShapeSnapshot::Beam
    } else {
        shape
    }
}

pub(crate) fn ai_terminal_cursor_visible_for_paint(
    shape: TerminalCursorShapeSnapshot,
    surface_focused: bool,
    blink_visible: bool,
    output_suppressed: bool,
) -> bool {
    !output_suppressed
        && (!surface_focused || !ai_terminal_cursor_shape_blinks(shape) || blink_visible)
}
