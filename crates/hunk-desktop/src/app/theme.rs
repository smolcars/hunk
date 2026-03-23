use std::rc::Rc;

use super::{preferred_mono_font_family, preferred_ui_font_family};
use gpui::{App, Hsla};
use gpui_component::{
    Colorize as _, Theme, ThemeMode,
    highlighter::{HighlightThemeStyle, SyntaxColors, ThemeStyle},
};
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
pub(crate) struct HunkCompletionMenuColors {
    pub panel: HunkSurfaceColors,
    pub row_hover: Hsla,
    pub row_selected: Hsla,
    pub row_selected_border: Hsla,
    pub primary_text: Hsla,
    pub secondary_text: Hsla,
    pub selected_secondary_text: Hsla,
    pub accent_text: Hsla,
    pub accent_soft_background: Hsla,
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
    pub rail: HunkSurfaceColors,
    pub card: HunkSurfaceColors,
    pub muted_card: HunkSurfaceColors,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkPendingMessageColors {
    pub text: Hsla,
    pub meta: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkDiffChromeColors {
    pub row_divider: Hsla,
    pub center_divider: Hsla,
    pub gutter_divider: Hsla,
    pub gutter_background: Hsla,
    pub empty_gutter_background: Hsla,
    pub column_header_background: Hsla,
    pub column_header_badge_background: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkEditorSyntaxColors {
    pub keyword: Hsla,
    pub string: Hsla,
    pub number: Hsla,
    pub comment: Hsla,
    pub function: Hsla,
    pub type_name: Hsla,
    pub constant: Hsla,
    pub variable: Hsla,
    pub operator: Hsla,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HunkEditorChromeColors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub active_line: Hsla,
    pub line_number: Hsla,
    pub active_line_number: Hsla,
    pub selection: Hsla,
    pub invisible: Hsla,
    pub indent_guide: Hsla,
    pub bracket_match: Hsla,
    pub current_scope: Hsla,
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

pub(crate) fn hunk_completion_menu(theme: &Theme, is_dark: bool) -> HunkCompletionMenuColors {
    let panel = HunkSurfaceColors {
        background: hunk_blend(theme.popover, theme.background, is_dark, 0.18, 0.03),
        border: hunk_opacity(theme.border, is_dark, 0.94, 0.76),
    };
    HunkCompletionMenuColors {
        panel,
        row_hover: hunk_blend(theme.accent, panel.background, is_dark, 0.10, 0.04),
        row_selected: hunk_blend(theme.accent, panel.background, is_dark, 0.20, 0.08),
        row_selected_border: hunk_opacity(theme.accent, is_dark, 0.78, 0.54),
        primary_text: hunk_opacity(theme.foreground, is_dark, 0.98, 0.92),
        secondary_text: hunk_opacity(theme.muted_foreground, is_dark, 0.88, 0.84),
        selected_secondary_text: hunk_blend(theme.foreground, theme.accent, is_dark, 0.34, 0.28),
        accent_text: hunk_blend(theme.accent, theme.foreground, is_dark, 0.90, 0.68),
        accent_soft_background: hunk_opacity(theme.accent, is_dark, 0.14, 0.08),
    }
}

pub(crate) fn hunk_pending_message(theme: &Theme, is_dark: bool) -> HunkPendingMessageColors {
    HunkPendingMessageColors {
        text: hunk_blend(
            theme.foreground,
            theme.muted_foreground,
            is_dark,
            0.52,
            0.74,
        ),
        meta: hunk_opacity(theme.muted_foreground, is_dark, 0.90, 0.94),
    }
}

pub(crate) fn hunk_git_workspace(theme: &Theme, is_dark: bool) -> HunkGitWorkspaceColors {
    HunkGitWorkspaceColors {
        shell: HunkSurfaceColors {
            background: hunk_blend(theme.sidebar, theme.muted, is_dark, 0.18, 0.26),
            border: hunk_opacity(theme.border, is_dark, 0.92, 0.74),
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

pub(crate) fn hunk_diff_chrome(theme: &Theme, is_dark: bool) -> HunkDiffChromeColors {
    HunkDiffChromeColors {
        row_divider: hunk_opacity(theme.border, is_dark, 0.74, 0.58),
        center_divider: hunk_opacity(theme.border, is_dark, 0.88, 0.72),
        gutter_divider: hunk_opacity(theme.border, is_dark, 0.66, 0.54),
        gutter_background: hunk_blend(theme.title_bar, theme.muted, is_dark, 0.24, 0.48),
        empty_gutter_background: hunk_blend(theme.sidebar, theme.muted, is_dark, 0.18, 0.38),
        column_header_background: hunk_blend(theme.title_bar, theme.muted, is_dark, 0.10, 0.22),
        column_header_badge_background: hunk_opacity(theme.muted, is_dark, 0.28, 0.42),
    }
}

pub(crate) fn hunk_editor_syntax_colors(_theme: &Theme, is_dark: bool) -> HunkEditorSyntaxColors {
    if is_dark {
        HunkEditorSyntaxColors {
            keyword: theme_hex("#569cd6"),
            string: theme_hex("#ce9178"),
            number: theme_hex("#b5cea8"),
            comment: theme_hex("#6a9955"),
            function: theme_hex("#dcdcaa"),
            type_name: theme_hex("#4ec9b0"),
            constant: theme_hex("#4fc1ff"),
            variable: theme_hex("#9cdcfe"),
            operator: theme_hex("#d4d4d4"),
        }
    } else {
        HunkEditorSyntaxColors {
            keyword: theme_hex("#0000ff"),
            string: theme_hex("#a31515"),
            number: theme_hex("#098658"),
            comment: theme_hex("#008000"),
            function: theme_hex("#795e26"),
            type_name: theme_hex("#267f99"),
            constant: theme_hex("#0070c1"),
            variable: theme_hex("#001080"),
            operator: theme_hex("#000000"),
        }
    }
}

pub(crate) fn hunk_editor_chrome_colors(theme: &Theme, is_dark: bool) -> HunkEditorChromeColors {
    let style = &theme.highlight_theme.style;
    if is_dark {
        HunkEditorChromeColors {
            background: style
                .editor_background
                .unwrap_or_else(|| theme_hex("#1e1e1e")),
            foreground: style
                .editor_foreground
                .unwrap_or_else(|| theme_hex("#d4d4d4")),
            active_line: style
                .editor_active_line
                .unwrap_or_else(|| theme_hex("#2a2d2e")),
            line_number: style
                .editor_line_number
                .unwrap_or_else(|| theme_hex("#858585")),
            active_line_number: style
                .editor_active_line_number
                .unwrap_or_else(|| theme_hex("#c6c6c6")),
            selection: theme_hex("#264f78"),
            invisible: style
                .editor_invisible
                .unwrap_or_else(|| theme_hex("#404040")),
            indent_guide: theme_hex("#404040"),
            bracket_match: theme_hex("#515c6a"),
            current_scope: theme_hex("#37373d"),
        }
    } else {
        HunkEditorChromeColors {
            background: style
                .editor_background
                .unwrap_or_else(|| theme_hex("#ffffff")),
            foreground: style
                .editor_foreground
                .unwrap_or_else(|| theme_hex("#000000")),
            active_line: style
                .editor_active_line
                .unwrap_or_else(|| theme_hex("#f3f3f3")),
            line_number: style
                .editor_line_number
                .unwrap_or_else(|| theme_hex("#237893")),
            active_line_number: style
                .editor_active_line_number
                .unwrap_or_else(|| theme_hex("#0b216f")),
            selection: theme_hex("#add6ff"),
            invisible: style
                .editor_invisible
                .unwrap_or_else(|| theme_hex("#d0d0d0")),
            indent_guide: theme_hex("#d8d8d8"),
            bracket_match: theme_hex("#c5c5c5"),
            current_scope: theme_hex("#eef6ff"),
        }
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

pub(crate) fn hunk_action_ready_button(
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
        background: hunk_blend(theme.background, accent, is_dark, 0.18, 0.08),
        border: hunk_opacity(accent, is_dark, 0.88, 0.62),
        text: if matches!(tone, HunkAccentTone::Neutral) {
            theme.foreground
        } else {
            hunk_tone(accent, is_dark, 0.22, 0.40)
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
    hunk_editor_chrome_colors(theme, is_dark).selection
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
    let background = hunk_blend(theme.title_bar, accent, is_dark, 0.20, 0.08);
    let row_background = if is_selected {
        hunk_blend(background, theme.primary, is_dark, 0.20, 0.12)
    } else {
        background
    };

    HunkFileStatusBannerColors {
        label,
        row_background,
        badge_background: hunk_opacity(accent, is_dark, 0.22, 0.14),
        badge_border: hunk_opacity(accent, is_dark, 0.52, 0.24),
        accent_strip: hunk_tone(accent, is_dark, 0.12, 0.04),
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

fn theme_hex(hex: &str) -> Hsla {
    hsla_hex(hex).expect("valid theme hex color")
}

fn syntax_style(hex: &str) -> ThemeStyle {
    serde_json::from_str(&format!(r#"{{"color":"{hex}"}}"#)).expect("valid syntax theme style")
}

fn syntax_style_json(json: &str) -> ThemeStyle {
    serde_json::from_str(json).expect("valid syntax theme style")
}

fn vscode_syntax_colors(mode: ThemeMode) -> SyntaxColors {
    if mode.is_dark() {
        SyntaxColors {
            attribute: Some(syntax_style("#c586c0")),
            boolean: Some(syntax_style("#569cd6")),
            comment: Some(syntax_style("#6a9955")),
            comment_doc: Some(syntax_style("#608b4e")),
            constant: Some(syntax_style("#4fc1ff")),
            constructor: Some(syntax_style("#dcdcaa")),
            emphasis: Some(syntax_style_json(r#"{"font_style":"italic"}"#)),
            emphasis_strong: Some(syntax_style_json(r#"{"font_weight":700}"#)),
            enum_: Some(syntax_style("#4ec9b0")),
            function: Some(syntax_style("#dcdcaa")),
            keyword: Some(syntax_style("#569cd6")),
            link_text: Some(syntax_style("#4fc1ff")),
            link_uri: Some(syntax_style("#3794ff")),
            number: Some(syntax_style("#b5cea8")),
            operator: Some(syntax_style("#d4d4d4")),
            preproc: Some(syntax_style("#c586c0")),
            property: Some(syntax_style("#9cdcfe")),
            punctuation: Some(syntax_style("#d4d4d4")),
            punctuation_bracket: Some(syntax_style("#d4d4d4")),
            punctuation_delimiter: Some(syntax_style("#d4d4d4")),
            punctuation_list_marker: Some(syntax_style("#d4d4d4")),
            string: Some(syntax_style("#ce9178")),
            string_escape: Some(syntax_style("#d7ba7d")),
            string_regex: Some(syntax_style("#d16969")),
            string_special: Some(syntax_style("#d7ba7d")),
            tag: Some(syntax_style("#569cd6")),
            text_literal: Some(syntax_style("#ce9178")),
            title: Some(syntax_style_json(
                r##"{"color":"#569cd6","font_weight":700}"##,
            )),
            type_: Some(syntax_style("#4ec9b0")),
            variable: Some(syntax_style("#9cdcfe")),
            variable_special: Some(syntax_style("#4fc1ff")),
            variant: Some(syntax_style("#4ec9b0")),
            ..SyntaxColors::default()
        }
    } else {
        SyntaxColors {
            attribute: Some(syntax_style("#af00db")),
            boolean: Some(syntax_style("#0000ff")),
            comment: Some(syntax_style("#008000")),
            comment_doc: Some(syntax_style("#008000")),
            constant: Some(syntax_style("#0070c1")),
            constructor: Some(syntax_style("#795e26")),
            emphasis: Some(syntax_style_json(r#"{"font_style":"italic"}"#)),
            emphasis_strong: Some(syntax_style_json(r#"{"font_weight":700}"#)),
            enum_: Some(syntax_style("#267f99")),
            function: Some(syntax_style("#795e26")),
            keyword: Some(syntax_style("#0000ff")),
            link_text: Some(syntax_style("#001080")),
            link_uri: Some(syntax_style("#0000ff")),
            number: Some(syntax_style("#098658")),
            operator: Some(syntax_style("#000000")),
            preproc: Some(syntax_style("#af00db")),
            property: Some(syntax_style("#001080")),
            punctuation: Some(syntax_style("#000000")),
            punctuation_bracket: Some(syntax_style("#000000")),
            punctuation_delimiter: Some(syntax_style("#000000")),
            punctuation_list_marker: Some(syntax_style("#000000")),
            string: Some(syntax_style("#a31515")),
            string_escape: Some(syntax_style("#ee0000")),
            string_regex: Some(syntax_style("#811f3f")),
            string_special: Some(syntax_style("#795e26")),
            tag: Some(syntax_style("#800000")),
            text_literal: Some(syntax_style("#a31515")),
            title: Some(syntax_style_json(
                r##"{"color":"#0000ff","font_weight":700}"##,
            )),
            type_: Some(syntax_style("#267f99")),
            variable: Some(syntax_style("#001080")),
            variable_special: Some(syntax_style("#0070c1")),
            variant: Some(syntax_style("#267f99")),
            ..SyntaxColors::default()
        }
    }
}

fn editor_highlight_style(
    base: Option<HighlightThemeStyle>,
    fallback: HighlightThemeStyle,
    mode: ThemeMode,
) -> HighlightThemeStyle {
    let mut style = base.unwrap_or(fallback);
    if mode.is_dark() {
        style.editor_background = hsla_hex("#1e1e1e");
        style.editor_foreground = hsla_hex("#d4d4d4");
        style.editor_active_line = hsla_hex("#2a2d2e");
        style.editor_line_number = hsla_hex("#858585");
        style.editor_active_line_number = hsla_hex("#c6c6c6");
        style.editor_invisible = hsla_hex("#404040");
    } else {
        style.editor_background = hsla_hex("#ffffff");
        style.editor_foreground = hsla_hex("#000000");
        style.editor_active_line = hsla_hex("#f3f3f3");
        style.editor_line_number = hsla_hex("#237893");
        style.editor_active_line_number = hsla_hex("#0b216f");
        style.editor_invisible = hsla_hex("#d0d0d0");
    }
    style.syntax = vscode_syntax_colors(mode);
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
    dark_theme.colors.background = Some("#1e1e1e".into());
    dark_theme.colors.list = Some("#1e1e1e".into());
    dark_theme.colors.list_active = Some("#5f81eb33".into());
    dark_theme.colors.list_active_border = Some("#7d9eff".into());
    dark_theme.colors.list_hover = Some("#2a2d2e".into());
    dark_theme.colors.popover = Some("#252526".into());
    dark_theme.colors.table = Some("#1e1e1e".into());
    dark_theme.colors.sidebar = Some("#252526".into());
    dark_theme.colors.title_bar = Some("#252526".into());
    dark_theme.colors.list_even = Some("#1e1e1e".into());
    dark_theme.colors.list_head = Some("#252526".into());
    dark_theme.colors.secondary = Some("#2d2d30".into());
    dark_theme.colors.secondary_hover = Some("#37373d".into());
    dark_theme.colors.secondary_active = Some("#3e3e42".into());
    dark_theme.colors.muted = Some("#2d2d30".into());
    dark_theme.colors.muted_foreground = Some("#969696".into());
    dark_theme.colors.border = Some("#3e3e42".into());
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
