use gpui::{Pixels, TextStyle, relative};

use crate::app::theme::{
    HunkAccentTone, hunk_blend, hunk_input_surface, hunk_text_selection_background,
    hunk_tinted_button, hunk_toolbar_brand_chip, hunk_toolbar_chip,
};

impl DiffViewer {
    fn render_file_editor_surface(
        &mut self,
        window: &mut Window,
        editor_font_size: Pixels,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.files_editor.borrow_mut().sync_theme(is_dark);
        let view = cx.entity();
        let (editor_status, search_match_count, show_whitespace, soft_wrap_enabled) = {
            let files_editor = self.files_editor.borrow();
            (
                files_editor.status_snapshot(),
                files_editor.search_match_count(),
                files_editor.show_whitespace(),
                files_editor.soft_wrap_enabled(),
            )
        };
        let is_editor_focused = self.files_editor_focus_handle.is_focused(window);
        let text_style = TextStyle {
            color: cx.theme().foreground,
            font_family: cx.theme().mono_font_family.clone(),
            font_size: editor_font_size.into(),
            line_height: relative(1.45),
            ..Default::default()
        };
        let editor_element = crate::app::native_files_editor::FilesEditorElement::new(
            self.files_editor.clone(),
            is_editor_focused,
            text_style,
            crate::app::native_files_editor::FilesEditorPalette {
                background: cx.theme().background,
                active_line_background: hunk_blend(
                    cx.theme().background,
                    cx.theme().primary,
                    is_dark,
                    0.08,
                    0.04,
                ),
                line_number: cx.theme().muted_foreground,
                current_line_number: cx.theme().foreground,
                border: hunk_opacity(cx.theme().border, is_dark, 0.92, 0.78),
                default_foreground: cx.theme().foreground,
                muted_foreground: cx.theme().muted_foreground,
                selection_background: hunk_text_selection_background(cx.theme(), is_dark),
                cursor: cx.theme().primary,
                invisible: cx
                    .theme()
                    .highlight_theme
                    .style
                    .editor_invisible
                    .unwrap_or(cx.theme().muted_foreground),
                indent_guide: hunk_opacity(cx.theme().border, is_dark, 0.54, 0.46),
                fold_marker: cx.theme().muted_foreground,
                current_scope: hunk_opacity(cx.theme().accent, is_dark, 0.42, 0.28),
                bracket_match: hunk_opacity(cx.theme().accent, is_dark, 0.24, 0.16),
                diagnostic_error: cx.theme().danger,
                diagnostic_warning: cx.theme().warning,
                diagnostic_info: cx.theme().accent,
                diff_addition: cx.theme().success,
                diff_deletion: cx.theme().danger,
                diff_modification: cx.theme().warning,
            },
        );
        let shell_border = if is_editor_focused {
            hunk_opacity(cx.theme().accent, is_dark, 0.74, 0.46)
        } else {
            hunk_opacity(cx.theme().border, is_dark, 0.92, 0.78)
        };
        let header_mode = editor_status
            .as_ref()
            .map(|status| status.mode)
            .unwrap_or("READY");
        let header_language = editor_status
            .as_ref()
            .map(|status| status.language.clone())
            .unwrap_or_else(|| "text".to_string());
        let footer_selection = editor_status
            .as_ref()
            .map(|status| status.selection.clone())
            .unwrap_or_else(|| "0 cursors".to_string());
        let footer_position = editor_status
            .as_ref()
            .map(|status| status.position.clone())
            .unwrap_or_else(|| "Ln 1  Col 1".to_string());
        let mode_tone = match header_mode {
            "INSERT" => HunkAccentTone::Success,
            "SELECT" => HunkAccentTone::Warning,
            _ => HunkAccentTone::Accent,
        };
        let brand = hunk_toolbar_brand_chip(cx.theme(), is_dark);
        let mode_chip = hunk_tinted_button(cx.theme(), is_dark, mode_tone);
        let language_chip = hunk_toolbar_chip(cx.theme(), is_dark);
        let search_surface = hunk_input_surface(cx.theme(), is_dark);
        let search_count_label = if self.editor_search_visible {
            match search_match_count {
                0 => "No matches".to_string(),
                1 => "1 match".to_string(),
                count => format!("{count} matches"),
            }
        } else {
            String::new()
        };

        div()
            .flex_1()
            .min_h_0()
            .p_2()
            .key_context("FilesEditor")
            .track_focus(&self.files_editor_focus_handle)
            .on_action(cx.listener(Self::files_editor_copy_action))
            .on_action(cx.listener(Self::files_editor_cut_action))
            .on_action(cx.listener(Self::files_editor_paste_action))
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.files_editor_focus_handle.focus(window, cx);
                    });
                }
            })
            .on_key_down({
                let view = view.clone();
                move |event, window, cx| {
                    let handled = view.update(cx, |this, cx| {
                        let uses_primary_shortcut = if cfg!(target_os = "macos") {
                            event.keystroke.modifiers.platform
                        } else {
                            event.keystroke.modifiers.control
                        };
                        if uses_primary_shortcut
                            && !event.keystroke.modifiers.shift
                            && event.keystroke.key == "f"
                        {
                            this.toggle_editor_search(true, window, cx);
                            return true;
                        }

                        if this.editor_markdown_preview
                            || !this.files_editor_focus_handle.is_focused(window)
                            || is_desktop_clipboard_shortcut(&event.keystroke)
                        {
                            return false;
                        }

                        if this
                            .files_editor
                            .borrow_mut()
                            .handle_keystroke(&event.keystroke)
                        {
                            this.sync_editor_dirty_from_input(cx);
                            cx.notify();
                            return true;
                        }
                        false
                    });
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            .on_scroll_wheel({
                let view = view.clone();
                move |event, _, cx| {
                    let handled = view.update(cx, |this, cx| {
                        let line_height = (editor_font_size * 1.45).max(px(14.0));
                        if let Some((direction, line_count)) =
                            crate::app::native_files_editor::scroll_direction_and_count(
                                event,
                                line_height,
                            )
                        {
                            this.files_editor
                                .borrow_mut()
                                .scroll_lines(line_count, direction);
                            this.sync_editor_dirty_from_input(cx);
                            cx.notify();
                            return true;
                        }
                        false
                    });
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            .child(
                v_flex()
                    .h_full()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(shell_border)
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .px_2()
                            .py_1p5()
                            .border_b_1()
                            .border_color(shell_border)
                            .bg(hunk_opacity(
                                cx.theme().secondary_active,
                                is_dark,
                                0.34,
                                0.54,
                            ))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_full()
                                            .bg(brand.background)
                                            .border_1()
                                            .border_color(brand.border)
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child("FILES"),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_full()
                                            .bg(mode_chip.background)
                                            .border_1()
                                            .border_color(mode_chip.border)
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(mode_chip.text)
                                            .child(header_mode),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_full()
                                            .bg(language_chip.background)
                                            .border_1()
                                            .border_color(language_chip.border)
                                            .text_xs()
                                            .text_color(cx.theme().foreground)
                                            .child(header_language),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child("Files editor"),
                                    ),
                            )
                            .child(h_flex().items_center().gap_1().child({
                                let view = view.clone();
                                let mut button = Button::new("editor-search-toggle")
                                    .compact()
                                    .rounded(px(7.0))
                                    .label("Search")
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.toggle_editor_search_visibility(window, cx);
                                        });
                                    });
                                if self.editor_search_visible {
                                    button = button.primary();
                                } else {
                                    button = button
                                        .outline()
                                        .bg(hunk_opacity(
                                            cx.theme().secondary,
                                            is_dark,
                                            0.46,
                                            0.68,
                                        ))
                                        .border_color(hunk_opacity(
                                            cx.theme().border,
                                            is_dark,
                                            0.86,
                                            0.70,
                                        ));
                                }
                                button
                            }).child({
                                let view = view.clone();
                                let mut button = Button::new("editor-wrap-toggle")
                                    .compact()
                                    .rounded(px(7.0))
                                    .label("Wrap")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            if this.files_editor.borrow_mut().toggle_soft_wrap() {
                                                cx.notify();
                                            }
                                        });
                                    });
                                if soft_wrap_enabled {
                                    button = button.primary();
                                } else {
                                    button = button
                                        .outline()
                                        .bg(hunk_opacity(
                                            cx.theme().secondary,
                                            is_dark,
                                            0.46,
                                            0.68,
                                        ))
                                        .border_color(hunk_opacity(
                                            cx.theme().border,
                                            is_dark,
                                            0.86,
                                            0.70,
                                        ));
                                }
                                button
                            }).child({
                                let view = view.clone();
                                let mut button = Button::new("editor-whitespace-toggle")
                                    .compact()
                                    .rounded(px(7.0))
                                    .label("Invisibles")
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            if this.files_editor.borrow_mut().toggle_show_whitespace()
                                            {
                                                cx.notify();
                                            }
                                        });
                                    });
                                if show_whitespace {
                                    button = button.primary();
                                } else {
                                    button = button
                                        .outline()
                                        .bg(hunk_opacity(
                                            cx.theme().secondary,
                                            is_dark,
                                            0.46,
                                            0.68,
                                        ))
                                        .border_color(hunk_opacity(
                                            cx.theme().border,
                                            is_dark,
                                            0.86,
                                            0.70,
                                        ));
                                }
                                button
                            }).child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(if is_editor_focused {
                                        cx.theme().accent
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(if is_editor_focused {
                                        "Focused"
                                    } else {
                                        "Inactive"
                                    }),
                            )),
                    )
                    .when(self.editor_search_visible, |this| {
                        this.child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_2()
                                .px_2()
                                .py_1p5()
                                .border_b_1()
                                .border_color(shell_border)
                                .bg(hunk_opacity(
                                    cx.theme().secondary,
                                    is_dark,
                                    0.34,
                                    0.44,
                                ))
                                .child(
                                    Input::new(&self.editor_search_input_state)
                                        .flex_1()
                                        .h(px(34.0))
                                        .rounded(px(8.0))
                                        .border_1()
                                        .border_color(search_surface.border)
                                        .bg(search_surface.background),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(search_count_label.clone()),
                                )
                                .child({
                                    let view = view.clone();
                                    Button::new("editor-search-prev")
                                        .outline()
                                        .compact()
                                        .rounded(px(7.0))
                                        .label("Prev")
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.navigate_editor_search(false, cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("editor-search-next")
                                        .outline()
                                        .compact()
                                        .rounded(px(7.0))
                                        .label("Next")
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.navigate_editor_search(true, cx);
                                            });
                                        })
                                })
                                .child({
                                    let view = view.clone();
                                    Button::new("editor-search-close")
                                        .outline()
                                        .compact()
                                        .rounded(px(7.0))
                                        .label("Done")
                                        .on_click(move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.toggle_editor_search(false, window, cx);
                                            });
                                        })
                                }),
                        )
                    })
                    .child(div().flex_1().min_h_0().child(editor_element))
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .px_2()
                            .py_1()
                            .border_t_1()
                            .border_color(shell_border)
                            .bg(hunk_opacity(
                                cx.theme().secondary_active,
                                is_dark,
                                0.24,
                                0.42,
                            ))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Selection"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().foreground)
                                            .child(footer_selection),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(cx.theme().foreground)
                                    .child(footer_position),
                            ),
                    ),
            )
            .into_any_element()
    }
}
