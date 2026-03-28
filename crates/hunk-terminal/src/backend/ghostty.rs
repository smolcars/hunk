use std::sync::Arc;

use libghostty_vt::{
    RenderState, Terminal, TerminalOptions, ffi, focus, key, mouse,
    render::{CellIterator, Colors, CursorVisualStyle, RowIterator, Snapshot},
    style::RgbColor,
    terminal::{Mode, ScrollViewport},
};

use crate::input::{
    TerminalGridPoint, TerminalInputModifiers, TerminalKeyInput, TerminalMouseButton,
    TerminalPointerInput, terminal_key_input_bytes, terminal_paste_input_bytes,
};
use crate::snapshot::{
    TerminalCellSnapshot, TerminalColorSnapshot, TerminalCursorShapeSnapshot,
    TerminalCursorSnapshot, TerminalDamageSnapshot, TerminalModeSnapshot,
    TerminalNamedColorSnapshot, TerminalScreenSnapshot, TerminalScroll,
};

const DEFAULT_MAX_SCROLLBACK: usize = 10_000;
const ALACRITTY_WIDE_CHAR_FLAG: u16 = 0b0000_0000_0010_0000;
const ALACRITTY_WIDE_CHAR_SPACER_FLAG: u16 = 0b0000_0000_0100_0000;
const ALACRITTY_LEADING_WIDE_CHAR_SPACER_FLAG: u16 = 0b0000_0100_0000_0000;

pub(crate) struct GhosttyTerminalVt {
    cols: u16,
    rows: u16,
    terminal: Terminal<'static, 'static>,
    render_state: RenderState<'static>,
    row_iterator: RowIterator<'static>,
    cell_iterator: CellIterator<'static>,
    key_encoder: key::Encoder<'static>,
    mouse_encoder: mouse::Encoder<'static>,
}

impl GhosttyTerminalVt {
    pub(crate) fn new(rows: u16, cols: u16) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);

        Self {
            cols,
            rows,
            terminal: Terminal::new(TerminalOptions {
                cols,
                rows,
                max_scrollback: DEFAULT_MAX_SCROLLBACK,
            })
            .expect("create libghostty-vt terminal"),
            render_state: RenderState::new().expect("create libghostty-vt render state"),
            row_iterator: RowIterator::new().expect("create libghostty-vt row iterator"),
            cell_iterator: CellIterator::new().expect("create libghostty-vt cell iterator"),
            key_encoder: key::Encoder::new().expect("create libghostty-vt key encoder"),
            mouse_encoder: mouse::Encoder::new().expect("create libghostty-vt mouse encoder"),
        }
    }

    pub(crate) fn snapshot(&mut self) -> Arc<TerminalScreenSnapshot> {
        Arc::new(build_snapshot(
            &self.terminal,
            &mut self.render_state,
            &mut self.row_iterator,
            &mut self.cell_iterator,
        ))
    }

    pub(crate) fn advance(&mut self, bytes: &[u8]) -> Arc<TerminalScreenSnapshot> {
        self.terminal.vt_write(bytes);
        self.snapshot()
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) -> Arc<TerminalScreenSnapshot> {
        self.rows = rows.max(1);
        self.cols = cols.max(1);
        self.terminal
            .resize(self.cols, self.rows, 0, 0)
            .expect("resize libghostty-vt terminal");
        self.snapshot()
    }

    pub(crate) fn scroll_display(&mut self, scroll: TerminalScroll) -> Arc<TerminalScreenSnapshot> {
        self.terminal.scroll_viewport(match scroll {
            TerminalScroll::Delta(count) => {
                ScrollViewport::Delta(isize::try_from(count).unwrap_or(0))
            }
            TerminalScroll::PageUp => {
                ScrollViewport::Delta(-isize::try_from(self.rows).unwrap_or(isize::MAX))
            }
            TerminalScroll::PageDown => {
                ScrollViewport::Delta(isize::try_from(self.rows).unwrap_or(isize::MAX))
            }
            TerminalScroll::Top => ScrollViewport::Top,
            TerminalScroll::Bottom => ScrollViewport::Bottom,
        });
        self.snapshot()
    }

    pub(crate) fn focus_input_bytes(&self, focused: bool) -> Option<Vec<u8>> {
        if !self
            .terminal
            .mode(Mode::FOCUS_EVENT)
            .expect("read libghostty-vt focus event mode")
        {
            return None;
        }

        let mut bytes = [0_u8; 8];
        let written = match if focused {
            focus::Event::Gained
        } else {
            focus::Event::Lost
        }
        .encode(&mut bytes)
        {
            Ok(written) => written,
            Err(_) => return None,
        };

        Some(bytes[..written].to_vec())
    }

    pub(crate) fn paste_input_bytes(&self, text: &str) -> Vec<u8> {
        let bracketed = self
            .terminal
            .mode(Mode::BRACKETED_PASTE)
            .expect("read libghostty-vt bracketed paste mode");
        terminal_paste_input_bytes(text, bracketed)
    }

    pub(crate) fn key_input_bytes(&mut self, input: &TerminalKeyInput) -> Option<Vec<u8>> {
        terminal_key_input_bytes(input, &self.terminal, &mut self.key_encoder)
    }

    pub(crate) fn pointer_input_bytes(&mut self, input: TerminalPointerInput) -> Vec<Vec<u8>> {
        match input {
            TerminalPointerInput::Button {
                point,
                button,
                modifiers,
                pressed,
            } => self
                .mouse_report_bytes(
                    point,
                    Some(button),
                    if pressed {
                        mouse::Action::Press
                    } else {
                        mouse::Action::Release
                    },
                    modifiers,
                    false,
                )
                .into_iter()
                .collect(),
            TerminalPointerInput::Move {
                point,
                button,
                modifiers,
            } => self
                .mouse_report_bytes(
                    point,
                    button,
                    mouse::Action::Motion,
                    modifiers,
                    button.is_some(),
                )
                .into_iter()
                .collect(),
            TerminalPointerInput::Scroll {
                point,
                scroll_lines,
                modifiers,
            } => {
                if scroll_lines == 0 {
                    return Vec::new();
                }

                let Some(report) = self.mouse_report_bytes_raw(
                    point,
                    Some(if scroll_lines > 0 {
                        mouse::Button::Four
                    } else {
                        mouse::Button::Five
                    }),
                    mouse::Action::Press,
                    modifiers,
                    false,
                ) else {
                    return Vec::new();
                };

                std::iter::repeat_n(report, scroll_lines.unsigned_abs() as usize).collect()
            }
        }
    }

    fn mouse_report_bytes(
        &mut self,
        point: TerminalGridPoint,
        button: Option<TerminalMouseButton>,
        action: mouse::Action,
        modifiers: TerminalInputModifiers,
        any_button_pressed: bool,
    ) -> Option<Vec<u8>> {
        self.mouse_report_bytes_raw(
            point,
            button.map(terminal_mouse_button),
            action,
            modifiers,
            any_button_pressed,
        )
    }

    fn mouse_report_bytes_raw(
        &mut self,
        point: TerminalGridPoint,
        button: Option<mouse::Button>,
        action: mouse::Action,
        modifiers: TerminalInputModifiers,
        any_button_pressed: bool,
    ) -> Option<Vec<u8>> {
        if point.line < 0 {
            return None;
        }

        self.mouse_encoder
            .set_options_from_terminal(&self.terminal)
            .set_size(mouse::EncoderSize {
                screen_width: u32::from(self.cols),
                screen_height: u32::from(self.rows),
                cell_width: 1,
                cell_height: 1,
                padding_top: 0,
                padding_bottom: 0,
                padding_left: 0,
                padding_right: 0,
            })
            .set_any_button_pressed(any_button_pressed)
            .set_track_last_cell(false);

        let mut event = mouse::Event::new().ok()?;
        event
            .set_action(action)
            .set_button(button)
            .set_mods(terminal_mouse_mods(modifiers))
            .set_position(mouse::Position {
                x: point.column as f32,
                y: point.line as f32,
            });

        let mut bytes = Vec::new();
        self.mouse_encoder.encode_to_vec(&event, &mut bytes).ok()?;
        if bytes.is_empty() { None } else { Some(bytes) }
    }
}

fn terminal_mouse_button(button: TerminalMouseButton) -> mouse::Button {
    match button {
        TerminalMouseButton::Left => mouse::Button::Left,
        TerminalMouseButton::Middle => mouse::Button::Middle,
        TerminalMouseButton::Right => mouse::Button::Right,
    }
}

fn terminal_mouse_mods(modifiers: TerminalInputModifiers) -> key::Mods {
    let mut mods = key::Mods::empty();
    if modifiers.shift {
        mods |= key::Mods::SHIFT;
    }
    if modifiers.alt {
        mods |= key::Mods::ALT;
    }
    if modifiers.control {
        mods |= key::Mods::CTRL;
    }
    mods
}

fn build_snapshot(
    terminal: &Terminal<'static, 'static>,
    render_state: &mut RenderState<'static>,
    row_iterator: &mut RowIterator<'static>,
    cell_iterator: &mut CellIterator<'static>,
) -> TerminalScreenSnapshot {
    let snapshot = render_state
        .update(terminal)
        .expect("update libghostty-vt render state");
    let rows = snapshot.rows().expect("read libghostty-vt rows");
    let cols = snapshot.cols().expect("read libghostty-vt cols");
    let colors = snapshot.colors().expect("read libghostty-vt colors");
    let cursor = snapshot_cursor(terminal, &snapshot);
    let mode = snapshot_mode(terminal, &snapshot);
    let display_offset = snapshot_display_offset(terminal);
    let cells = snapshot_cells(&snapshot, &colors, row_iterator, cell_iterator);

    TerminalScreenSnapshot {
        rows,
        cols,
        display_offset,
        cursor,
        mode,
        damage: TerminalDamageSnapshot::Full,
        cells,
    }
}

fn snapshot_cells(
    snapshot: &Snapshot<'static, '_>,
    colors: &Colors,
    row_iterator: &mut RowIterator<'static>,
    cell_iterator: &mut CellIterator<'static>,
) -> Vec<TerminalCellSnapshot> {
    let mut cells = Vec::new();
    let mut row_index = 0_i32;
    let mut rows = row_iterator
        .update(snapshot)
        .expect("update libghostty-vt row iterator");

    while let Some(row) = rows.next() {
        let mut row_cells = cell_iterator
            .update(row)
            .expect("update libghostty-vt cell iterator");
        let mut column = 0_usize;
        while let Some(cell) = row_cells.next() {
            cells.push(snapshot_cell(row_index, column, cell, colors));
            column += 1;
        }

        row_index += 1;
    }

    cells
}

fn snapshot_cell(
    line: i32,
    column: usize,
    cell: &libghostty_vt::render::CellIteration<'static, '_>,
    colors: &Colors,
) -> TerminalCellSnapshot {
    let wide = cell
        .raw_cell()
        .and_then(|raw_cell| raw_cell.wide())
        .expect("read libghostty-vt cell width");
    let graphemes = cell.graphemes().expect("read libghostty-vt cell graphemes");
    let (character, zerowidth) = snapshot_graphemes(wide, graphemes);

    TerminalCellSnapshot {
        line,
        column,
        character,
        fg: snapshot_color(
            cell.fg_color().expect("read libghostty-vt foreground"),
            colors.foreground,
            TerminalNamedColorSnapshot::Foreground,
            colors,
        ),
        bg: snapshot_color(
            cell.bg_color().expect("read libghostty-vt background"),
            colors.background,
            TerminalNamedColorSnapshot::Background,
            colors,
        ),
        flags: snapshot_flags(wide),
        zerowidth,
    }
}

fn snapshot_graphemes(
    wide: libghostty_vt::screen::CellWide,
    graphemes: Vec<char>,
) -> (char, Vec<char>) {
    if graphemes.is_empty() {
        let character = match wide {
            libghostty_vt::screen::CellWide::SpacerTail
            | libghostty_vt::screen::CellWide::SpacerHead => ' ',
            libghostty_vt::screen::CellWide::Narrow | libghostty_vt::screen::CellWide::Wide => '\0',
        };
        return (character, Vec::new());
    }

    let mut graphemes = graphemes;
    let character = graphemes.remove(0);
    (character, graphemes)
}

fn snapshot_flags(wide: libghostty_vt::screen::CellWide) -> u16 {
    match wide {
        libghostty_vt::screen::CellWide::Narrow => 0,
        libghostty_vt::screen::CellWide::Wide => ALACRITTY_WIDE_CHAR_FLAG,
        libghostty_vt::screen::CellWide::SpacerTail => ALACRITTY_WIDE_CHAR_SPACER_FLAG,
        libghostty_vt::screen::CellWide::SpacerHead => ALACRITTY_LEADING_WIDE_CHAR_SPACER_FLAG,
    }
}

fn snapshot_color(
    color: Option<RgbColor>,
    default_color: RgbColor,
    default_named: TerminalNamedColorSnapshot,
    colors: &Colors,
) -> TerminalColorSnapshot {
    match color {
        None => TerminalColorSnapshot::Named(default_named),
        Some(color) if color == default_color => TerminalColorSnapshot::Named(default_named),
        Some(color) => snapshot_rgb_color(color, colors),
    }
}

fn snapshot_rgb_color(color: RgbColor, colors: &Colors) -> TerminalColorSnapshot {
    if let Some(named) = snapshot_named_palette_color(color, colors) {
        return TerminalColorSnapshot::Named(named);
    }

    if let Some(index) = colors
        .palette
        .iter()
        .position(|candidate| *candidate == color)
    {
        return TerminalColorSnapshot::Indexed(index as u8);
    }

    TerminalColorSnapshot::Rgb {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

fn snapshot_named_palette_color(
    color: RgbColor,
    colors: &Colors,
) -> Option<TerminalNamedColorSnapshot> {
    match colors
        .palette
        .iter()
        .position(|candidate| *candidate == color)
    {
        Some(0) => Some(TerminalNamedColorSnapshot::Black),
        Some(1) => Some(TerminalNamedColorSnapshot::Red),
        Some(2) => Some(TerminalNamedColorSnapshot::Green),
        Some(3) => Some(TerminalNamedColorSnapshot::Yellow),
        Some(4) => Some(TerminalNamedColorSnapshot::Blue),
        Some(5) => Some(TerminalNamedColorSnapshot::Magenta),
        Some(6) => Some(TerminalNamedColorSnapshot::Cyan),
        Some(7) => Some(TerminalNamedColorSnapshot::White),
        Some(8) => Some(TerminalNamedColorSnapshot::BrightBlack),
        Some(9) => Some(TerminalNamedColorSnapshot::BrightRed),
        Some(10) => Some(TerminalNamedColorSnapshot::BrightGreen),
        Some(11) => Some(TerminalNamedColorSnapshot::BrightYellow),
        Some(12) => Some(TerminalNamedColorSnapshot::BrightBlue),
        Some(13) => Some(TerminalNamedColorSnapshot::BrightMagenta),
        Some(14) => Some(TerminalNamedColorSnapshot::BrightCyan),
        Some(15) => Some(TerminalNamedColorSnapshot::BrightWhite),
        _ => None,
    }
}

fn snapshot_cursor(
    terminal: &Terminal<'static, 'static>,
    snapshot: &Snapshot<'static, '_>,
) -> TerminalCursorSnapshot {
    let viewport = snapshot
        .cursor_viewport()
        .expect("read libghostty-vt cursor viewport");
    let line = viewport
        .map(|cursor| i32::from(cursor.y))
        .unwrap_or_else(|| i32::from(terminal.cursor_y().expect("read libghostty-vt cursor row")));
    let column = viewport
        .map(|cursor| usize::from(cursor.x))
        .unwrap_or_else(|| {
            usize::from(
                terminal
                    .cursor_x()
                    .expect("read libghostty-vt cursor column"),
            )
        });

    TerminalCursorSnapshot {
        line,
        column,
        shape: snapshot_cursor_shape(
            snapshot
                .cursor_visual_style()
                .expect("read libghostty-vt cursor style"),
        ),
    }
}

fn snapshot_cursor_shape(shape: CursorVisualStyle) -> TerminalCursorShapeSnapshot {
    match shape {
        CursorVisualStyle::Bar => TerminalCursorShapeSnapshot::Beam,
        CursorVisualStyle::Block => TerminalCursorShapeSnapshot::Block,
        CursorVisualStyle::Underline => TerminalCursorShapeSnapshot::Underline,
        CursorVisualStyle::BlockHollow => TerminalCursorShapeSnapshot::HollowBlock,
        _ => TerminalCursorShapeSnapshot::Block,
    }
}

fn snapshot_mode(
    terminal: &Terminal<'static, 'static>,
    snapshot: &Snapshot<'static, '_>,
) -> TerminalModeSnapshot {
    let cursor_in_viewport = snapshot
        .cursor_viewport()
        .expect("read libghostty-vt cursor viewport")
        .is_some();

    TerminalModeSnapshot {
        alt_screen: terminal
            .active_screen()
            .expect("read libghostty-vt active screen")
            == ffi::GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_ALTERNATE,
        app_cursor: terminal
            .mode(Mode::DECCKM)
            .expect("read libghostty-vt app cursor mode"),
        app_keypad: terminal
            .mode(Mode::KEYPAD_KEYS)
            .expect("read libghostty-vt keypad mode"),
        show_cursor: snapshot
            .cursor_visible()
            .expect("read libghostty-vt cursor visibility")
            && cursor_in_viewport,
        line_wrap: terminal
            .mode(Mode::WRAPAROUND)
            .expect("read libghostty-vt wraparound mode"),
        bracketed_paste: terminal
            .mode(Mode::BRACKETED_PASTE)
            .expect("read libghostty-vt bracketed paste mode"),
        focus_in_out: terminal
            .mode(Mode::FOCUS_EVENT)
            .expect("read libghostty-vt focus event mode"),
        mouse_mode: terminal
            .is_mouse_tracking()
            .expect("read libghostty-vt mouse tracking state"),
        mouse_motion: terminal
            .mode(Mode::ANY_MOUSE)
            .expect("read libghostty-vt any-event mouse mode"),
        mouse_drag: terminal
            .mode(Mode::BUTTON_MOUSE)
            .expect("read libghostty-vt button-event mouse mode"),
        sgr_mouse: terminal
            .mode(Mode::SGR_MOUSE)
            .expect("read libghostty-vt sgr mouse mode"),
        utf8_mouse: terminal
            .mode(Mode::UTF8_MOUSE)
            .expect("read libghostty-vt utf8 mouse mode"),
        alternate_scroll: terminal
            .mode(Mode::ALT_SCROLL)
            .expect("read libghostty-vt alternate scroll mode"),
    }
}

fn snapshot_display_offset(terminal: &Terminal<'static, 'static>) -> usize {
    let scrollbar = terminal
        .scrollbar()
        .expect("read libghostty-vt scrollbar state");
    let visible_end = scrollbar.offset.saturating_add(scrollbar.len);
    let trailing_rows = scrollbar.total.saturating_sub(visible_end);
    usize::try_from(trailing_rows).unwrap_or(usize::MAX)
}
