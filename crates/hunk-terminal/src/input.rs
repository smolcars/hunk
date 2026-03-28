use libghostty_vt::{Terminal, TerminalOptions, focus, key, terminal::Mode};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalKeyInput {
    pub key: String,
    pub key_char: Option<String>,
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

pub fn terminal_key_input(keystroke: &TerminalKeystroke<'_>) -> Option<TerminalKeyInput> {
    if keystroke.modifiers.platform || keystroke.modifiers.function {
        return None;
    }

    Some(TerminalKeyInput {
        key: keystroke.key.to_string(),
        key_char: keystroke.key_char.map(ToOwned::to_owned),
        modifiers: keystroke.modifiers,
    })
}

pub fn terminal_keystroke_input_bytes(
    keystroke: &TerminalKeystroke<'_>,
    mode: Option<TerminalModeSnapshot>,
) -> Option<Vec<u8>> {
    let input = terminal_key_input(keystroke)?;
    let mode = mode.unwrap_or_default();
    let mut terminal = Terminal::new(TerminalOptions {
        cols: 80,
        rows: 24,
        max_scrollback: 0,
    })
    .ok()?;
    apply_key_terminal_mode(&mut terminal, mode).ok()?;

    let mut encoder = key::Encoder::new().ok()?;
    terminal_key_input_bytes(&input, &terminal, &mut encoder)
}

pub(crate) fn terminal_key_input_bytes(
    input: &TerminalKeyInput,
    terminal: &Terminal<'_, '_>,
    encoder: &mut key::Encoder<'_>,
) -> Option<Vec<u8>> {
    if let Some(bytes) = terminal_compat_key_input_bytes(input) {
        return Some(bytes);
    }

    let (key, unshifted_codepoint) = terminal_key_spec(input)?;
    let text = terminal_key_text(input);
    let mods = terminal_key_mods(input.modifiers);

    let mut consumed_mods = key::Mods::empty();
    if unshifted_codepoint != '\0' && mods.contains(key::Mods::SHIFT) && text.is_some() {
        consumed_mods |= key::Mods::SHIFT;
    }

    let mut event = key::Event::new().ok()?;
    event
        .set_action(key::Action::Press)
        .set_key(key)
        .set_mods(mods)
        .set_consumed_mods(consumed_mods)
        .set_unshifted_codepoint(unshifted_codepoint)
        .set_utf8(text);

    let mut bytes = Vec::new();
    encoder
        .set_options_from_terminal(terminal)
        .encode_to_vec(&event, &mut bytes)
        .ok()?;

    if bytes.is_empty() {
        terminal_fallback_text_bytes(input)
    } else {
        Some(bytes)
    }
}

fn apply_key_terminal_mode(
    terminal: &mut Terminal<'_, '_>,
    mode: TerminalModeSnapshot,
) -> Result<(), libghostty_vt::Error> {
    terminal.set_mode(Mode::DECCKM, mode.app_cursor)?;
    terminal.set_mode(Mode::KEYPAD_KEYS, mode.app_keypad)?;
    Ok(())
}

fn terminal_compat_key_input_bytes(input: &TerminalKeyInput) -> Option<Vec<u8>> {
    match input.key.as_str() {
        "left"
            if !input.modifiers.shift
                && ((input.modifiers.alt && !input.modifiers.control)
                    || (input.modifiers.control && !input.modifiers.alt)) =>
        {
            Some(b"\x1bb".to_vec())
        }
        "right"
            if !input.modifiers.shift
                && ((input.modifiers.alt && !input.modifiers.control)
                    || (input.modifiers.control && !input.modifiers.alt)) =>
        {
            Some(b"\x1bf".to_vec())
        }
        "backspace" if input.modifiers.alt && !input.modifiers.control => Some(vec![0x1b, 0x7f]),
        "delete" if input.modifiers.control && !input.modifiers.alt => Some(b"\x1bd".to_vec()),
        _ => None,
    }
}

fn terminal_fallback_text_bytes(input: &TerminalKeyInput) -> Option<Vec<u8>> {
    let text = terminal_key_text(input)?;
    let mut bytes = Vec::with_capacity(text.len() + usize::from(input.modifiers.alt));
    if input.modifiers.alt {
        bytes.push(0x1b);
    }
    bytes.extend_from_slice(text.as_bytes());
    Some(bytes)
}

fn terminal_key_text(input: &TerminalKeyInput) -> Option<&str> {
    input.key_char.as_deref().or(match input.key.as_str() {
        "space" => Some(" "),
        key if key.chars().count() == 1 => Some(key),
        _ => None,
    })
}

fn terminal_key_mods(modifiers: TerminalInputModifiers) -> key::Mods {
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

fn terminal_key_spec(input: &TerminalKeyInput) -> Option<(key::Key, char)> {
    let key = input.key.as_str();
    match key {
        "enter" => Some((key::Key::Enter, '\0')),
        "tab" => Some((key::Key::Tab, '\0')),
        "backspace" => Some((key::Key::Backspace, '\0')),
        "escape" => Some((key::Key::Escape, '\0')),
        "up" => Some((key::Key::ArrowUp, '\0')),
        "down" => Some((key::Key::ArrowDown, '\0')),
        "left" => Some((key::Key::ArrowLeft, '\0')),
        "right" => Some((key::Key::ArrowRight, '\0')),
        "home" => Some((key::Key::Home, '\0')),
        "end" => Some((key::Key::End, '\0')),
        "pageup" => Some((key::Key::PageUp, '\0')),
        "pagedown" => Some((key::Key::PageDown, '\0')),
        "delete" => Some((key::Key::Delete, '\0')),
        "insert" => Some((key::Key::Insert, '\0')),
        "space" => Some((key::Key::Space, ' ')),
        "f1" => Some((key::Key::F1, '\0')),
        "f2" => Some((key::Key::F2, '\0')),
        "f3" => Some((key::Key::F3, '\0')),
        "f4" => Some((key::Key::F4, '\0')),
        "f5" => Some((key::Key::F5, '\0')),
        "f6" => Some((key::Key::F6, '\0')),
        "f7" => Some((key::Key::F7, '\0')),
        "f8" => Some((key::Key::F8, '\0')),
        "f9" => Some((key::Key::F9, '\0')),
        "f10" => Some((key::Key::F10, '\0')),
        "f11" => Some((key::Key::F11, '\0')),
        "f12" => Some((key::Key::F12, '\0')),
        _ => terminal_character_key_spec(key),
    }
}

fn terminal_character_key_spec(key: &str) -> Option<(key::Key, char)> {
    if key.chars().count() != 1 {
        return None;
    }

    let ch = key.chars().next()?;
    let spec = match ch {
        'a' => key::Key::A,
        'b' => key::Key::B,
        'c' => key::Key::C,
        'd' => key::Key::D,
        'e' => key::Key::E,
        'f' => key::Key::F,
        'g' => key::Key::G,
        'h' => key::Key::H,
        'i' => key::Key::I,
        'j' => key::Key::J,
        'k' => key::Key::K,
        'l' => key::Key::L,
        'm' => key::Key::M,
        'n' => key::Key::N,
        'o' => key::Key::O,
        'p' => key::Key::P,
        'q' => key::Key::Q,
        'r' => key::Key::R,
        's' => key::Key::S,
        't' => key::Key::T,
        'u' => key::Key::U,
        'v' => key::Key::V,
        'w' => key::Key::W,
        'x' => key::Key::X,
        'y' => key::Key::Y,
        'z' => key::Key::Z,
        '0' => key::Key::Digit0,
        '1' => key::Key::Digit1,
        '2' => key::Key::Digit2,
        '3' => key::Key::Digit3,
        '4' => key::Key::Digit4,
        '5' => key::Key::Digit5,
        '6' => key::Key::Digit6,
        '7' => key::Key::Digit7,
        '8' => key::Key::Digit8,
        '9' => key::Key::Digit9,
        '-' => key::Key::Minus,
        '=' => key::Key::Equal,
        '[' => key::Key::BracketLeft,
        ']' => key::Key::BracketRight,
        '\\' => key::Key::Backslash,
        ';' => key::Key::Semicolon,
        '\'' => key::Key::Quote,
        ',' => key::Key::Comma,
        '.' => key::Key::Period,
        '/' => key::Key::Slash,
        '`' => key::Key::Backquote,
        _ => return None,
    };

    Some((spec, ch))
}
