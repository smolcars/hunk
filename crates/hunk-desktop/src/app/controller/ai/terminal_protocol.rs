#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AiTerminalGridPoint {
    line: i32,
    column: usize,
}

fn ai_terminal_focus_bytes(
    focused: bool,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    hunk_terminal::terminal_focus_input_bytes(focused, mode)
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
    hunk_terminal::terminal_mouse_button_input_bytes(
        ai_terminal_terminal_grid_point(point),
        ai_terminal_mouse_button(button)?,
        ai_terminal_input_modifiers(modifiers),
        pressed,
        mode,
    )
}

fn ai_terminal_mouse_move_bytes(
    point: AiTerminalGridPoint,
    button: Option<MouseButton>,
    modifiers: gpui::Modifiers,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    hunk_terminal::terminal_mouse_move_input_bytes(
        ai_terminal_terminal_grid_point(point),
        button.and_then(ai_terminal_mouse_button),
        ai_terminal_input_modifiers(modifiers),
        mode,
    )
}

fn ai_terminal_mouse_scroll_bytes(
    point: AiTerminalGridPoint,
    scroll_lines: i32,
    modifiers: gpui::Modifiers,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<Vec<u8>>> {
    hunk_terminal::terminal_mouse_scroll_input_bytes(
        ai_terminal_terminal_grid_point(point),
        scroll_lines,
        ai_terminal_input_modifiers(modifiers),
        mode,
    )
}

fn ai_terminal_alt_scroll_bytes(
    scroll_lines: i32,
    mode: Option<hunk_terminal::TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    hunk_terminal::terminal_alt_scroll_input_bytes(scroll_lines, mode)
}

fn ai_terminal_paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    hunk_terminal::terminal_paste_input_bytes(text, bracketed)
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
    let keystroke = hunk_terminal::TerminalKeystroke {
        key: keystroke.key.as_str(),
        key_char: keystroke.key_char.as_deref(),
        modifiers: ai_terminal_input_modifiers(keystroke.modifiers),
    };
    hunk_terminal::terminal_keystroke_input_bytes(&keystroke, mode)
}

fn ai_terminal_terminal_grid_point(point: AiTerminalGridPoint) -> hunk_terminal::TerminalGridPoint {
    hunk_terminal::TerminalGridPoint {
        line: point.line,
        column: point.column,
    }
}

fn ai_terminal_input_modifiers(modifiers: gpui::Modifiers) -> hunk_terminal::TerminalInputModifiers {
    hunk_terminal::TerminalInputModifiers {
        shift: modifiers.shift,
        alt: modifiers.alt,
        control: modifiers.control,
        platform: modifiers.platform,
        function: modifiers.function,
    }
}

fn ai_terminal_mouse_button(button: MouseButton) -> Option<hunk_terminal::TerminalMouseButton> {
    match button {
        MouseButton::Left => Some(hunk_terminal::TerminalMouseButton::Left),
        MouseButton::Middle => Some(hunk_terminal::TerminalMouseButton::Middle),
        MouseButton::Right => Some(hunk_terminal::TerminalMouseButton::Right),
        MouseButton::Navigate(_) => None,
    }
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

fn ai_terminal_uses_insert_paste_shortcut(keystroke: &gpui::Keystroke) -> bool {
    #[cfg(target_os = "macos")]
    {
        let _ = keystroke;
        false
    }

    #[cfg(not(target_os = "macos"))]
    {
        keystroke.modifiers.shift
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.platform
            && !keystroke.modifiers.function
            && keystroke.key == "insert"
    }
}

#[cfg(test)]
mod terminal_protocol_tests {
    use super::{
        AiTerminalGridPoint, ai_terminal_alt_scroll_bytes, ai_terminal_focus_bytes,
        ai_terminal_grid_point_from_position, ai_terminal_input_bytes_for_keystroke,
        ai_terminal_mouse_button_bytes, ai_terminal_mouse_move_bytes,
        ai_terminal_mouse_scroll_bytes, ai_terminal_paste_bytes, ai_terminal_uses_copy_shortcut,
        ai_terminal_uses_desktop_clipboard_shortcut, ai_terminal_uses_insert_paste_shortcut,
        ai_terminal_viewport_scroll_for_keystroke,
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
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("ctrl-z").expect("valid ctrl-z keystroke"),
                None,
            ),
            Some(vec![0x1a])
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
    fn terminal_keystroke_translation_maps_common_word_navigation_shortcuts() {
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("alt-left").expect("valid alt-left keystroke"),
                None,
            ),
            Some(b"\x1bb".to_vec())
        );
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("ctrl-right").expect("valid ctrl-right keystroke"),
                None,
            ),
            Some(b"\x1bf".to_vec())
        );
        assert_eq!(
            ai_terminal_input_bytes_for_keystroke(
                &Keystroke::parse("alt-backspace").expect("valid alt-backspace keystroke"),
                None,
            ),
            Some(vec![0x1b, 0x7f])
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
    fn terminal_insert_paste_shortcut_matches_platform_conventions() {
        #[cfg(target_os = "macos")]
        {
            assert!(!ai_terminal_uses_insert_paste_shortcut(
                &Keystroke::parse("shift-insert").expect("valid shift-insert keystroke")
            ));
        }

        #[cfg(not(target_os = "macos"))]
        {
            assert!(ai_terminal_uses_insert_paste_shortcut(
                &Keystroke::parse("shift-insert").expect("valid shift-insert keystroke")
            ));
            assert!(!ai_terminal_uses_insert_paste_shortcut(
                &Keystroke::parse("insert").expect("valid insert keystroke")
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
            Some(b"\x1b[I".to_vec())
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
