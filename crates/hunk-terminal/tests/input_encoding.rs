use hunk_terminal::{
    TerminalGridPoint, TerminalInputModifiers, TerminalKeystroke, TerminalModeSnapshot,
    TerminalMouseButton, TerminalPointerInput, terminal_alt_scroll_input_bytes,
    terminal_focus_input_bytes, terminal_keystroke_input_bytes, terminal_mouse_button_input,
    terminal_mouse_move_input, terminal_mouse_scroll_input, terminal_paste_input_bytes,
};

#[test]
fn terminal_keystroke_translation_handles_enter_and_arrows() {
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "enter",
                key_char: None,
                modifiers: TerminalInputModifiers::default(),
            },
            None,
        ),
        Some(vec![b'\r'])
    );
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "up",
                key_char: None,
                modifiers: TerminalInputModifiers::default(),
            },
            None,
        ),
        Some(b"\x1b[A".to_vec())
    );
}

#[test]
fn terminal_keystroke_translation_maps_control_shortcuts() {
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "c",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    control: true,
                    ..TerminalInputModifiers::default()
                },
            },
            None,
        ),
        Some(vec![0x03])
    );
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "space",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    control: true,
                    ..TerminalInputModifiers::default()
                },
            },
            None,
        ),
        Some(vec![0x00])
    );
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "z",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    control: true,
                    ..TerminalInputModifiers::default()
                },
            },
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
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "up",
                key_char: None,
                modifiers: TerminalInputModifiers::default(),
            },
            Some(mode),
        ),
        Some(b"\x1bOA".to_vec())
    );
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "home",
                key_char: None,
                modifiers: TerminalInputModifiers::default(),
            },
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
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "pageup",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    shift: true,
                    ..TerminalInputModifiers::default()
                },
            },
            Some(mode),
        ),
        Some(b"\x1b[5;2~".to_vec())
    );
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "end",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    shift: true,
                    ..TerminalInputModifiers::default()
                },
            },
            Some(mode),
        ),
        Some(b"\x1b[1;2F".to_vec())
    );
}

#[test]
fn terminal_keystroke_translation_maps_common_word_navigation_shortcuts() {
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "left",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    alt: true,
                    ..TerminalInputModifiers::default()
                },
            },
            None,
        ),
        Some(b"\x1bb".to_vec())
    );
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "right",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    control: true,
                    ..TerminalInputModifiers::default()
                },
            },
            None,
        ),
        Some(b"\x1bf".to_vec())
    );
    assert_eq!(
        terminal_keystroke_input_bytes(
            &TerminalKeystroke {
                key: "backspace",
                key_char: None,
                modifiers: TerminalInputModifiers {
                    alt: true,
                    ..TerminalInputModifiers::default()
                },
            },
            None,
        ),
        Some(vec![0x1b, 0x7f])
    );
}

#[test]
fn terminal_paste_bytes_wrap_bracketed_paste_when_requested() {
    assert_eq!(
        terminal_paste_input_bytes("echo hi", true),
        b"\x1b[200~echo hi\x1b[201~".to_vec()
    );
    assert_eq!(
        terminal_paste_input_bytes("echo hi", false),
        b"echo hi".to_vec()
    );
}

#[test]
fn terminal_focus_reports_follow_focus_mode() {
    assert_eq!(
        terminal_focus_input_bytes(
            true,
            Some(TerminalModeSnapshot {
                focus_in_out: true,
                ..TerminalModeSnapshot::default()
            }),
        ),
        Some(b"\x1b[I".to_vec())
    );
    assert_eq!(terminal_focus_input_bytes(false, None), None);
}

#[test]
fn terminal_mouse_reports_prefer_sgr_and_skip_shift_override() {
    let point = TerminalGridPoint { line: 4, column: 2 };
    let mode = TerminalModeSnapshot {
        mouse_mode: true,
        sgr_mouse: true,
        ..TerminalModeSnapshot::default()
    };

    assert_eq!(
        terminal_mouse_button_input(
            point,
            TerminalMouseButton::Left,
            TerminalInputModifiers::default(),
            true,
            Some(mode),
        ),
        Some(TerminalPointerInput::Button {
            point,
            button: TerminalMouseButton::Left,
            modifiers: TerminalInputModifiers::default(),
            pressed: true,
        })
    );

    let modifiers = TerminalInputModifiers {
        shift: true,
        ..TerminalInputModifiers::default()
    };
    assert_eq!(
        terminal_mouse_button_input(
            point,
            TerminalMouseButton::Left,
            modifiers,
            true,
            Some(mode),
        ),
        None
    );
}

#[test]
fn terminal_mouse_move_reports_follow_drag_and_motion_modes() {
    let point = TerminalGridPoint { line: 1, column: 1 };

    assert_eq!(
        terminal_mouse_move_input(
            point,
            Some(TerminalMouseButton::Left),
            TerminalInputModifiers::default(),
            Some(TerminalModeSnapshot {
                mouse_drag: true,
                ..TerminalModeSnapshot::default()
            }),
        ),
        Some(TerminalPointerInput::Move {
            point,
            button: Some(TerminalMouseButton::Left),
            modifiers: TerminalInputModifiers::default(),
        })
    );

    assert_eq!(
        terminal_mouse_move_input(
            point,
            None,
            TerminalInputModifiers::default(),
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
    let input = terminal_mouse_scroll_input(
        TerminalGridPoint { line: 2, column: 4 },
        -3,
        TerminalInputModifiers::default(),
        Some(TerminalModeSnapshot {
            mouse_mode: true,
            ..TerminalModeSnapshot::default()
        }),
    )
    .expect("mouse mode should produce pointer input");

    assert_eq!(
        input,
        TerminalPointerInput::Scroll {
            point: TerminalGridPoint { line: 2, column: 4 },
            scroll_lines: -3,
            modifiers: TerminalInputModifiers::default(),
        }
    );
}

#[test]
fn terminal_alt_scroll_requires_alt_screen_mode() {
    assert_eq!(
        terminal_alt_scroll_input_bytes(
            2,
            Some(TerminalModeSnapshot {
                alt_screen: true,
                alternate_scroll: true,
                ..TerminalModeSnapshot::default()
            }),
        ),
        Some(b"\x1bOA\x1bOA".to_vec())
    );
    assert_eq!(terminal_alt_scroll_input_bytes(2, None), None);
}

#[test]
fn terminal_scroll_reports_ignore_zero_scroll_delta() {
    assert_eq!(
        terminal_mouse_scroll_input(
            TerminalGridPoint { line: 0, column: 0 },
            0,
            TerminalInputModifiers::default(),
            Some(TerminalModeSnapshot {
                mouse_mode: true,
                ..TerminalModeSnapshot::default()
            }),
        ),
        None
    );
}
