mod backend;
mod input;
mod runtime;
mod snapshot;

pub use input::{
    TerminalGridPoint, TerminalInputModifiers, TerminalKeystroke, TerminalMouseButton,
    terminal_alt_scroll_input_bytes, terminal_focus_input_bytes, terminal_keystroke_input_bytes,
    terminal_mouse_button_input_bytes, terminal_mouse_move_input_bytes,
    terminal_mouse_scroll_input_bytes, terminal_paste_input_bytes,
};
pub use runtime::{
    TerminalEvent, TerminalSessionHandle, TerminalSpawnRequest, spawn_terminal_session,
};
pub use snapshot::{
    TerminalCellSnapshot, TerminalColorSnapshot, TerminalCursorShapeSnapshot,
    TerminalCursorSnapshot, TerminalDamageLineSnapshot, TerminalDamageSnapshot,
    TerminalModeSnapshot, TerminalNamedColorSnapshot, TerminalScreenSnapshot, TerminalScroll,
};
