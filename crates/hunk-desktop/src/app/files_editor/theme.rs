use gpui::{Hsla, black, blue, green, hsla, red, rgb, white, yellow};
use helix_view::Theme;
use helix_view::graphics::Color;
use helix_view::theme;

const HUNK_HELIX_DARK_THEME: &str = "dark_plus";
const HUNK_HELIX_LIGHT_THEME: &str = "github_light";

pub(super) fn load_hunk_helix_theme(theme_loader: &theme::Loader, is_dark: bool) -> Theme {
    theme_loader
        .load(preferred_hunk_helix_theme_name(is_dark))
        .unwrap_or_else(|_| theme_loader.default_theme(is_dark))
}

fn preferred_hunk_helix_theme_name(is_dark: bool) -> &'static str {
    if is_dark {
        HUNK_HELIX_DARK_THEME
    } else {
        HUNK_HELIX_LIGHT_THEME
    }
}

pub(super) fn color_to_hsla(color: Color) -> Option<Hsla> {
    match color {
        Color::Reset => None,
        Color::Black => Some(black()),
        Color::Red => Some(red()),
        Color::Green => Some(green()),
        Color::Yellow => Some(yellow()),
        Color::Blue => Some(blue()),
        Color::Magenta => Some(hsla(0.82, 0.72, 0.68, 1.0)),
        Color::Cyan => Some(hsla(0.52, 0.70, 0.62, 1.0)),
        Color::Gray => Some(hsla(0.0, 0.0, 0.55, 1.0)),
        Color::LightRed => Some(hsla(0.0, 0.85, 0.68, 1.0)),
        Color::LightGreen => Some(hsla(0.34, 0.80, 0.62, 1.0)),
        Color::LightYellow => Some(hsla(0.15, 0.90, 0.67, 1.0)),
        Color::LightBlue => Some(hsla(0.60, 0.85, 0.70, 1.0)),
        Color::LightMagenta => Some(hsla(0.82, 0.80, 0.75, 1.0)),
        Color::LightCyan => Some(hsla(0.52, 0.82, 0.72, 1.0)),
        Color::LightGray => Some(hsla(0.0, 0.0, 0.78, 1.0)),
        Color::White => Some(white()),
        Color::Rgb(r, g, b) => {
            Some(rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)).into())
        }
        Color::Indexed(index) => indexed_color_to_hsla(index),
    }
}

fn indexed_color_to_hsla(index: u8) -> Option<Hsla> {
    let (r, g, b) = match index {
        0 => (0, 0, 0),
        1 => (205, 49, 49),
        2 => (13, 188, 121),
        3 => (229, 229, 16),
        4 => (36, 114, 200),
        5 => (188, 63, 188),
        6 => (17, 168, 205),
        7 => (229, 229, 229),
        8 => (102, 102, 102),
        9 => (241, 76, 76),
        10 => (35, 209, 139),
        11 => (245, 245, 67),
        12 => (59, 142, 234),
        13 => (214, 112, 214),
        14 => (41, 184, 219),
        15 => (255, 255, 255),
        16..=231 => {
            let cube = index - 16;
            let red = cube / 36;
            let green = (cube % 36) / 6;
            let blue = cube % 6;
            (
                xterm_cube_component(red),
                xterm_cube_component(green),
                xterm_cube_component(blue),
            )
        }
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            (gray, gray, gray)
        }
    };
    Some(rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)).into())
}

fn xterm_cube_component(value: u8) -> u8 {
    match value {
        0 => 0,
        1 => 95,
        2 => 135,
        3 => 175,
        4 => 215,
        _ => 255,
    }
}
