impl DiffViewer {
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let project_label = self.project_display_name();
        let repo_label = self
            .repo_root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "No Git repository found".to_string());
        let selected_theme = self.config.theme;
        let theme_label = match self.config.theme {
            ThemePreference::System => "System",
            ThemePreference::Light => "Light",
            ThemePreference::Dark => "Dark",
        };
        let theme_button_label = format!("Theme ({theme_label})");
        let is_dark = cx.theme().mode.is_dark();
        let git_selected = self.workspace_view_mode == WorkspaceViewMode::GitWorkspace;
        let active_branch = self
            .checked_out_branch_name()
            .unwrap_or(self.branch_name.as_str())
            .to_string();
        let chip_colors = hunk_toolbar_chip(cx.theme(), is_dark);
        let brand_colors = hunk_toolbar_brand_chip(cx.theme(), is_dark);
        let dropdown_bg = hunk_dropdown_fill(cx.theme(), is_dark);

        h_flex()
            .w_full()
            .h_11()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .overflow_x_hidden()
                    .child(
                        h_flex()
                            .items_center()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(brand_colors.background)
                            .border_1()
                            .border_color(brand_colors.border)
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(project_label),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(chip_colors.background)
                            .border_1()
                            .border_color(chip_colors.border)
                            .child(
                                div()
                                    .text_sm()
                                    .font_medium()
                                    .text_color(cx.theme().foreground)
                                    .child(active_branch),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_1()
                            .overflow_x_hidden()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(chip_colors.background)
                            .border_1()
                            .border_color(chip_colors.border)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .text_sm()
                                    .text_color(cx.theme().foreground.opacity(0.82))
                                    .child(repo_label),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .child(
                        h_flex().items_center().gap_1().child(
                            Button::new("theme-dropdown")
                                .outline()
                                .compact()
                                .rounded(px(7.0))
                                .bg(dropdown_bg)
                                .dropdown_caret(true)
                                .label(theme_button_label)
                                .dropdown_menu({
                                    let view = view.clone();
                                    move |menu, _, _| {
                                        menu.item(
                                            PopupMenuItem::new("System")
                                                .checked(selected_theme == ThemePreference::System)
                                                .on_click({
                                                    let view = view.clone();
                                                    move |_, window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_theme_preference(
                                                                ThemePreference::System,
                                                                window,
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
                                                    move |_, window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_theme_preference(
                                                                ThemePreference::Light,
                                                                window,
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
                                                    move |_, window, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.set_theme_preference(
                                                                ThemePreference::Dark,
                                                                window,
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                    }
                                }),
                        ),
                    )
                    .child({
                        let view = view.clone();
                        Button::new("toggle-comments-preview")
                            .outline()
                            .compact()
                            .rounded(px(7.0))
                            .bg(dropdown_bg)
                            .label(format!("Comments ({})", self.comments_open_count()))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_comments_preview(cx);
                                });
                            })
                    })
                    .when(git_selected, |this| {
                        this.when(self.overall_line_stats.changed() > 0, |this| {
                            this.child(self.render_line_stats("overall", self.overall_line_stats, cx))
                        })
                        .child(self.render_git_metric_pill(
                            if self.branch_has_upstream {
                                "Published"
                            } else {
                                "Local Only"
                            },
                            if self.branch_has_upstream {
                                HunkAccentTone::Success
                            } else {
                                HunkAccentTone::Warning
                            },
                            cx,
                        ))
                        .child(self.render_git_metric_pill(
                            format!("Ahead {}", self.branch_ahead_count),
                            if self.branch_ahead_count > 0 {
                                HunkAccentTone::Accent
                            } else {
                                HunkAccentTone::Neutral
                            },
                            cx,
                        ))
                        .child(self.render_git_metric_pill(
                            format!("Behind {}", self.branch_behind_count),
                            if self.branch_behind_count > 0 {
                                HunkAccentTone::Warning
                            } else {
                                HunkAccentTone::Neutral
                            },
                            cx,
                        ))
                        .child(self.render_git_metric_pill(
                            format!("Changed {}", self.files.len()),
                            if self.files.is_empty() {
                                HunkAccentTone::Neutral
                            } else {
                                HunkAccentTone::Accent
                            },
                            cx,
                        ))
                    })
                    .when(!git_selected, |this| {
                        this.child(self.render_line_stats("overall", self.overall_line_stats, cx))
                            .child({
                                let view = view.clone();
                                Button::new("toggle-diff-whitespace")
                                    .outline()
                                    .compact()
                                    .rounded(px(7.0))
                                    .bg(dropdown_bg)
                                    .label(if self.diff_show_whitespace {
                                        "Whitespace: On"
                                    } else {
                                        "Whitespace: Off"
                                    })
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.toggle_diff_show_whitespace(cx);
                                        });
                                    })
                            })
                            .child({
                                let view = view.clone();
                                Button::new("toggle-diff-eol")
                                    .outline()
                                    .compact()
                                    .rounded(px(7.0))
                                    .bg(dropdown_bg)
                                    .label(if self.diff_show_eol_markers {
                                        "EOL: On"
                                    } else {
                                        "EOL: Off"
                                    })
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.toggle_diff_show_eol_markers(cx);
                                        });
                                    })
                            })
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{} files", self.files.len())),
                            )
                    })
                    .when(self.config.show_fps_counter, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(if self.fps >= 110.0 {
                                    cx.theme().success
                                } else if self.fps >= 60.0 {
                                    cx.theme().warning
                                } else {
                                    cx.theme().danger
                                })
                                .child(format!("{:>3.0} fps", self.fps.round())),
                        )
                    }),
            )
    }

    fn project_display_name(&self) -> String {
        self.repo_root
            .as_ref()
            .or(self.project_path.as_ref())
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| "Hunk".to_string())
    }

}
