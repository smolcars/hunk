use libghostty_vt::{focus, mouse};

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

pub fn terminal_mouse_button_input_bytes(
    point: TerminalGridPoint,
    button: TerminalMouseButton,
    modifiers: TerminalInputModifiers,
    pressed: bool,
    mode: Option<TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || !mode.mouse_mode {
        return None;
    }

    terminal_mouse_report(
        point,
        Some(button),
        if pressed {
            mouse::Action::Press
        } else {
            mouse::Action::Release
        },
        modifiers,
        mode,
        false,
    )
}

pub fn terminal_mouse_move_input_bytes(
    point: TerminalGridPoint,
    button: Option<TerminalMouseButton>,
    modifiers: TerminalInputModifiers,
    mode: Option<TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || (!mode.mouse_motion && !mode.mouse_drag) {
        return None;
    }
    if mode.mouse_drag && button.is_none() {
        return None;
    }

    terminal_mouse_report(
        point,
        button,
        mouse::Action::Motion,
        modifiers,
        mode,
        button.is_some(),
    )
}

pub fn terminal_mouse_scroll_input_bytes(
    point: TerminalGridPoint,
    scroll_lines: i32,
    modifiers: TerminalInputModifiers,
    mode: Option<TerminalModeSnapshot>,
) -> Option<Vec<Vec<u8>>> {
    let mode = mode.unwrap_or_default();
    if modifiers.shift || !mode.mouse_mode || scroll_lines == 0 {
        return None;
    }

    let button = if scroll_lines > 0 {
        mouse::Button::Four
    } else {
        mouse::Button::Five
    };

    let report = terminal_mouse_report_raw(
        point,
        Some(button),
        mouse::Action::Press,
        modifiers,
        mode,
        false,
    )?;

    Some(std::iter::repeat_n(report, scroll_lines.unsigned_abs() as usize).collect())
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

fn terminal_mouse_report(
    point: TerminalGridPoint,
    button: Option<TerminalMouseButton>,
    action: mouse::Action,
    modifiers: TerminalInputModifiers,
    mode: TerminalModeSnapshot,
    any_button_pressed: bool,
) -> Option<Vec<u8>> {
    let button = button.map(terminal_mouse_button);
    terminal_mouse_report_raw(point, button, action, modifiers, mode, any_button_pressed)
}

fn terminal_mouse_report_raw(
    point: TerminalGridPoint,
    button: Option<mouse::Button>,
    action: mouse::Action,
    modifiers: TerminalInputModifiers,
    mode: TerminalModeSnapshot,
    any_button_pressed: bool,
) -> Option<Vec<u8>> {
    if point.line < 0 {
        return None;
    }

    let mut encoder = mouse::Encoder::new().ok()?;
    encoder
        .set_tracking_mode(terminal_mouse_tracking_mode(mode))
        .set_format(terminal_mouse_format(mode))
        .set_size(mouse::EncoderSize {
            screen_width: point.column.saturating_add(1) as u32,
            screen_height: usize::try_from(point.line).ok()?.saturating_add(1) as u32,
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
        .set_mods(terminal_mouse_mods(modifiers))
        .set_position(mouse::Position {
            x: point.column as f32,
            y: point.line as f32,
        });

    match button {
        Some(button) => {
            event.set_button(Some(button));
        }
        None => {
            event.set_button(None);
        }
    }

    let mut bytes = Vec::new();
    encoder.encode_to_vec(&event, &mut bytes).ok()?;
    if bytes.is_empty() { None } else { Some(bytes) }
}

fn terminal_mouse_button(button: TerminalMouseButton) -> mouse::Button {
    match button {
        TerminalMouseButton::Left => mouse::Button::Left,
        TerminalMouseButton::Middle => mouse::Button::Middle,
        TerminalMouseButton::Right => mouse::Button::Right,
    }
}

fn terminal_mouse_tracking_mode(mode: TerminalModeSnapshot) -> mouse::TrackingMode {
    if mode.mouse_motion {
        mouse::TrackingMode::Any
    } else if mode.mouse_drag {
        mouse::TrackingMode::Button
    } else if mode.mouse_mode {
        mouse::TrackingMode::Normal
    } else {
        mouse::TrackingMode::None
    }
}

fn terminal_mouse_format(mode: TerminalModeSnapshot) -> mouse::Format {
    if mode.sgr_mouse {
        mouse::Format::Sgr
    } else if mode.utf8_mouse {
        mouse::Format::Utf8
    } else {
        mouse::Format::X10
    }
}

fn terminal_mouse_mods(modifiers: TerminalInputModifiers) -> libghostty_vt::key::Mods {
    let mut mods = libghostty_vt::key::Mods::empty();
    if modifiers.shift {
        mods |= libghostty_vt::key::Mods::SHIFT;
    }
    if modifiers.alt {
        mods |= libghostty_vt::key::Mods::ALT;
    }
    if modifiers.control {
        mods |= libghostty_vt::key::Mods::CTRL;
    }
    mods
}
