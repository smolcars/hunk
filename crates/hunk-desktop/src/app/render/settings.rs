impl DiffViewer {
    fn render_settings_popup(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(settings) = self.settings_draft.as_ref() else {
            return div().into_any_element();
        };

        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let backdrop_bg = hunk_modal_backdrop(cx.theme(), is_dark);
        let modal_surface = hunk_modal_surface(cx.theme(), is_dark);
        let nav_surface = hunk_nav_surface(cx.theme(), is_dark);

        div()
            .id("settings-popup-overlay")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .bg(backdrop_bg)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .id("settings-popup-anchor")
                    .size_full()
                    .p_4()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_scroll_wheel(|_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        v_flex()
                            .id("settings-popup")
                            .w_full()
                            .h_full()
                            .max_w(px(860.0))
                            .max_h(px(620.0))
                            .rounded(px(12.0))
                            .border_1()
                            .border_color(modal_surface.border)
                            .bg(modal_surface.background)
                            .child(
                                h_flex()
                                    .items_center()
                                    .justify_between()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(
                                        cx.theme().border.opacity(if is_dark { 0.92 } else { 0.74 }),
                                    )
                                    .child(
                                        v_flex()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .font_semibold()
                                                    .text_color(cx.theme().foreground)
                                                    .child("Settings"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("Changes are saved to ~/.hunkdiff/config.toml"),
                                            ),
                                    )
                                    .child({
                                        let view = view.clone();
                                        Button::new("settings-close")
                                            .ghost()
                                            .compact()
                                            .rounded(px(8.0))
                                            .label("Close")
                                            .on_click(move |_, window, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.close_settings_and_refocus(window, cx);
                                                });
                                            })
                                    }),
                            )
                            .child(
                                h_flex()
                                    .flex_1()
                                    .min_h_0()
                                    .items_start()
                                    .child(
                                        v_flex()
                                            .w(px(220.0))
                                            .h_full()
                                            .justify_start()
                                            .p_3()
                                            .gap_2()
                                            .border_r_1()
                                            .border_color(nav_surface.border)
                                            .bg(nav_surface.background)
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_semibold()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("Categories"),
                                            )
                                            .children(SettingsCategory::ALL.into_iter().map(
                                                |category| {
                                                    let is_selected = settings.category == category;
                                                    let view = view.clone();
                                                    let label = category.title();
                                                    let id = match category {
                                                        SettingsCategory::Ui => "settings-nav-ui",
                                                        SettingsCategory::KeyboardShortcuts => {
                                                            "settings-nav-keyboard-shortcuts"
                                                        }
                                                    };
                                                    let button_colors = hunk_settings_nav_button(
                                                        cx.theme(),
                                                        is_dark,
                                                        is_selected,
                                                    );

                                                    Button::new(id)
                                                        .outline()
                                                        .rounded(px(8.0))
                                                        .label(label)
                                                        .bg(button_colors.background)
                                                        .border_color(button_colors.border)
                                                        .text_color(button_colors.text)
                                                        .on_click(move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.select_settings_category(
                                                                    category, cx,
                                                                );
                                                            });
                                                        })
                                                        .into_any_element()
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("settings-scroll-content")
                                            .flex_1()
                                            .h_full()
                                            .min_w_0()
                                            .min_h_0()
                                            .p_4()
                                            .overflow_y_scroll()
                                            .occlude()
                                            .child(match settings.category {
                                                SettingsCategory::Ui => {
                                                    self.render_settings_ui_category(settings, cx)
                                                }
                                                SettingsCategory::KeyboardShortcuts => {
                                                    self.render_settings_shortcuts_category(
                                                        settings, cx,
                                                    )
                                                }
                                            }),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_3()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(
                                        cx.theme().border.opacity(if is_dark { 0.92 } else { 0.74 }),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(if settings.error_message.is_some() {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().muted_foreground
                                            })
                                            .child(
                                                settings.error_message.clone().unwrap_or_else(|| {
                                                    "Shortcut updates are saved to config.toml."
                                                        .to_string()
                                                }),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child({
                                                let view = view.clone();
                                                Button::new("settings-cancel")
                                                    .outline()
                                                    .rounded(px(8.0))
                                                    .label("Cancel")
                                                    .on_click(move |_, window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.close_settings_and_refocus(
                                                                window, cx,
                                                            );
                                                        });
                                                    })
                                            })
                                            .child({
                                                let view = view.clone();
                                                Button::new("settings-save")
                                                    .primary()
                                                    .rounded(px(8.0))
                                                    .label("Save")
                                                    .on_click(move |_, window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.save_settings(window, cx);
                                                        });
                                                    })
                                            }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_ui_category(
        &self,
        settings: &SettingsDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let card_surface = hunk_card_surface(cx.theme(), is_dark);
        let dropdown_bg = hunk_dropdown_fill(cx.theme(), is_dark);
        let theme_label = match settings.theme {
            ThemePreference::System => "System",
            ThemePreference::Light => "Light",
            ThemePreference::Dark => "Dark",
        };
        let whitespace_label = if settings.show_whitespace { "On" } else { "Off" };
        let eol_label = if settings.show_eol_markers { "On" } else { "Off" };
        let reduced_motion_label = if settings.reduce_motion { "On" } else { "Off" };
        let show_fps_counter_label = if settings.show_fps_counter { "On" } else { "Off" };
        v_flex()
            .w_full()
            .gap_3()
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("UI"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Theme and UI visibility preferences."),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .gap_3()
                    .p_3()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(card_surface.border)
                    .bg(card_surface.background)
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Theme"),
                            )
                            .child({
                                let view = view.clone();
                                let selected_theme = settings.theme;
                                Button::new("settings-theme-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(theme_label)
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("System")
                                                .checked(selected_theme == ThemePreference::System)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_theme(
                                                                ThemePreference::System,
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Light")
                                                .checked(selected_theme == ThemePreference::Light)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_theme(
                                                                ThemePreference::Light,
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Dark")
                                                .checked(selected_theme == ThemePreference::Dark)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_theme(
                                                                ThemePreference::Dark,
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                    })
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Whitespace Markers"),
                            )
                            .child({
                                let view = view.clone();
                                let show_whitespace = settings.show_whitespace;
                                Button::new("settings-whitespace-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(whitespace_label)
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("On")
                                                .checked(show_whitespace)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_show_whitespace(true, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Off")
                                                .checked(!show_whitespace)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_show_whitespace(false, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                    })
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("End-Of-Line Markers"),
                            )
                            .child({
                                let view = view.clone();
                                let show_eol_markers = settings.show_eol_markers;
                                Button::new("settings-eol-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(eol_label)
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("On")
                                                .checked(show_eol_markers)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_show_eol_markers(true, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Off")
                                                .checked(!show_eol_markers)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_show_eol_markers(false, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                    })
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Reduced Motion"),
                            )
                            .child({
                                let view = view.clone();
                                let reduce_motion = settings.reduce_motion;
                                Button::new("settings-reduced-motion-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(reduced_motion_label)
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("On")
                                                .checked(reduce_motion)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_reduce_motion(true, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Off")
                                                .checked(!reduce_motion)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_reduce_motion(false, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                    })
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("FPS Counter"),
                            )
                            .child({
                                let view = view.clone();
                                let show_fps_counter = settings.show_fps_counter;
                                Button::new("settings-fps-counter-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(show_fps_counter_label)
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("On")
                                                .checked(show_fps_counter)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_show_fps_counter(
                                                                true, cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Off")
                                                .checked(!show_fps_counter)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_show_fps_counter(
                                                                false, cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                    })
                            }),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child("Update behavior"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "Diffs refresh immediately on file events. The app also performs \
                            a background periodic check as a fallback if file events are missed. \
                            Reduced Motion disables animated transitions in the Git workspace.",
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_shortcuts_category(
        &self,
        settings: &SettingsDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();

        v_flex()
            .w_full()
            .gap_3()
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Keyboard Shortcuts"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Edit comma-separated shortcut strings for each action."),
                    ),
            )
            .children(
                settings
                    .shortcuts
                    .rows()
                    .into_iter()
                    .map(|row| self.render_settings_shortcut_row(row, cx)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(if is_dark { 0.94 } else { 1.0 }))
                    .child(
                        "Use commas to add alternatives. For comma key, use cmd-, literally.",
                    ),
            )
            .into_any_element()
    }

    fn render_settings_shortcut_row(
        &self,
        row: SettingsShortcutRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_dark = cx.theme().mode.is_dark();
        let card_surface = hunk_card_surface(cx.theme(), is_dark);
        let input_surface = hunk_input_surface(cx.theme(), is_dark);

        v_flex()
            .id(row.id)
            .w_full()
            .gap_1()
            .p_3()
            .rounded(px(10.0))
            .border_1()
            .border_color(card_surface.border)
            .bg(card_surface.background)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(row.label),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(row.hint),
            )
            .child(
                Input::new(&row.input_state)
                    .h(px(36.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(input_surface.border)
                    .bg(input_surface.background)
                    .disabled(false),
            )
            .into_any_element()
    }
}
