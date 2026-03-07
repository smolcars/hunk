use std::rc::Rc;

use super::{preferred_mono_font_family, preferred_ui_font_family};
use gpui::{App, Hsla};
use gpui_component::{Colorize as _, Theme, ThemeMode, highlighter::HighlightThemeStyle};
use hunk_git::git::FileStatus;

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkSurfaceColors {
    pub background: Hsla,
    pub border: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkButtonColors {
    pub background: Hsla,
    pub border: Hsla,
    pub text: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HunkAccentTone {
    Accent,
    Success,
    Warning,
    Neutral,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkDisclosureColors {
    pub title: Hsla,
    pub summary: Hsla,
    pub hover_background: Hsla,
    pub chevron: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkFileStatusBannerColors {
    pub label: &'static str,
    pub row_background: Hsla,
    pub border: Hsla,
    pub badge_background: Hsla,
    pub badge_border: Hsla,
    pub accent_strip: Hsla,
    pub arrow: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkLineStatsColors {
    pub added: Hsla,
    pub removed: Hsla,
    pub changed: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkGitWorkspaceColors {
    pub shell: HunkSurfaceColors,
    pub hero: HunkSurfaceColors,
    pub rail: HunkSurfaceColors,
    pub card: HunkSurfaceColors,
    pub muted_card: HunkSurfaceColors,
}

pub(crate) fn install_hunk_themes(cx: &mut App) {
    apply_soft_light_theme(cx);
    apply_soft_dark_theme(cx);
}

pub(crate) fn hunk_modal_backdrop(theme: &Theme, is_dark: bool) -> Hsla {
    hunk_opacity(theme.background, is_dark, 0.24, 0.12)
}

pub(crate) fn hunk_modal_surface(theme: &Theme, is_dark: bool) -> HunkSurfaceColors {
    HunkSurfaceColors {
        background: hunk_blend(theme.popover, theme.background, is_dark, 0.16, 0.05),
        border: hunk_opacity(theme.border, is_dark, 0.92, 0.72),
    }
}

pub(crate) fn hunk_nav_surface(theme: &Theme, is_dark: bool) -> HunkSurfaceColors {
    HunkSurfaceColors {
        background: hunk_blend(theme.sidebar, theme.muted, is_dark, 0.24, 0.16),
        border: hunk_opacity(theme.border, is_dark, 0.90, 0.70),
    }
}

pub(crate) fn hunk_card_surface(theme: &Theme, is_dark: bool) -> HunkSurfaceColors {
    HunkSurfaceColors {
        background: hunk_blend(theme.background, theme.muted, is_dark, 0.24, 0.12),
        border: hunk_opacity(theme.border, is_dark, 0.90, 0.72),
    }
}

pub(crate) fn hunk_input_surface(theme: &Theme, is_dark: bool) -> HunkSurfaceColors {
    HunkSurfaceColors {
        background: hunk_blend(theme.background, theme.muted, is_dark, 0.20, 0.09),
        border: hunk_opacity(theme.border, is_dark, 0.90, 0.72),
    }
}

pub(crate) fn hunk_git_workspace(theme: &Theme, is_dark: bool) -> HunkGitWorkspaceColors {
    HunkGitWorkspaceColors {
        shell: HunkSurfaceColors {
            background: hunk_blend(theme.sidebar, theme.muted, is_dark, 0.18, 0.26),
            border: hunk_opacity(theme.border, is_dark, 0.92, 0.74),
        },
        hero: HunkSurfaceColors {
            background: hunk_blend(theme.background, theme.accent, is_dark, 0.14, 0.08),
            border: hunk_opacity(theme.accent, is_dark, 0.46, 0.34),
        },
        rail: HunkSurfaceColors {
            background: hunk_blend(theme.popover, theme.muted, is_dark, 0.22, 0.12),
            border: hunk_opacity(theme.border, is_dark, 0.90, 0.72),
        },
        card: hunk_card_surface(theme, is_dark),
        muted_card: HunkSurfaceColors {
            background: hunk_blend(theme.background, theme.muted, is_dark, 0.16, 0.20),
            border: hunk_opacity(theme.border, is_dark, 0.88, 0.70),
        },
    }
}

pub(crate) fn hunk_tinted_button(
    theme: &Theme,
    is_dark: bool,
    tone: HunkAccentTone,
) -> HunkButtonColors {
    let accent = match tone {
        HunkAccentTone::Accent => theme.accent,
        HunkAccentTone::Success => theme.success,
        HunkAccentTone::Warning => theme.warning,
        HunkAccentTone::Neutral => theme.secondary,
    };

    HunkButtonColors {
        background: hunk_opacity(accent, is_dark, 0.18, 0.12),
        border: hunk_opacity(accent, is_dark, 0.54, 0.34),
        text: if matches!(tone, HunkAccentTone::Neutral) {
            theme.foreground
        } else {
            hunk_tone(accent, is_dark, 0.42, 0.28)
        },
    }
}

pub(crate) fn hunk_dropdown_fill(theme: &Theme, is_dark: bool) -> Hsla {
    hunk_opacity(theme.secondary, is_dark, 0.52, 0.70)
}

pub(crate) fn hunk_settings_nav_button(
    theme: &Theme,
    is_dark: bool,
    is_selected: bool,
) -> HunkButtonColors {
    if is_selected {
        HunkButtonColors {
            background: theme.secondary_active,
            border: hunk_opacity(theme.accent, is_dark, 0.84, 0.64),
            text: theme.foreground,
        }
    } else {
        HunkButtonColors {
            background: theme.background.opacity(0.0),
            border: hunk_opacity(theme.border, is_dark, 0.86, 0.68),
            text: hunk_opacity(theme.muted_foreground, is_dark, 0.94, 0.98),
        }
    }
}

pub(crate) fn hunk_toolbar_chip(theme: &Theme, is_dark: bool) -> HunkSurfaceColors {
    HunkSurfaceColors {
        background: hunk_opacity(theme.muted, is_dark, 0.26, 0.52),
        border: hunk_opacity(theme.border, is_dark, 0.88, 0.70),
    }
}

pub(crate) fn hunk_toolbar_brand_chip(theme: &Theme, is_dark: bool) -> HunkSurfaceColors {
    HunkSurfaceColors {
        background: hunk_opacity(theme.accent, is_dark, 0.26, 0.14),
        border: hunk_opacity(theme.accent, is_dark, 0.62, 0.42),
    }
}

pub(crate) fn hunk_disclosure_row(theme: &Theme, is_dark: bool) -> HunkDisclosureColors {
    let title = hunk_opacity(theme.foreground, is_dark, 0.78, 0.74);
    HunkDisclosureColors {
        title,
        summary: theme.muted_foreground,
        hover_background: hunk_opacity(theme.muted, is_dark, 0.18, 0.24),
        chevron: hunk_opacity(theme.muted_foreground, is_dark, 0.94, 0.88),
    }
}

pub(crate) fn hunk_text_selection_background(theme: &Theme, is_dark: bool) -> Hsla {
    hunk_opacity(theme.primary, is_dark, 0.26, 0.18)
}

pub(crate) fn hunk_file_status_banner(
    theme: &Theme,
    status: FileStatus,
    is_dark: bool,
    is_selected: bool,
) -> HunkFileStatusBannerColors {
    let (label, accent) = match status {
        FileStatus::Added | FileStatus::Untracked => ("NEW FILE", theme.success),
        FileStatus::Deleted => ("DELETED FILE", theme.danger),
        FileStatus::Renamed => ("RENAMED", theme.accent),
        FileStatus::Modified => ("MODIFIED", theme.warning),
        FileStatus::TypeChange => ("TYPE CHANGED", theme.warning),
        FileStatus::Conflicted => ("CONFLICTED", theme.danger),
        FileStatus::Unknown => ("MODIFIED", theme.muted_foreground),
    };
    let background = hunk_blend(theme.background, accent, is_dark, 0.34, 0.16);
    let row_background = if is_selected {
        hunk_blend(background, theme.primary, is_dark, 0.28, 0.16)
    } else {
        background
    };

    HunkFileStatusBannerColors {
        label,
        row_background,
        border: hunk_opacity(accent, is_dark, 0.78, 0.52),
        badge_background: hunk_opacity(accent, is_dark, 0.50, 0.27),
        badge_border: hunk_opacity(accent, is_dark, 0.88, 0.44),
        accent_strip: hunk_tone(accent, is_dark, 0.18, 0.06),
        arrow: hunk_tone(accent, is_dark, 0.34, 0.18),
    }
}

pub(crate) fn hunk_line_stats(theme: &Theme, is_dark: bool) -> HunkLineStatsColors {
    HunkLineStatsColors {
        added: hunk_tone(theme.success, is_dark, 0.42, 0.05),
        removed: hunk_tone(theme.danger, is_dark, 0.42, 0.05),
        changed: theme.muted_foreground,
    }
}

pub(crate) fn hunk_pick<T: Copy>(is_dark: bool, dark: T, light: T) -> T {
    if is_dark { dark } else { light }
}

pub(crate) fn hunk_opacity(color: Hsla, is_dark: bool, dark: f32, light: f32) -> Hsla {
    color.opacity(hunk_pick(is_dark, dark, light))
}

pub(crate) fn hunk_blend(base: Hsla, overlay: Hsla, is_dark: bool, dark: f32, light: f32) -> Hsla {
    base.blend(hunk_opacity(overlay, is_dark, dark, light))
}

pub(crate) fn hunk_tone(color: Hsla, is_dark: bool, dark_lighten: f32, light_darken: f32) -> Hsla {
    if is_dark {
        color.lighten(dark_lighten)
    } else {
        color.darken(light_darken)
    }
}

fn hsla_hex(hex: &str) -> Option<Hsla> {
    Hsla::parse_hex(hex).ok()
}

fn editor_highlight_style(
    base: Option<HighlightThemeStyle>,
    fallback: HighlightThemeStyle,
    mode: ThemeMode,
) -> HighlightThemeStyle {
    let mut style = base.unwrap_or(fallback);
    if mode.is_dark() {
        style.editor_background = hsla_hex("#20252f");
        style.editor_active_line = hsla_hex("#2a3140");
        style.editor_line_number = hsla_hex("#748094");
        style.editor_active_line_number = hsla_hex("#ced7e6");
    } else {
        style.editor_background = hsla_hex("#f4f6fa");
        style.editor_active_line = hsla_hex("#e7edf7");
        style.editor_line_number = hsla_hex("#8d97a8");
        style.editor_active_line_number = hsla_hex("#4a5363");
    }
    style
}

fn apply_soft_light_theme(cx: &mut App) {
    let mut light_theme = (*Theme::global(cx).light_theme).clone();
    let fallback_highlight = Theme::global(cx).highlight_theme.style.clone();

    light_theme.colors.accent = Some("#4f6ddf".into());
    light_theme.colors.accent_foreground = Some("#f8fbff".into());
    light_theme.colors.background = Some("#f5f6f8".into());
    light_theme.colors.list = Some("#f5f6f8".into());
    light_theme.colors.list_active = Some("#4f6ddf33".into());
    light_theme.colors.list_active_border = Some("#4f6ddf".into());
    light_theme.colors.list_hover = Some("#dce3ee".into());
    light_theme.colors.popover = Some("#f5f6f8".into());
    light_theme.colors.table = Some("#f5f6f8".into());
    light_theme.colors.sidebar = Some("#f5f6f8".into());
    light_theme.colors.title_bar = Some("#f5f6f8".into());
    light_theme.colors.list_even = Some("#f1f2f5".into());
    light_theme.colors.list_head = Some("#eef0f4".into());
    light_theme.colors.secondary = Some("#eceef3".into());
    light_theme.colors.secondary_hover = Some("#dde4ef".into());
    light_theme.colors.secondary_active = Some("#d3dbe8".into());
    light_theme.colors.muted = Some("#e9ecf2".into());
    light_theme.colors.muted_foreground = Some("#616977".into());
    light_theme.colors.border = Some("#d2d8e3".into());
    light_theme.font_family = Some(preferred_ui_font_family().into());
    light_theme.font_size = Some(14.0);
    light_theme.mono_font_family = Some(preferred_mono_font_family().into());
    light_theme.mono_font_size = Some(12.0);
    light_theme.radius = Some(8);
    light_theme.radius_lg = Some(10);
    light_theme.shadow = Some(false);
    light_theme.highlight = Some(editor_highlight_style(
        light_theme.highlight.clone(),
        fallback_highlight,
        ThemeMode::Light,
    ));

    Theme::global_mut(cx).light_theme = Rc::new(light_theme);

    if !Theme::global(cx).mode.is_dark() {
        Theme::change(ThemeMode::Light, None, cx);
    }
}

fn apply_soft_dark_theme(cx: &mut App) {
    let mut dark_theme = (*Theme::global(cx).dark_theme).clone();
    let fallback_highlight = Theme::global(cx).highlight_theme.style.clone();

    dark_theme.colors.accent = Some("#5f81eb".into());
    dark_theme.colors.accent_foreground = Some("#f8fbff".into());
    dark_theme.colors.background = Some("#1f2126".into());
    dark_theme.colors.list = Some("#1f2126".into());
    dark_theme.colors.list_active = Some("#5f81eb33".into());
    dark_theme.colors.list_active_border = Some("#7d9eff".into());
    dark_theme.colors.list_hover = Some("#343e4c".into());
    dark_theme.colors.popover = Some("#242831".into());
    dark_theme.colors.table = Some("#1f2126".into());
    dark_theme.colors.sidebar = Some("#1b1e24".into());
    dark_theme.colors.title_bar = Some("#1a1d22".into());
    dark_theme.colors.list_even = Some("#21242b".into());
    dark_theme.colors.list_head = Some("#292d36".into());
    dark_theme.colors.secondary = Some("#2a2f38".into());
    dark_theme.colors.secondary_hover = Some("#3b4554".into());
    dark_theme.colors.secondary_active = Some("#465163".into());
    dark_theme.colors.muted = Some("#272c35".into());
    dark_theme.colors.muted_foreground = Some("#a3adbb".into());
    dark_theme.colors.border = Some("#3d4554".into());
    dark_theme.font_family = Some(preferred_ui_font_family().into());
    dark_theme.font_size = Some(14.0);
    dark_theme.mono_font_family = Some(preferred_mono_font_family().into());
    dark_theme.mono_font_size = Some(12.0);
    dark_theme.radius = Some(8);
    dark_theme.radius_lg = Some(10);
    dark_theme.shadow = Some(false);
    dark_theme.highlight = Some(editor_highlight_style(
        dark_theme.highlight.clone(),
        fallback_highlight,
        ThemeMode::Dark,
    ));

    Theme::global_mut(cx).dark_theme = Rc::new(dark_theme);

    if Theme::global(cx).mode.is_dark() {
        Theme::change(ThemeMode::Dark, None, cx);
    }
}
