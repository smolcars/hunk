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
                                    .border_color(hunk_opacity(cx.theme().border, is_dark, 0.92, 0.74))
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
                                                        SettingsCategory::Terminal => {
                                                            "settings-nav-terminal"
                                                        }
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
                                                SettingsCategory::Terminal => {
                                                    self.render_settings_terminal_category(
                                                        settings, cx,
                                                    )
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
                                    .border_color(hunk_opacity(cx.theme().border, is_dark, 0.92, 0.74))
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
                                                    "Settings are saved to config.toml."
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
        let reduced_motion_label = if settings.reduce_motion { "On" } else { "Off" };
        let show_fps_counter_label = if settings.show_fps_counter { "On" } else { "Off" };
        let auto_update_label = if settings.auto_update_enabled { "On" } else { "Off" };
        let update_status_label = settings_format_update_status(&self.update_status);
        let last_checked_label = settings_format_last_update_check(self.config.last_update_check_at);
        let updates_disabled_by_install_source =
            matches!(self.update_install_source, InstallSource::PackageManaged { .. });
        let update_action_in_progress = self.update_activity_in_progress();
        let update_ready_to_restart =
            matches!(self.update_status, UpdateStatus::ReadyToRestart { .. });
        let update_explanation = match &self.update_install_source {
            InstallSource::SelfManaged => {
                "Hunk checks the stable release manifest in the background, downloads verified updates automatically, and prompts before restarting."
                    .to_string()
            }
            InstallSource::PackageManaged { explanation } => explanation.clone(),
        };
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
                            .child("Theme and UI preferences."),
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
                                    .child("Updater"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(update_explanation),
                            ),
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
                                    .child("Automatic update checks"),
                            )
                            .child({
                                let view = view.clone();
                                let auto_update_enabled = settings.auto_update_enabled;
                                Button::new("settings-auto-update-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(auto_update_label)
                                    .disabled(updates_disabled_by_install_source)
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("On")
                                                .checked(auto_update_enabled)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_auto_update_enabled(
                                                                true, cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Off")
                                                .checked(!auto_update_enabled)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_auto_update_enabled(
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
                                    .child("Updater status"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(update_status_label),
                            ),
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
                                    .child("Last checked"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(last_checked_label),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .children(update_ready_to_restart.then(|| {
                                let view = view.clone();
                                Button::new("settings-install-update")
                                    .primary()
                                    .rounded(px(8.0))
                                    .label("Restart to Update")
                                    .disabled(updates_disabled_by_install_source || update_action_in_progress)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.install_available_update(Some(window), cx);
                                        });
                                    })
                                    .into_any_element()
                            }))
                            .child({
                                let view = view.clone();
                                Button::new("settings-check-for-updates")
                                    .outline()
                                    .rounded(px(8.0))
                                    .label("Check Now")
                                    .disabled(updates_disabled_by_install_source || update_action_in_progress)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.check_for_updates_action(&CheckForUpdates, window, cx);
                                        });
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
                                    .child("Refresh behavior"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "Diffs refresh immediately on file events. The app also performs \
a background periodic check as a fallback if file events are missed. Reduced Motion disables animated transitions in the Git workspace.",
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_terminal_category(
        &self,
        settings: &SettingsDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let card_surface = hunk_card_surface(cx.theme(), is_dark);
        let dropdown_bg = hunk_dropdown_fill(cx.theme(), is_dark);
        let input_surface = hunk_input_surface(cx.theme(), is_dark);
        let shell_label = settings.terminal.shell_choice.title();
        let inherit_label = if settings.terminal.inherit_login_environment {
            "On"
        } else {
            "Off"
        };
        let hydrate_label = if settings.terminal.hydrate_app_environment_on_launch {
            "On"
        } else {
            "Off"
        };
        let preserves_custom_arguments =
            terminal_shell_preserves_custom_arguments(&settings.terminal.original_shell);

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
                            .child("Terminal"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Choose the shell used for new AI terminal sessions and how much shell environment Hunk should inherit.",
                            ),
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
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child("Shell"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                "System follows Hunk's built-in per-platform shell resolver.",
                                            ),
                                    ),
                            )
                            .child({
                                let view = view.clone();
                                let selected = settings.terminal.shell_choice;
                                Button::new("settings-terminal-shell-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(shell_label)
                                    .dropdown_menu(move |menu, _, _| {
                                        SettingsTerminalShellChoice::choices_for_current_platform()
                                            .iter()
                                            .copied()
                                            .fold(menu, |menu, choice| {
                                                menu.item(
                                                    PopupMenuItem::new(choice.title())
                                                        .checked(selected == choice)
                                                        .on_click({
                                                            let view = view.clone();
                                                            move |_, _, cx| {
                                                                view.update(cx, |this, cx| {
                                                                    this.set_settings_terminal_shell_choice(
                                                                        choice, cx,
                                                                    );
                                                                });
                                                            }
                                                        }),
                                                )
                                            })
                                    })
                            }),
                    )
                    .when(
                        settings.terminal.shell_choice == SettingsTerminalShellChoice::Custom,
                        |this| {
                            this.child(
                                v_flex()
                                    .w_full()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child("Custom Shell Program"),
                                    )
                                    .child(
                                        Input::new(&settings.terminal.custom_program)
                                            .h(px(36.0))
                                            .rounded(px(8.0))
                                            .border_1()
                                            .border_color(input_surface.border)
                                            .bg(input_surface.background)
                                            .disabled(false),
                                    )
                                    .when(preserves_custom_arguments, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(
                                                    "Existing custom shell arguments from config.toml are preserved unless you change the program.",
                                                ),
                                        )
                                    }),
                            )
                        },
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child("Inherit Login Environment"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                "Controls whether new terminal shells load login/profile startup state.",
                                            ),
                                    ),
                            )
                            .child({
                                let view = view.clone();
                                let inherit_login_environment =
                                    settings.terminal.inherit_login_environment;
                                Button::new("settings-terminal-inherit-dropdown")
                                    .outline()
                                    .compact()
                                    .rounded(px(8.0))
                                    .bg(dropdown_bg)
                                    .dropdown_caret(true)
                                    .label(inherit_label)
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("On")
                                                .checked(inherit_login_environment)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_terminal_inherit_login_environment(true, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .item(
                                            PopupMenuItem::new("Off")
                                                .checked(!inherit_login_environment)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_settings_terminal_inherit_login_environment(false, cx);
                                                        });
                                                    }
                                                }),
                                        )
                                    })
                            }),
                    )
                    .when(!cfg!(target_os = "windows"), |this| {
                        this.child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    v_flex()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(cx.theme().foreground)
                                                .child("Hydrate App Environment On Launch"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(
                                                    "For GUI launches, ask the selected shell for its startup environment before the app fully boots.",
                                                ),
                                        ),
                                )
                                .child({
                                    let view = view.clone();
                                    let hydrate_app_environment_on_launch =
                                        settings.terminal.hydrate_app_environment_on_launch;
                                    Button::new("settings-terminal-hydrate-dropdown")
                                        .outline()
                                        .compact()
                                        .rounded(px(8.0))
                                        .bg(dropdown_bg)
                                        .dropdown_caret(true)
                                        .label(hydrate_label)
                                        .dropdown_menu(move |menu, _, _| {
                                            menu.item(
                                                PopupMenuItem::new("On")
                                                    .checked(hydrate_app_environment_on_launch)
                                                    .on_click({
                                                        let view = view.clone();
                                                        move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.set_settings_terminal_hydrate_app_environment_on_launch(
                                                                    true, cx,
                                                                );
                                                            });
                                                        }
                                                    }),
                                            )
                                            .item(
                                                PopupMenuItem::new("Off")
                                                    .checked(!hydrate_app_environment_on_launch)
                                                    .on_click({
                                                        let view = view.clone();
                                                        move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.set_settings_terminal_hydrate_app_environment_on_launch(
                                                                    false, cx,
                                                                );
                                                            });
                                                        }
                                                    }),
                                            )
                                        })
                                }),
                        )
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(hunk_opacity(cx.theme().muted_foreground, is_dark, 0.94, 1.0))
                            .child(if cfg!(target_os = "windows") {
                                "Shell changes apply to newly opened AI terminal sessions. App environment hydration is currently only used for Unix GUI launches."
                            } else {
                                "Shell changes apply to newly opened AI terminal sessions. Startup environment hydration changes take effect after restarting Hunk."
                            }),
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
                            .child(
                                "Edit comma-separated shortcut strings for each action. Use spaces for key sequences.",
                            ),
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
                    .text_color(hunk_opacity(cx.theme().muted_foreground, is_dark, 0.94, 1.0))
                    .child(
                        "Use commas to add alternatives, spaces for key sequences, and cmd-, literally for the comma key.",
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
