use libghostty_vt::focus;

use crate::TerminalModeSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalInputModifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
    pub platform: bool,
    pub function: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalGridPoint {
    pub line: i32,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalKeystroke<'a> {
    pub key: &'a str,
    pub key_char: Option<&'a str>,
    pub modifiers: TerminalInputModifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalPointerInput {
    Button {
        point: TerminalGridPoint,
        button: TerminalMouseButton,
        modifiers: TerminalInputModifiers,
        pressed: bool,
    },
    Move {
        point: TerminalGridPoint,
        button: Option<TerminalMouseButton>,
        modifiers: TerminalInputModifiers,
    },
    Scroll {
        point: TerminalGridPoint,
        scroll_lines: i32,
        modifiers: TerminalInputModifiers,
    },
}

pub fn terminal_focus_input_bytes(
    focused: bool,
    mode: Option<TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    if !mode.unwrap_or_default().focus_in_out {
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

pub fn terminal_mouse_button_input(
    point: TerminalGridPoint,
    button: TerminalMouseButton,
    modifiers: TerminalInputModifiers,
    pressed: bool,
    mode: Option<TerminalModeSnapshot>,
) -> Option<TerminalPointerInput> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || !mode.mouse_mode {
        return None;
    }

    Some(TerminalPointerInput::Button {
        point,
        button,
        modifiers,
        pressed,
    })
}

pub fn terminal_mouse_move_input(
    point: TerminalGridPoint,
    button: Option<TerminalMouseButton>,
    modifiers: TerminalInputModifiers,
    mode: Option<TerminalModeSnapshot>,
) -> Option<TerminalPointerInput> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || (!mode.mouse_motion && !mode.mouse_drag) {
        return None;
    }
    if mode.mouse_drag && button.is_none() {
        return None;
    }

    Some(TerminalPointerInput::Move {
        button,
        point,
        modifiers,
    })
}

pub fn terminal_mouse_scroll_input(
    point: TerminalGridPoint,
    scroll_lines: i32,
    modifiers: TerminalInputModifiers,
    mode: Option<TerminalModeSnapshot>,
) -> Option<TerminalPointerInput> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || !mode.mouse_mode || scroll_lines == 0 {
        return None;
    }

    Some(TerminalPointerInput::Scroll {
        point,
        scroll_lines,
        modifiers,
    })
}

pub fn terminal_alt_scroll_input_bytes(
    scroll_lines: i32,
    mode: Option<TerminalModeSnapshot>,
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

pub fn terminal_paste_input_bytes(text: &str, bracketed: bool) -> Vec<u8> {
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

pub fn terminal_keystroke_input_bytes(
    keystroke: &TerminalKeystroke<'_>,
    mode: Option<TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    if keystroke.modifiers.platform || keystroke.modifiers.function {
        return None;
    }

    let mode = mode.unwrap_or_default();

    match keystroke.key {
        "enter" => return Some(vec![b'\r']),
        "tab" => {
            return Some(if keystroke.modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            });
        }
        "backspace" if keystroke.modifiers.alt && !keystroke.modifiers.control => {
            return Some(vec![0x1b, 0x7f]);
        }
        "backspace" => return Some(vec![0x7f]),
        "escape" => return Some(vec![0x1b]),
        "home"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[1;2H".to_vec());
        }
        "end"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[1;2F".to_vec());
        }
        "pageup"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[5;2~".to_vec());
        }
        "pagedown"
            if keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && mode.alt_screen =>
        {
            return Some(b"\x1b[6;2~".to_vec());
        }
        "up" if no_navigation_modifiers(keystroke.modifiers) => {
            return Some(if mode.app_cursor {
                b"\x1bOA".to_vec()
            } else {
                b"\x1b[A".to_vec()
            });
        }
        "down" if no_navigation_modifiers(keystroke.modifiers) => {
            return Some(if mode.app_cursor {
                b"\x1bOB".to_vec()
            } else {
                b"\x1b[B".to_vec()
            });
        }
        "right" if no_navigation_modifiers(keystroke.modifiers) => {
            return Some(if mode.app_cursor {
                b"\x1bOC".to_vec()
            } else {
                b"\x1b[C".to_vec()
            });
        }
        "left" if no_navigation_modifiers(keystroke.modifiers) => {
            return Some(if mode.app_cursor {
                b"\x1bOD".to_vec()
            } else {
                b"\x1b[D".to_vec()
            });
        }
        "left"
            if !keystroke.modifiers.shift
                && ((keystroke.modifiers.alt && !keystroke.modifiers.control)
                    || (keystroke.modifiers.control && !keystroke.modifiers.alt)) =>
        {
            return Some(b"\x1bb".to_vec());
        }
        "right"
            if !keystroke.modifiers.shift
                && ((keystroke.modifiers.alt && !keystroke.modifiers.control)
                    || (keystroke.modifiers.control && !keystroke.modifiers.alt)) =>
        {
            return Some(b"\x1bf".to_vec());
        }
        "home" if no_navigation_modifiers(keystroke.modifiers) => {
            return Some(if mode.app_cursor {
                b"\x1bOH".to_vec()
            } else {
                b"\x1b[H".to_vec()
            });
        }
        "end" if no_navigation_modifiers(keystroke.modifiers) => {
            return Some(if mode.app_cursor {
                b"\x1bOF".to_vec()
            } else {
                b"\x1b[F".to_vec()
            });
        }
        "up" => return Some(b"\x1b[A".to_vec()),
        "down" => return Some(b"\x1b[B".to_vec()),
        "right" => return Some(b"\x1b[C".to_vec()),
        "left" => return Some(b"\x1b[D".to_vec()),
        "home" => return Some(b"\x1b[H".to_vec()),
        "end" => return Some(b"\x1b[F".to_vec()),
        "pageup" => return Some(b"\x1b[5~".to_vec()),
        "pagedown" => return Some(b"\x1b[6~".to_vec()),
        "delete" if keystroke.modifiers.control && !keystroke.modifiers.alt => {
            return Some(b"\x1bd".to_vec());
        }
        "delete" => return Some(b"\x1b[3~".to_vec()),
        "space" if keystroke.modifiers.control => return Some(vec![0x00]),
        _ => {}
    }

    if keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.shift
        && let Some(control) = terminal_control_byte(keystroke.key)
    {
        return Some(vec![control]);
    }

    let text = keystroke.key_char.unwrap_or(keystroke.key);
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

fn no_navigation_modifiers(modifiers: TerminalInputModifiers) -> bool {
    !modifiers.control && !modifiers.alt && !modifiers.shift
}

fn terminal_control_byte(key: &str) -> Option<u8> {
    if key.len() != 1 {
        return None;
    }

    let byte = key.as_bytes()[0];
    match byte.to_ascii_lowercase() {
        b'a'..=b'z' => Some((byte.to_ascii_lowercase() - b'a') + 1),
        b'[' => Some(0x1b),
        b'\\' => Some(0x1c),
        b']' => Some(0x1d),
        b'^' => Some(0x1e),
        b'_' => Some(0x1f),
        _ => None,
    }
}
