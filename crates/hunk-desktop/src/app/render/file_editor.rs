#[derive(Clone, Copy)]
struct MarkdownInlineRenderStyle {
    base_color: Hsla,
    is_dark: bool,
}

impl DiffViewer {
    fn files_terminal_panel_state(&self) -> TerminalPanelState {
        TerminalPanelState {
            kind: WorkspaceTerminalKind::Files,
            open: self.files_terminal_open,
            active_tab_id: self.files_terminal_active_tab_id,
            tabs: self
                .files_visible_terminal_tabs_snapshot()
                .into_iter()
                .map(|tab| TerminalPanelTabState {
                    id: tab.id,
                    title: tab.title,
                    status: tab.session.status,
                })
                .collect(),
            cwd_label: self
                .files_terminal_session
                .cwd
                .clone()
                .or_else(|| self.repo_root.clone())
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "No repository selected".to_string()),
            shell_label: ai_terminal_shell_label(&self.config),
            status_message: self.files_terminal_session.status_message.clone(),
            status: self.files_terminal_session.status,
            running: self.files_terminal_is_running(),
            surface_focused: self.files_terminal_surface_focused,
            screen: self.files_terminal_session.screen.clone(),
            display_offset: self
                .files_terminal_session
                .screen
                .as_ref()
                .map(|screen| screen.display_offset)
                .unwrap_or(0),
            has_transcript: !self.files_terminal_session.transcript.trim().is_empty(),
            has_output: self.files_terminal_session.screen.is_some()
                || !self.files_terminal_session.transcript.trim().is_empty(),
            has_last_command: self.files_terminal_session.last_command.is_some(),
            transcript: self.files_terminal_session.transcript.clone(),
            height_px: self.files_terminal_height_px,
        }
    }

    fn file_editor_tab_title(&self, path: &str) -> String {
        std::path::Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.to_string())
    }

    fn file_editor_tab_detail(&self, path: &str) -> Option<String> {
        let title = self.file_editor_tab_title(path);
        let duplicate_titles = self
            .file_editor_tabs
            .iter()
            .filter(|tab| self.file_editor_tab_title(tab.path.as_str()) == title)
            .count();
        if duplicate_titles < 2 {
            return None;
        }

        std::path::Path::new(path)
            .parent()
            .and_then(|parent| parent.to_str())
            .filter(|parent| !parent.is_empty() && *parent != ".")
            .map(ToOwned::to_owned)
    }

    fn render_file_editor_tab_bar(
        &self,
        view: Entity<Self>,
        _editor_chrome: HunkEditorChromeColors,
        _is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.file_editor_tabs.is_empty() {
            return div().into_any_element();
        }

        let tab_height = px(crate::app::FILES_WORKSPACE_RAIL_HEIGHT);
        let border_color = cx.theme().border;

        div()
            .w_full()
            .h(tab_height)
            .bg(cx.theme().tab_bar)
            .child(
                div()
                    .id("file-editor-tab-scroll-area")
                    .w_full()
                    .h_full()
                    .track_scroll(&self.file_editor_tab_scroll_handle)
                    .overflow_x_scroll()
                    .child(
                        h_flex()
                            .w_full()
                            .h_full()
                            .items_center()
                            .children(self.file_editor_tabs.iter().map(|tab| {
                                let activate_view = view.clone();
                                let tab_id = tab.id;
                                let path = tab.path.clone();
                                let status = self
                                    .status_for_path(path.as_str())
                                    .unwrap_or(FileStatus::Unknown);
                                let is_active = self.active_file_editor_tab_id == Some(tab_id);
                                let mut title = self.file_editor_tab_title(path.as_str());
                                let detail = self.file_editor_tab_detail(path.as_str());
                                let is_dirty =
                                    if is_active { self.editor_dirty } else { tab.dirty };
                                let is_loading =
                                    if is_active { self.editor_loading } else { tab.loading };
                                let has_error = if is_active {
                                    self.editor_error.is_some()
                                } else {
                                    tab.error.is_some()
                                };
                                if is_dirty {
                                    title.push_str(" •");
                                }

                                let mut tab_surface = div()
                                    .id(("file-editor-tab", tab_id))
                                    .flex_none()
                                    .min_w(px(150.0))
                                    .max_w(px(260.0))
                                    .h(tab_height)
                                    .px_2()
                                    .gap_2()
                                    .items_center()
                                    .border_r_1()
                                    .border_b_1()
                                    .border_color(border_color)
                                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                        activate_view.update(cx, |this, cx| {
                                            let _ = this.open_file_in_files_workspace(
                                                path.clone(),
                                                status,
                                                window,
                                                cx,
                                            );
                                        });
                                    });
                                if is_active {
                                    tab_surface = tab_surface
                                        .bg(cx.theme().tab_active)
                                        .border_b_0();
                                } else {
                                    tab_surface = tab_surface
                                        .bg(cx.theme().tab)
                                        .hover(|this| this.bg(cx.theme().muted))
                                        .cursor_pointer();
                                }

                                tab_surface
                                    .child(
                                        h_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                h_flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_sm()
                                                            .font_family(
                                                                cx.theme().font_family.clone(),
                                                            )
                                                            .text_color(if is_active {
                                                                cx.theme().tab_active_foreground
                                                            } else {
                                                                cx.theme().tab_foreground
                                                            })
                                                            .child(title),
                                                    )
                                                    .when_some(detail, |this, detail| {
                                                        this.child(
                                                            div()
                                                                .truncate()
                                                                .text_xs()
                                                                .italic()
                                                                .text_color(if is_active {
                                                                    cx.theme().tab_active_foreground
                                                                } else {
                                                                    cx.theme().tab_foreground
                                                                })
                                                                .child(detail),
                                                        )
                                                    }),
                                            )
                                            .child({
                                                let close_view = view.clone();
                                                Button::new(("file-editor-tab-close", tab_id))
                                                    .ghost()
                                                    .xsmall()
                                                    .icon(Icon::new(IconName::Close).size(px(12.0)))
                                                    .tooltip("Close tab")
                                                    .on_click(move |_, window, cx| {
                                                        cx.stop_propagation();
                                                        close_view.update(cx, |this, cx| {
                                                            this.close_file_editor_tab_by_id(
                                                                tab_id, window, cx,
                                                            );
                                                        });
                                                    })
                                            })
                                            .when(has_error || is_loading, |this| {
                                                this.child(
                                                    if has_error {
                                                        Icon::new(IconName::TriangleAlert)
                                                            .size(px(12.0))
                                                            .text_color(cx.theme().danger)
                                                            .into_any_element()
                                                    } else {
                                                        Icon::new(IconName::LoaderCircle)
                                                            .size(px(12.0))
                                                            .text_color(cx.theme().warning)
                                                            .into_any_element()
                                                    },
                                                )
                                            }),
                                    )
                                    .into_any_element()
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .border_b_1()
                                    .border_color(border_color),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn markdown_inline_render_style(
        &self,
        base_color: Hsla,
        is_dark: bool,
    ) -> MarkdownInlineRenderStyle {
        MarkdownInlineRenderStyle {
            base_color,
            is_dark,
        }
    }

    fn render_file_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let Some(file_path) = self.editor_path.clone() else {
            let terminal_state = self.files_terminal_panel_state();
            return v_flex()
                .size_full()
                .child(
                    v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Select a file from Files tree to edit it."),
                        ),
                )
                .when(terminal_state.open, |this| {
                    this.child(
                        self.render_workspace_terminal_panel(view.clone(), &terminal_state, is_dark, cx)
                            .unwrap_or_else(|| div().into_any_element()),
                    )
                })
                .into_any_element();
        };

        let editor_chrome = crate::app::theme::hunk_editor_chrome_colors(cx.theme(), is_dark);
        let editor_font_size = cx.theme().mono_font_size * 1.2;
        let is_markdown_file = is_markdown_path(file_path.as_str());
        let preview_active = is_markdown_file && self.editor_markdown_preview;
        let (editor_status, search_match_count, show_whitespace, soft_wrap_enabled) = {
            let files_editor = self.files_editor.borrow();
            (
                files_editor.status_snapshot(),
                files_editor.search_match_count(),
                files_editor.show_whitespace(),
                files_editor.soft_wrap_enabled(),
            )
        };
        let status_color = if self.editor_save_loading {
            cx.theme().warning
        } else if self.editor_dirty {
            cx.theme().danger
        } else {
            cx.theme().success
        };
        let status_label = if self.editor_save_loading {
            "Saving..."
        } else if self.editor_dirty {
            "Unsaved changes"
        } else {
            "Saved"
        };
        let save_disabled = self.editor_save_loading || !self.editor_dirty;
        let reload_disabled = self.editor_save_loading;
        let language_label = editor_status
            .as_ref()
            .map(|status| status.language.clone())
            .unwrap_or_else(|| "text".to_string());
        let selection_label = editor_status
            .as_ref()
            .map(|status| status.selection.clone())
            .unwrap_or_else(|| "0 cursors".to_string());
        let position_label = editor_status
            .as_ref()
            .map(|status| status.position.clone())
            .unwrap_or_else(|| "Ln 1  Col 1".to_string());
        let meta_label = format!("{position_label}  {selection_label}");
        let editor_content = if self.editor_loading {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading file editor..."),
                )
                .into_any_element()
        } else if let Some(error) = self.editor_error.as_ref() {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_6()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .whitespace_normal()
                        .child(error.clone()),
                )
                .into_any_element()
        } else if preview_active {
            self.render_markdown_preview(is_dark, cx)
        } else {
            self.render_file_editor_surface(window, editor_font_size, is_dark, cx)
        };
        let terminal_state = self.files_terminal_panel_state();

        v_flex()
            .size_full()
            .child(self.render_file_editor_tab_bar(
                view.clone(),
                editor_chrome,
                is_dark,
                cx,
            ))
            .child(
                h_flex()
                    .w_full()
                    .h(px(34.0))
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(hunk_opacity(cx.theme().border, is_dark, 0.86, 0.72))
                    .bg(cx.theme().tab_bar)
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(editor_chrome.foreground)
                                    .child(file_path),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(status_color)
                                    .child(status_label),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(editor_chrome.line_number)
                                    .child(language_label),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .text_color(editor_chrome.line_number)
                                    .child(meta_label),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1p5()
                            .child({
                                let view = view.clone();
                                let mut button = Button::new("editor-search-toggle")
                                    .compact()
                                    .rounded(px(7.0))
                                    .icon(Icon::new(IconName::Search).size(px(12.0)))
                                    .tooltip(if self.editor_search_visible {
                                        "Hide find and replace"
                                    } else {
                                        "Show find and replace"
                                    })
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.toggle_editor_search_visibility(window, cx);
                                        });
                                    });
                                if self.editor_search_visible {
                                    button = button.primary();
                                } else {
                                    button = button.outline();
                                }
                                button
                            })
                            .child({
                                let view = view.clone();
                                let mut button = Button::new("editor-wrap-toggle")
                                    .compact()
                                    .rounded(px(7.0))
                                    .icon(Icon::new(IconName::ALargeSmall).size(px(12.0)))
                                    .tooltip("Toggle soft wrap")
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
                                    button = button.outline();
                                }
                                button
                            })
                            .child({
                                let view = view.clone();
                                let mut button = Button::new("editor-whitespace-toggle")
                                    .compact()
                                    .rounded(px(7.0))
                                    .icon(Icon::new(IconName::Eye).size(px(12.0)))
                                    .tooltip("Toggle invisible characters")
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
                                    button = button.outline();
                                }
                                button
                            })
                            .child({
                                let view = view.clone();
                                Button::new("editor-reload")
                                    .outline()
                                    .compact()
                                    .rounded(px(7.0))
                                    .icon(Icon::new(HunkIconName::RotateCcw).size(px(12.0)))
                                    .tooltip("Reload file")
                                    .disabled(reload_disabled)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.reload_current_editor_file(cx);
                                        });
                                    })
                            })
                            .child(
                                if is_markdown_file {
                                    let view = view.clone();
                                    let mut preview_button = Button::new("editor-markdown-preview")
                                        .compact()
                                        .rounded(px(7.0))
                                        .label(if self.editor_markdown_preview {
                                            "Edit"
                                        } else {
                                            "Preview"
                                        })
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.toggle_editor_markdown_preview(cx);
                                            });
                                        });
                                    if self.editor_markdown_preview {
                                        preview_button = preview_button.primary();
                                    } else {
                                        preview_button = preview_button.outline();
                                    }
                                    preview_button.into_any_element()
                                } else {
                                    div().into_any_element()
                                }
                            )
                            .child({
                                let view = view.clone();
                                let mut button = Button::new("editor-save")
                                    .compact()
                                    .rounded(px(7.0))
                                    .icon(Icon::new(HunkIconName::Save).size(px(12.0)))
                                    .tooltip(if self.editor_save_loading {
                                        "Saving file"
                                    } else if self.editor_dirty {
                                        "Save file"
                                    } else {
                                        "File is saved"
                                    })
                                    .loading(self.editor_save_loading)
                                    .disabled(save_disabled)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.save_current_editor_file(window, cx);
                                        });
                                    });
                                if save_disabled {
                                    button = button.outline();
                                } else {
                                    button = button.primary();
                                }
                                button
                            }),
                    ),
            )
            .when(self.editor_search_visible, |this| {
                this.child(self.render_workspace_search_bar(
                    view.clone(),
                    editor_chrome,
                    is_dark,
                    search_match_count,
                    true,
                    cx,
                ))
            })
            .child(editor_content)
            .when(terminal_state.open, |this| {
                this.child(
                    self.render_workspace_terminal_panel(view.clone(), &terminal_state, is_dark, cx)
                        .unwrap_or_else(|| div().into_any_element()),
                )
            })
            .into_any_element()
    }

    fn render_markdown_preview(&self, is_dark: bool, cx: &mut Context<Self>) -> AnyElement {
        if self.editor_markdown_preview_blocks.is_empty() {
            let placeholder = if self.editor_markdown_preview_loading {
                "Preparing markdown preview..."
            } else {
                "Markdown preview is empty."
            };

            return div()
                .flex_1()
                .size_full()
                .min_h_0()
                .p_2()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(placeholder)
                .into_any_element();
        }

        let view = cx.entity();
        let rendered_blocks = self
            .editor_markdown_preview_blocks
            .iter()
            .enumerate()
            .map(|(block_ix, block)| {
                self.render_markdown_preview_block(view.clone(), block_ix, block, is_dark, cx)
            })
            .collect::<Vec<_>>();
        let mut preview = div().flex_1().size_full().min_h_0().p_2().child(
            div()
                .w_full()
                .overflow_y_scrollbar()
                .v_flex()
                .gap_2()
                .children(rendered_blocks)
                .into_any_element(),
        );

        if self.editor_markdown_preview_loading {
            preview = preview.child(
                div()
                    .w_full()
                    .px_1()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Updating preview..."),
            );
        }

        preview.into_any_element()
    }

    fn render_markdown_preview_block(
        &self,
        view: Entity<Self>,
        block_ix: usize,
        block: &MarkdownPreviewBlock,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let element = match block {
            MarkdownPreviewBlock::Heading { level, spans } => {
                let heading = match level {
                    1 | 2 => self.render_markdown_inline_spans(
                        view.clone(),
                        spans,
                        true,
                        true,
                        self.markdown_inline_render_style(cx.theme().foreground, is_dark),
                        cx,
                    ),
                    _ => self.render_markdown_inline_spans(
                        view.clone(),
                        spans,
                        false,
                        true,
                        self.markdown_inline_render_style(cx.theme().foreground, is_dark),
                        cx,
                    ),
                };
                heading.into_any_element()
            }
            MarkdownPreviewBlock::Paragraph(spans) => self
                .render_markdown_inline_spans(
                    view.clone(),
                    spans,
                    false,
                    false,
                    self.markdown_inline_render_style(cx.theme().foreground, is_dark),
                    cx,
                )
                .into_any_element(),
            MarkdownPreviewBlock::UnorderedListItem(spans) => h_flex()
                .w_full()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("-"),
                )
                .child(
                    self.render_markdown_inline_spans(
                        view.clone(),
                        spans,
                        false,
                        false,
                        self.markdown_inline_render_style(cx.theme().foreground, is_dark),
                        cx,
                    ),
                )
                .into_any_element(),
            MarkdownPreviewBlock::OrderedListItem { number, spans } => h_flex()
                .w_full()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{number}.")),
                )
                .child(
                    self.render_markdown_inline_spans(
                        view.clone(),
                        spans,
                        false,
                        false,
                        self.markdown_inline_render_style(cx.theme().foreground, is_dark),
                        cx,
                    ),
                )
                .into_any_element(),
            MarkdownPreviewBlock::BlockQuote(spans) => h_flex()
                .w_full()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("|"),
                )
                .child(
                    self.render_markdown_inline_spans(
                        view.clone(),
                        spans,
                        false,
                        false,
                        self.markdown_inline_render_style(cx.theme().muted_foreground, is_dark),
                        cx,
                    ),
                )
                .into_any_element(),
            MarkdownPreviewBlock::CodeBlock { language, lines } => {
                let language_label = language.clone().unwrap_or_else(|| "code".to_string());
                let code_rows = if lines.is_empty() {
                    vec![
                        div()
                            .w_full()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child("")
                            .into_any_element(),
                    ]
                } else {
                    lines
                        .iter()
                        .map(|line_spans| {
                            h_flex()
                                .w_full()
                                .items_start()
                                .gap_0()
                                .text_xs()
                                .font_family(cx.theme().mono_font_family.clone())
                                .flex_wrap()
                                .whitespace_normal()
                                .children(line_spans.iter().map(|span| {
                                    let token_color = markdown_syntax_color(
                                        cx.theme(),
                                        cx.theme().foreground,
                                        span.token,
                                    );
                                    div()
                                        .flex_none()
                                        .whitespace_nowrap()
                                        .text_color(token_color)
                                        .child(span.text.clone())
                                        .into_any_element()
                                }))
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                };

                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(language_label),
                    )
                    .child(
                        div()
                            .w_full()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(hunk_opacity(cx.theme().border, is_dark, 0.88, 0.74))
                            .bg(hunk_opacity(cx.theme().secondary, is_dark, 0.34, 0.48))
                            .p_2()
                            .child(v_flex().w_full().children(code_rows)),
                    )
                    .into_any_element()
            }
            MarkdownPreviewBlock::ThematicBreak => div()
                .h(px(1.0))
                .w_full()
                .bg(hunk_opacity(cx.theme().border, is_dark, 0.8, 0.95))
                .into_any_element(),
        };

        self.wrap_markdown_preview_block_context_menu(view, block_ix, block, element, cx)
    }

    fn wrap_markdown_preview_block_context_menu(
        &self,
        view: Entity<Self>,
        block_ix: usize,
        block: &MarkdownPreviewBlock,
        element: AnyElement,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(selection_surfaces) = self.markdown_preview_block_selection_surfaces(block_ix, block)
        else {
            return element;
        };
        let row_id = format!("editor-markdown-preview-block-{block_ix}");
        div()
            .w_full()
            .on_mouse_down(MouseButton::Right, {
                let row_id = row_id.clone();
                let selection_surfaces = selection_surfaces.clone();
                move |event, window, cx| {
                    view.update(cx, |this, cx| {
                        this.focus_handle.focus(window, cx);
                        if !this.ai_select_all_text_for_surfaces(
                            row_id.as_str(),
                            selection_surfaces.clone(),
                            cx,
                        ) {
                            return;
                        }
                        this.open_workspace_text_context_menu(
                            WorkspaceTextContextMenuTarget::SelectableText(
                                SelectableTextContextMenuTarget {
                                    row_id: row_id.clone(),
                                    selection_surfaces: selection_surfaces.clone(),
                                    can_copy: true,
                                    can_select_all: true,
                                    link_target: None,
                                },
                            ),
                            event.position,
                            cx,
                        );
                    });
                    cx.stop_propagation();
                }
            })
            .child(element)
            .into_any_element()
    }

    fn markdown_preview_block_selection_surfaces(
        &self,
        block_ix: usize,
        block: &MarkdownPreviewBlock,
    ) -> Option<Arc<[AiTextSelectionSurfaceSpec]>> {
        let mut surfaces = Vec::new();
        match block {
            MarkdownPreviewBlock::Heading { spans, .. }
            | MarkdownPreviewBlock::Paragraph(spans)
            | MarkdownPreviewBlock::UnorderedListItem(spans)
            | MarkdownPreviewBlock::BlockQuote(spans) => {
                let text = markdown_inline_spans_plain_text(spans.as_slice());
                if !text.is_empty() {
                    surfaces.push(AiTextSelectionSurfaceSpec::new(
                        format!("markdown-preview-block-{block_ix}-0"),
                        text,
                    ));
                }
            }
            MarkdownPreviewBlock::OrderedListItem { number, spans } => {
                let text = format!("{number}. {}", markdown_inline_spans_plain_text(spans.as_slice()));
                surfaces.push(AiTextSelectionSurfaceSpec::new(
                    format!("markdown-preview-block-{block_ix}-0"),
                    text,
                ));
            }
            MarkdownPreviewBlock::CodeBlock { lines, .. } => {
                for (line_ix, spans) in lines.iter().enumerate() {
                    let surface = AiTextSelectionSurfaceSpec::new(
                        format!("markdown-preview-block-{block_ix}-{line_ix}"),
                        markdown_code_line_plain_text(spans.as_slice()),
                    );
                    surfaces.push(if line_ix == 0 {
                        surface
                    } else {
                        surface.with_separator_before("\n")
                    });
                }
            }
            MarkdownPreviewBlock::ThematicBreak => {}
        }

        if surfaces.is_empty() {
            None
        } else {
            Some(Arc::from(surfaces))
        }
    }

    fn render_markdown_inline_spans(
        &self,
        view: Entity<Self>,
        spans: &[MarkdownInlineSpan],
        large: bool,
        emphasized: bool,
        style: MarkdownInlineRenderStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if spans.is_empty() {
            return div().w_full().text_sm().child("").into_any_element();
        }

        let mut row = h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_0()
            .text_color(style.base_color)
            .flex_wrap()
            .whitespace_normal()
            .children(
                spans
                    .iter()
                    .map(|span| self.render_markdown_inline_span(view.clone(), span, style, cx)),
            );

        if large {
            row = row.text_lg();
        } else {
            row = row.text_sm();
        }
        if emphasized {
            row = row.font_semibold();
        }

        row.into_any_element()
    }

    fn render_markdown_inline_span(
        &self,
        view: Entity<Self>,
        span: &MarkdownInlineSpan,
        style: MarkdownInlineRenderStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if span.style.hard_break {
            return div().w_full().h(px(0.0)).into_any_element();
        }

        let mut element = div()
            .flex_none()
            .whitespace_nowrap()
            .text_color(style.base_color)
            .child(span.text.clone());

        if span.style.bold {
            element = element.font_semibold();
        }
        if span.style.italic {
            element = element.italic();
        }
        if span.style.strikethrough {
            element = element.line_through();
        }
        if span.style.code {
            element = element
                .font_family(cx.theme().mono_font_family.clone())
                .bg(hunk_opacity(cx.theme().secondary, style.is_dark, 0.34, 0.48))
                .border_1()
                .border_color(hunk_opacity(cx.theme().border, style.is_dark, 0.88, 0.74))
                .rounded(px(4.0))
                .px_1();
        }
        if let Some(raw_target) = span.style.link.as_ref().cloned() {
            let link_color = cx.theme().primary;
            element = element
                .underline()
                .text_color(link_color)
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| {
                        this.activate_markdown_link(raw_target.clone(), Some(window), cx);
                    });
                });
        }

        element.into_any_element()
    }

}

fn markdown_inline_spans_plain_text(spans: &[MarkdownInlineSpan]) -> String {
    let mut text = String::new();
    for span in spans {
        if span.style.hard_break {
            text.push('\n');
        }
        text.push_str(span.text.as_str());
    }
    text
}

fn markdown_code_line_plain_text(
    spans: &[hunk_domain::markdown_preview::MarkdownCodeSpan],
) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

fn is_desktop_clipboard_shortcut(keystroke: &gpui::Keystroke) -> bool {
    let uses_desktop_modifier = keystroke.modifiers.platform || keystroke.modifiers.control;
    uses_desktop_modifier
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.function
        && matches!(keystroke.key.as_str(), "c" | "x" | "v")
}
