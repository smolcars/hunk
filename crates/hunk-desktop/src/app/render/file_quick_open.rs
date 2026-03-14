impl DiffViewer {
    fn render_file_quick_open_popup(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.file_quick_open_visible || self.workspace_view_mode != WorkspaceViewMode::Files {
            return div().into_any_element();
        }

        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let backdrop_bg = hunk_modal_backdrop(cx.theme(), is_dark);
        let modal_surface = hunk_modal_surface(cx.theme(), is_dark);
        let input_surface = hunk_input_surface(cx.theme(), is_dark);
        let query = self.file_quick_open_input_state.read(cx).value().to_string();
        let selected_ix = self
            .file_quick_open_selected_ix
            .min(self.file_quick_open_matches.len().saturating_sub(1));

        div()
            .id("file-quick-open-overlay")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .bg(backdrop_bg)
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.dismiss_file_quick_open(window, cx);
                    });
                    cx.stop_propagation();
                }
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
                    .size_full()
                    .p_6()
                    .flex()
                    .items_start()
                    .justify_center()
                    .child(
                        v_flex()
                            .id("file-quick-open-popup")
                            .w_full()
                            .max_w(px(680.0))
                            .max_h(px(420.0))
                            .rounded(px(16.0))
                            .border_1()
                            .border_color(modal_surface.border)
                            .bg(modal_surface.background)
                            .overflow_hidden()
                            .shadow_lg()
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
                                h_flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(hunk_opacity(
                                        cx.theme().border,
                                        is_dark,
                                        0.92,
                                        0.74,
                                    ))
                                    .child(
                                        v_flex()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_base()
                                                    .font_semibold()
                                                    .text_color(cx.theme().foreground)
                                                    .child("Quick Open"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("Search repository files and press Enter to open"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Esc to close"),
                                    ),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(hunk_opacity(
                                        cx.theme().border,
                                        is_dark,
                                        0.88,
                                        0.70,
                                    ))
                                    .child(
                                        Input::new(&self.file_quick_open_input_state)
                                            .h(px(36.0))
                                            .rounded(px(10.0))
                                            .border_1()
                                            .border_color(input_surface.border)
                                            .bg(input_surface.background),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .max_h(px(280.0))
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .occlude()
                                    .p_2()
                                    .when(
                                        self.repo_file_search_loading
                                            && self.file_quick_open_matches.is_empty(),
                                        |this| {
                                            this.child(
                                                div()
                                                    .px_3()
                                                    .py_4()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("Loading files..."),
                                            )
                                        },
                                    )
                                    .when(
                                        !self.repo_file_search_loading
                                            && self.file_quick_open_matches.is_empty(),
                                        |this| {
                                            let empty_message = if query.trim().is_empty() {
                                                "No files available."
                                            } else {
                                                "No matching files."
                                            };
                                            this.child(
                                                div()
                                                    .px_3()
                                                    .py_4()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(empty_message),
                                            )
                                        },
                                    )
                                    .children(
                                        self.file_quick_open_matches
                                            .iter()
                                            .enumerate()
                                            .map(|(ix, path)| {
                                                let selected = ix == selected_ix;
                                                let select_view = view.clone();
                                                let select_path = path.clone();
                                                let file_name = path
                                                    .rsplit('/')
                                                    .next()
                                                    .unwrap_or(path.as_str())
                                                    .to_string();
                                                let dir_prefix = path
                                                    .strip_suffix(file_name.as_str())
                                                    .unwrap_or_default()
                                                    .trim_end_matches('/')
                                                    .to_string();

                                                h_flex()
                                                    .id(("file-quick-open-item", ix))
                                                    .w_full()
                                                    .min_w_0()
                                                    .items_center()
                                                    .gap_2()
                                                    .rounded(px(12.0))
                                                    .px_3()
                                                    .py_2()
                                                    .hover(|style| {
                                                        style.bg(hunk_opacity(
                                                            cx.theme().accent,
                                                            is_dark,
                                                            0.18,
                                                            0.10,
                                                        ))
                                                    })
                                                    .when(selected, |this| {
                                                        this.bg(hunk_opacity(
                                                            cx.theme().accent,
                                                            is_dark,
                                                            0.24,
                                                            0.14,
                                                        ))
                                                        .border_1()
                                                        .border_color(hunk_opacity(
                                                            cx.theme().accent,
                                                            is_dark,
                                                            0.64,
                                                            0.44,
                                                        ))
                                                    })
                                                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                                        select_view.update(cx, |this, cx| {
                                                            this.accept_file_quick_open_path(
                                                                select_path.clone(),
                                                                window,
                                                                cx,
                                                            );
                                                        });
                                                        cx.stop_propagation();
                                                    })
                                                    .child(
                                                        Icon::new(IconName::File)
                                                            .size(px(14.0))
                                                            .text_color(if selected {
                                                                cx.theme().accent
                                                            } else {
                                                                cx.theme().muted_foreground
                                                            }),
                                                    )
                                                    .child(
                                                        v_flex()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .gap_0p5()
                                                            .child(
                                                                div()
                                                                    .truncate()
                                                                    .text_sm()
                                                                    .font_medium()
                                                                    .text_color(cx.theme().foreground)
                                                                    .child(file_name),
                                                            )
                                                            .child(
                                                                div()
                                                                    .truncate()
                                                                    .text_xs()
                                                                    .text_color(cx.theme().muted_foreground)
                                                                    .child(if dir_prefix.is_empty() {
                                                                        ".".to_string()
                                                                    } else {
                                                                        dir_prefix
                                                                    }),
                                                            ),
                                                    )
                                            }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
