#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AiTerminalGridPoint {
    line: i32,
    column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTerminalMouseFormat {
    Sgr,
    Normal { utf8: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTerminalMouseButtonCode {
    LeftButton = 0,
    MiddleButton = 1,
    RightButton = 2,
    LeftMove = 32,
    MiddleMove = 33,
    RightMove = 34,
    NoneMove = 35,
    ScrollUp = 64,
    ScrollDown = 65,
}

impl AiTerminalMouseButtonCode {
    fn from_button(button: MouseButton) -> Option<Self> {
        match button {
            MouseButton::Left => Some(Self::LeftButton),
            MouseButton::Middle => Some(Self::MiddleButton),
            MouseButton::Right => Some(Self::RightButton),
            MouseButton::Navigate(_) => None,
        }
    }

    fn from_move_button(button: Option<MouseButton>) -> Option<Self> {
        match button {
            Some(MouseButton::Left) => Some(Self::LeftMove),
            Some(MouseButton::Middle) => Some(Self::MiddleMove),
            Some(MouseButton::Right) => Some(Self::RightMove),
            Some(MouseButton::Navigate(_)) => None,
            None => Some(Self::NoneMove),
        }
    }

    fn from_scroll_lines(scroll_lines: i32) -> Option<Self> {
        if scroll_lines > 0 {
            Some(Self::ScrollUp)
        } else if scroll_lines < 0 {
            Some(Self::ScrollDown)
        } else {
            None
        }
    }
}

fn ai_terminal_focus_bytes(
    focused: bool,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<&'static [u8]> {
    if !mode.unwrap_or_default().focus_in_out {
        return None;
    }

    Some(if focused { b"\x1b[I" } else { b"\x1b[O" })
}

#[cfg(test)]
fn ai_terminal_grid_point_from_position(
    screen: &TerminalScreenSnapshot,
    bounds_origin: gpui::Point<Pixels>,
    position: gpui::Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
) -> AiTerminalGridPoint {
    let max_column = usize::from(screen.cols.saturating_sub(1));
    let max_visible_line = i32::from(screen.rows.saturating_sub(1));
    let relative_x = (position.x - bounds_origin.x).max(px(0.0));
    let relative_y = (position.y - bounds_origin.y).max(px(0.0));
    let column = ((relative_x / cell_width) as usize).min(max_column);
    let visible_line = ((relative_y / line_height) as i32).clamp(0, max_visible_line);

    AiTerminalGridPoint {
        line: visible_line - screen.display_offset as i32,
        column,
    }
}

fn ai_terminal_mouse_button_bytes(
    point: AiTerminalGridPoint,
    button: MouseButton,
    modifiers: gpui::Modifiers,
    pressed: bool,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || !mode.mouse_mode {
        return None;
    }

    let button = AiTerminalMouseButtonCode::from_button(button)?;
    ai_terminal_mouse_report(point, button, pressed, modifiers, mode)
}

fn ai_terminal_mouse_move_bytes(
    point: AiTerminalGridPoint,
    button: Option<MouseButton>,
    modifiers: gpui::Modifiers,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || (!mode.mouse_motion && !mode.mouse_drag) {
        return None;
    }

    let button = AiTerminalMouseButtonCode::from_move_button(button)?;
    if mode.mouse_drag && matches!(button, AiTerminalMouseButtonCode::NoneMove) {
        return None;
    }

    ai_terminal_mouse_report(point, button, true, modifiers, mode)
}

fn ai_terminal_mouse_scroll_bytes(
    point: AiTerminalGridPoint,
    scroll_lines: i32,
    modifiers: gpui::Modifiers,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<Vec<u8>>> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || !mode.mouse_mode {
        return None;
    }

    let button = AiTerminalMouseButtonCode::from_scroll_lines(scroll_lines)?;
    let report = ai_terminal_mouse_report(point, button, true, modifiers, mode)?;
    Some(
        std::iter::repeat_n(report, scroll_lines.unsigned_abs() as usize).collect::<Vec<_>>(),
    )
}

fn ai_terminal_alt_scroll_bytes(
    scroll_lines: i32,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    let mode = mode.unwrap_or_default();
    if !mode.alt_screen || !mode.alternate_scroll || mode.mouse_mode || scroll_lines == 0 {
        return None;
    }

    let command = if scroll_lines > 0 { b'A' } else { b'B' };
    let mut bytes = Vec::with_capacity(scroll_lines.unsigned_abs() as usize * 3);
    for _ in 0..scroll_lines.abs() {
        bytes.extend_from_slice(&[0x1b, b'O', command]);
    }
    Some(bytes)
}

fn ai_terminal_mouse_report(
    point: AiTerminalGridPoint,
    button: AiTerminalMouseButtonCode,
    pressed: bool,
    modifiers: gpui::Modifiers,
    mode: hunk_terminal::TerminalModeSnapshot,
) -> Option<Vec<u8>> {
    if point.line < 0 {
        return None;
    }

    let format = if mode.sgr_mouse {
        AiTerminalMouseFormat::Sgr
    } else {
        AiTerminalMouseFormat::Normal {
            utf8: mode.utf8_mouse,
        }
    };
    let mut modifier_bits = 0;
    if modifiers.shift {
        modifier_bits += 4;
    }
    if modifiers.alt {
        modifier_bits += 8;
    }
    if modifiers.control {
        modifier_bits += 16;
    }

    match format {
        AiTerminalMouseFormat::Sgr => Some(
            ai_terminal_sgr_mouse_report(point, button as u8 + modifier_bits, pressed)
                .into_bytes(),
        ),
        AiTerminalMouseFormat::Normal { utf8 } => {
            let button_code = if pressed {
                button as u8 + modifier_bits
            } else {
                3 + modifier_bits
            };
            ai_terminal_normal_mouse_report(point, button_code, utf8)
        }
    }
}

fn ai_terminal_normal_mouse_report(
    point: AiTerminalGridPoint,
    button: u8,
    utf8: bool,
) -> Option<Vec<u8>> {
    let max_point = if utf8 { 2015 } else { 223 };
    let line = usize::try_from(point.line).ok()?;
    if line >= max_point || point.column >= max_point {
        return None;
    }

    let mut bytes = vec![b'\x1b', b'[', b'M', 32 + button];

    if utf8 && point.column >= 95 {
        bytes.extend_from_slice(ai_terminal_utf8_mouse_position_bytes(point.column).as_slice());
    } else {
        bytes.push(32 + 1 + point.column as u8);
    }

    if utf8 && line >= 95 {
        bytes.extend_from_slice(ai_terminal_utf8_mouse_position_bytes(line).as_slice());
    } else {
        bytes.push(32 + 1 + line as u8);
    }

    Some(bytes)
}

fn ai_terminal_utf8_mouse_position_bytes(position: usize) -> [u8; 2] {
    let value = 32 + 1 + position;
    let first = 0xC0 + value / 64;
    let second = 0x80 + (value & 63);
    [first as u8, second as u8]
}

fn ai_terminal_sgr_mouse_report(
    point: AiTerminalGridPoint,
    button: u8,
    pressed: bool,
) -> String {
    let suffix = if pressed { 'M' } else { 'm' };
    format!(
        "\x1b[<{};{};{}{}",
        button,
        point.column + 1,
        point.line + 1,
        suffix
    )
}

fn ai_terminal_paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let mut bytes = Vec::with_capacity(text.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        bytes
    } else {
        text.as_bytes().to_vec()
    }
}

fn ai_terminal_viewport_scroll_for_keystroke(
    keystroke: &gpui::Keystroke,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<TerminalScroll> {
    if mode.is_some_and(|mode| mode.alt_screen) {
        return None;
    }

    if !keystroke.modifiers.shift
        || keystroke.modifiers.alt
        || keystroke.modifiers.control
        || keystroke.modifiers.platform
        || keystroke.modifiers.function
    {
        return None;
    }

    match keystroke.key.as_str() {
        "pageup" => Some(TerminalScroll::PageUp),
        "pagedown" => Some(TerminalScroll::PageDown),
        "home" => Some(TerminalScroll::Top),
        "end" => Some(TerminalScroll::Bottom),
        _ => None,
    }
}

fn ai_terminal_uses_copy_shortcut(keystroke: &gpui::Keystroke) -> bool {
    #[cfg(target_os = "macos")]
    {
        keystroke.modifiers.platform
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift
            && !keystroke.modifiers.function
            && keystroke.key == "c"
    }

    #[cfg(not(target_os = "macos"))]
    {
        keystroke.modifiers.control
            && keystroke.modifiers.shift
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.function
            && keystroke.key == "c"
    }
}

fn ai_terminal_input_bytes_for_keystroke(
    keystroke: &gpui::Keystroke,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    if keystroke.modifiers.platform || keystroke.modifiers.function {
        return None;
    }

    let mode = mode.unwrap_or_default();

    match keystroke.key.as_str() {
        "enter" => return Some(vec![b'\r']),
        "tab" => {
            return Some(if keystroke.modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            });
        }
        "backspace" => return Some(vec![0x7f]),
        "escape" => return Some(vec![0x1b]),
        "home"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[1;2H".to_vec())
        }
        "end"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[1;2F".to_vec())
        }
        "pageup"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[5;2~".to_vec())
        }
        "pagedown"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[6;2~".to_vec())
        }
        "up"
            if !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.shift =>
        {
            return Some(if mode.app_cursor {
                b"\x1bOA".to_vec()
            } else {
                b"\x1b[A".to_vec()
            })
        }
        "down"
            if !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.shift =>
        {
            return Some(if mode.app_cursor {
                b"\x1bOB".to_vec()
            } else {
                b"\x1b[B".to_vec()
            })
        }
        "right"
            if !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.shift =>
        {
            return Some(if mode.app_cursor {
                b"\x1bOC".to_vec()
            } else {
                b"\x1b[C".to_vec()
            })
        }
        "left"
            if !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.shift =>
        {
            return Some(if mode.app_cursor {
                b"\x1bOD".to_vec()
            } else {
                b"\x1b[D".to_vec()
            })
        }
        "home"
            if !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.shift =>
        {
            return Some(if mode.app_cursor {
                b"\x1bOH".to_vec()
            } else {
                b"\x1b[H".to_vec()
            })
        }
        "end"
            if !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.shift =>
        {
            return Some(if mode.app_cursor {
                b"\x1bOF".to_vec()
            } else {
                b"\x1b[F".to_vec()
            })
        }
        "up" => return Some(b"\x1b[A".to_vec()),
        "down" => return Some(b"\x1b[B".to_vec()),
        "right" => return Some(b"\x1b[C".to_vec()),
        "left" => return Some(b"\x1b[D".to_vec()),
        "home" => return Some(b"\x1b[H".to_vec()),
        "end" => return Some(b"\x1b[F".to_vec()),
        "pageup" => return Some(b"\x1b[5~".to_vec()),
        "pagedown" => return Some(b"\x1b[6~".to_vec()),
        "delete" => return Some(b"\x1b[3~".to_vec()),
        "space" => {
            if keystroke.modifiers.control {
                return Some(vec![0x00]);
            }
        }
        _ => {}
    }

    if keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.shift
        && let Some(control) = ai_terminal_control_byte(keystroke.key.as_str())
    {
        return Some(vec![control]);
    }

    let text = keystroke
        .key_char
        .as_deref()
        .unwrap_or(keystroke.key.as_str());
    if text.is_empty() {
        return None;
    }

    let mut bytes = Vec::with_capacity(text.len() + usize::from(keystroke.modifiers.alt));
    if keystroke.modifiers.alt {
        bytes.push(0x1b);
    }
    bytes.extend_from_slice(text.as_bytes());
    Some(bytes)
}

fn ai_terminal_control_byte(key: &str) -> Option<u8> {
    if key.len() == 1 {
        let byte = key.as_bytes()[0];
        return match byte.to_ascii_lowercase() {
            b'a'..=b'z' => Some((byte.to_ascii_lowercase() - b'a') + 1),
            b'[' => Some(0x1b),
            b'\\' => Some(0x1c),
            b']' => Some(0x1d),
            b'^' => Some(0x1e),
            b'_' => Some(0x1f),
            _ => None,
        };
    }

    None
}

fn ai_terminal_uses_desktop_clipboard_shortcut(keystroke: &gpui::Keystroke) -> bool {
    #[cfg(target_os = "macos")]
    {
        keystroke.modifiers.platform
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.function
            && matches!(keystroke.key.as_str(), "c" | "x" | "v")
    }

    #[cfg(not(target_os = "macos"))]
    {
        keystroke.modifiers.control
            && keystroke.modifiers.shift
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.function
            && matches!(keystroke.key.as_str(), "c" | "x" | "v")
    }
}

#[cfg(test)]
mod terminal_protocol_tests {
    use super::{
        AiTerminalGridPoint, ai_terminal_alt_scroll_bytes, ai_terminal_focus_bytes,
        ai_terminal_grid_point_from_position, ai_terminal_input_bytes_for_keystroke,
        ai_terminal_mouse_button_bytes, ai_terminal_mouse_move_bytes,
        ai_terminal_mouse_scroll_bytes, ai_terminal_paste_bytes, ai_terminal_uses_copy_shortcut,
        ai_terminal_uses_desktop_clipboard_shortcut, ai_terminal_viewport_scroll_for_keystroke,
    };
    use gpui::{Keystroke, MouseButton, ScrollDelta, TouchPhase, point, px};
    use hunk_terminal::{
        TerminalModeSnapshot, TerminalScroll,
    };

    #[test]
    fn terminal_keystroke_translation_handles_enter_and_arrows() {
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("enter").expect("valid enter keystroke"),
                None,
            ),
            Some(vec![b'\r'])
        );
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("up").expect("valid up keystroke"),
                None,
            ),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn terminal_keystroke_translation_maps_control_shortcuts() {
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("ctrl-c").expect("valid ctrl-c keystroke"),
                None,
            ),
            Some(vec![0x03])
        );
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("ctrl-space").expect("valid ctrl-space keystroke"),
                None,
            ),
            Some(vec![0x00])
        );
    }

    #[test]
    fn terminal_keystroke_translation_respects_app_cursor_mode() {
        let mode = TerminalModeSnapshot {
            app_cursor: true,
            ..TerminalModeSnapshot::default()
        };

        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("up").expect("valid up keystroke"),
                Some(mode),
            ),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("home").expect("valid home keystroke"),
                Some(mode),
            ),
            Some(b"\x1bOH".to_vec())
        );
    }

    #[test]
    fn terminal_keystroke_translation_preserves_alt_screen_navigation_input() {
        let mode = TerminalModeSnapshot {
            alt_screen: true,
            ..TerminalModeSnapshot::default()
        };

        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("shift-pageup").expect("valid shift-pageup keystroke"),
                Some(mode),
            ),
            Some(b"\x1b[5;2~".to_vec())
        );
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("shift-end").expect("valid shift-end keystroke"),
                Some(mode),
            ),
            Some(b"\x1b[1;2F".to_vec())
        );
    }

    #[test]
    fn terminal_paste_bytes_wrap_bracketed_paste_when_requested() {
        assert_eq!(
            ai_terminal_paste_bytes("echo hi", true),
            b"\x1b[200~echo hi\x1b[201~".to_vec()
        );
        assert_eq!(ai_terminal_paste_bytes("echo hi", false), b"echo hi".to_vec());
    }

    #[test]
    fn terminal_clipboard_shortcuts_match_platform_conventions() {
        #[cfg(target_os = "macos")]
        {
            assert!(ai_terminal_uses_desktop_clipboard_shortcut(
                &Keystroke::parse("cmd-v").expect("valid cmd-v keystroke")
            ));
            assert!(!ai_terminal_uses_desktop_clipboard_shortcut(
                &Keystroke::parse("ctrl-c").expect("valid ctrl-c keystroke")
            ));
        }

        #[cfg(not(target_os = "macos"))]
        {
            assert!(ai_terminal_uses_desktop_clipboard_shortcut(
                &Keystroke::parse("ctrl-shift-v").expect("valid ctrl-shift-v keystroke")
            ));
            assert!(!ai_terminal_uses_desktop_clipboard_shortcut(
                &Keystroke::parse("ctrl-c").expect("valid ctrl-c keystroke")
            ));
        }
    }

    #[test]
    fn terminal_viewport_scroll_shortcuts_use_shift_navigation_keys() {
        assert_eq!(
            ai_terminal_viewport_scroll_for_keystroke(
                &Keystroke::parse("shift-pageup").expect("valid shift-pageup keystroke"),
                None,
            ),
            Some(TerminalScroll::PageUp)
        );
        assert_eq!(
            ai_terminal_viewport_scroll_for_keystroke(
                &Keystroke::parse("shift-end").expect("valid shift-end keystroke"),
                None,
            ),
            Some(TerminalScroll::Bottom)
        );
        assert_eq!(
            ai_terminal_viewport_scroll_for_keystroke(
                &Keystroke::parse("pageup").expect("valid pageup keystroke"),
                None,
            ),
            None
        );
    }

    #[test]
    fn terminal_viewport_scroll_shortcuts_are_disabled_in_alt_screen() {
        let mode = TerminalModeSnapshot {
            alt_screen: true,
            ..TerminalModeSnapshot::default()
        };

        assert_eq!(
            ai_terminal_viewport_scroll_for_keystroke(
                &Keystroke::parse("shift-pageup").expect("valid shift-pageup keystroke"),
                Some(mode),
            ),
            None
        );
    }

    #[test]
    fn terminal_copy_shortcuts_match_terminal_platform_conventions() {
        #[cfg(target_os = "macos")]
        {
            assert!(ai_terminal_uses_copy_shortcut(
                &Keystroke::parse("cmd-c").expect("valid cmd-c keystroke")
            ));
            assert!(!ai_terminal_uses_copy_shortcut(
                &Keystroke::parse("ctrl-c").expect("valid ctrl-c keystroke")
            ));
        }

        #[cfg(not(target_os = "macos"))]
        {
            assert!(ai_terminal_uses_copy_shortcut(
                &Keystroke::parse("ctrl-shift-c").expect("valid ctrl-shift-c keystroke")
            ));
            assert!(!ai_terminal_uses_copy_shortcut(
                &Keystroke::parse("ctrl-c").expect("valid ctrl-c keystroke")
            ));
        }
    }

    #[test]
    fn terminal_focus_reports_follow_focus_mode() {
        assert_eq!(
            ai_terminal_focus_bytes(
                true,
                Some(TerminalModeSnapshot {
                    focus_in_out: true,
                    ..TerminalModeSnapshot::default()
                }),
            ),
            Some(&b"\x1b[I"[..])
        );
        assert_eq!(ai_terminal_focus_bytes(false, None), None);
    }

    #[test]
    fn terminal_grid_point_accounts_for_visible_scrollback_offset() {
        let screen = hunk_terminal::TerminalScreenSnapshot {
            rows: 4,
            cols: 8,
            display_offset: 2,
            cursor: hunk_terminal::TerminalCursorSnapshot {
                line: 0,
                column: 0,
                shape: hunk_terminal::TerminalCursorShapeSnapshot::Block,
            },
            mode: TerminalModeSnapshot::default(),
            damage: hunk_terminal::TerminalDamageSnapshot::Full,
            cells: Vec::new(),
        };

        let point = ai_terminal_grid_point_from_position(
            &screen,
            point(px(10.0), px(20.0)),
            point(px(39.0), px(59.0)),
            px(8.0),
            px(16.0),
        );

        assert_eq!(
            point,
            AiTerminalGridPoint {
                line: 0,
                column: 3,
            }
        );
    }

    #[test]
    fn terminal_mouse_reports_prefer_sgr_and_skip_shift_override() {
        let point = AiTerminalGridPoint { line: 4, column: 2 };
        let mode = TerminalModeSnapshot {
            mouse_mode: true,
            sgr_mouse: true,
            ..TerminalModeSnapshot::default()
        };

        assert_eq!(
            ai_terminal_mouse_button_bytes(
                point,
                MouseButton::Left,
                gpui::Modifiers::default(),
                true,
                Some(mode),
            ),
            Some(b"\x1b[<0;3;5M".to_vec())
        );

        let modifiers = gpui::Modifiers {
            shift: true,
            ..gpui::Modifiers::default()
        };
        assert_eq!(
            ai_terminal_mouse_button_bytes(point, MouseButton::Left, modifiers, true, Some(mode)),
            None
        );
    }

    #[test]
    fn terminal_mouse_move_reports_follow_drag_and_motion_modes() {
        let point = AiTerminalGridPoint { line: 1, column: 1 };

        assert_eq!(
            ai_terminal_mouse_move_bytes(
                point,
                Some(MouseButton::Left),
                gpui::Modifiers::default(),
                Some(TerminalModeSnapshot {
                    mouse_drag: true,
                    ..TerminalModeSnapshot::default()
                }),
            ),
            Some(b"\x1b[M@\"\"".to_vec())
        );

        assert_eq!(
            ai_terminal_mouse_move_bytes(
                point,
                None,
                gpui::Modifiers::default(),
                Some(TerminalModeSnapshot {
                    mouse_drag: true,
                    ..TerminalModeSnapshot::default()
                }),
            ),
            None
        );
    }

    #[test]
    fn terminal_scroll_reports_repeat_for_each_line() {
        let reports = ai_terminal_mouse_scroll_bytes(
            AiTerminalGridPoint { line: 2, column: 4 },
            -3,
            gpui::Modifiers::default(),
            Some(TerminalModeSnapshot {
                mouse_mode: true,
                ..TerminalModeSnapshot::default()
            }),
        )
        .expect("mouse mode should produce reports");

        assert_eq!(reports.len(), 3);
        assert!(reports.iter().all(|report| report == b"\x1b[Ma%#"));
    }

    #[test]
    fn terminal_alt_scroll_requires_alt_screen_mode() {
        assert_eq!(
            ai_terminal_alt_scroll_bytes(
                2,
                Some(TerminalModeSnapshot {
                    alt_screen: true,
                    alternate_scroll: true,
                    ..TerminalModeSnapshot::default()
                }),
            ),
            Some(b"\x1bOA\x1bOA".to_vec())
        );
        assert_eq!(ai_terminal_alt_scroll_bytes(2, None), None);
    }

    #[test]
    fn terminal_scroll_reports_ignore_zero_scroll_delta() {
        let event = gpui::ScrollWheelEvent {
            delta: ScrollDelta::Lines(point(0.0, 0.0)),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        };

        assert_eq!(
            ai_terminal_mouse_scroll_bytes(
                AiTerminalGridPoint { line: 0, column: 0 },
                match event.delta {
                    ScrollDelta::Lines(lines) => lines.y as i32,
                    ScrollDelta::Pixels(_) => 0,
                },
                event.modifiers,
                Some(TerminalModeSnapshot {
                    mouse_mode: true,
                    ..TerminalModeSnapshot::default()
                }),
            ),
            None
        );
    }
}
