impl DiffViewer {
    fn render_file_editor(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.editor_loading {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading file editor..."),
                )
                .into_any_element();
        }

        if let Some(error) = self.editor_error.as_ref() {
            return v_flex()
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
                .into_any_element();
        }

        let Some(file_path) = self.editor_path.clone() else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Select a file from Files tree to edit it."),
                )
                .into_any_element();
        };

        let view = cx.entity();
        let is_dark = cx.theme().mode.is_dark();
        let editor_font_size = cx.theme().mono_font_size * 1.2;
        let is_markdown_file = is_markdown_path(file_path.as_str());
        let preview_active = is_markdown_file && self.editor_markdown_preview;
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

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
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
                                    .text_color(cx.theme().muted_foreground)
                                    .child(file_path),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(status_color)
                                    .child(status_label),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child({
                                let view = view.clone();
                                Button::new("editor-reload")
                                    .outline()
                                    .compact()
                                    .rounded(px(7.0))
                                    .bg(
                                        cx.theme().secondary.opacity(if is_dark { 0.46 } else { 0.68 }),
                                    )
                                    .border_color(
                                        cx.theme().border.opacity(if is_dark { 0.86 } else { 0.70 }),
                                    )
                                    .label("Reload")
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
                                        preview_button = preview_button
                                            .outline()
                                            .bg(
                                                cx.theme().secondary.opacity(if is_dark { 0.46 } else { 0.68 }),
                                            )
                                            .border_color(
                                                cx.theme().border.opacity(if is_dark { 0.86 } else { 0.70 }),
                                            );
                                    }
                                    preview_button.into_any_element()
                                } else {
                                    div().into_any_element()
                                }
                            )
                            .child({
                                let view = view.clone();
                                Button::new("editor-save")
                                    .primary()
                                    .compact()
                                    .rounded(px(7.0))
                                    .label("Save")
                                    .disabled(save_disabled)
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.save_current_editor_file(window, cx);
                                        });
                                    })
                            }),
                    ),
            )
            .child(if preview_active {
                self.render_markdown_preview(is_dark, cx)
            } else {
                div()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .child(
                        Input::new(&self.editor_input_state)
                            .h_full()
                            .text_size(editor_font_size)
                            .disabled(self.editor_loading || self.editor_save_loading)
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(
                                cx.theme().border.opacity(if is_dark { 0.92 } else { 0.78 }),
                            ),
                    )
                    .into_any_element()
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

        let rendered_blocks = self
            .editor_markdown_preview_blocks
            .iter()
            .map(|block| self.render_markdown_preview_block(block, is_dark, cx))
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
        block: &MarkdownPreviewBlock,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match block {
            MarkdownPreviewBlock::Heading { level, spans } => {
                let heading = match level {
                    1 | 2 => self.render_markdown_inline_spans(
                        spans,
                        true,
                        true,
                        cx.theme().foreground,
                        is_dark,
                        cx,
                    ),
                    _ => self.render_markdown_inline_spans(
                        spans,
                        false,
                        true,
                        cx.theme().foreground,
                        is_dark,
                        cx,
                    ),
                };
                heading.into_any_element()
            }
            MarkdownPreviewBlock::Paragraph(spans) => self
                .render_markdown_inline_spans(
                    spans,
                    false,
                    false,
                    cx.theme().foreground,
                    is_dark,
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
                        spans,
                        false,
                        false,
                        cx.theme().foreground,
                        is_dark,
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
                        spans,
                        false,
                        false,
                        cx.theme().foreground,
                        is_dark,
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
                        spans,
                        false,
                        false,
                        cx.theme().muted_foreground,
                        is_dark,
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
                                    let token_color = self.markdown_code_token_color(
                                        cx.theme().foreground,
                                        span.token,
                                        cx,
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
                            .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.74 }))
                            .bg(cx.theme().secondary.opacity(if is_dark { 0.34 } else { 0.48 }))
                            .p_2()
                            .child(v_flex().w_full().children(code_rows)),
                    )
                    .into_any_element()
            }
            MarkdownPreviewBlock::ThematicBreak => div()
                .h(px(1.0))
                .w_full()
                .bg(cx.theme().border.opacity(if is_dark { 0.8 } else { 0.95 }))
                .into_any_element(),
        }
    }

    fn render_markdown_inline_spans(
        &self,
        spans: &[MarkdownInlineSpan],
        large: bool,
        emphasized: bool,
        base_color: Hsla,
        is_dark: bool,
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
            .text_color(base_color)
            .flex_wrap()
            .whitespace_normal()
            .children(
                spans
                    .iter()
                    .map(|span| self.render_markdown_inline_span(span, base_color, is_dark, cx)),
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
        span: &MarkdownInlineSpan,
        base_color: Hsla,
        is_dark: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if span.style.hard_break {
            return div().w_full().h(px(0.0)).into_any_element();
        }

        let mut element = div()
            .flex_none()
            .whitespace_nowrap()
            .text_color(base_color)
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
                .bg(cx.theme().secondary.opacity(if is_dark { 0.34 } else { 0.48 }))
                .border_1()
                .border_color(cx.theme().border.opacity(if is_dark { 0.88 } else { 0.74 }))
                .rounded(px(4.0))
                .px_1();
        }
        if span.style.link.is_some() {
            element = element.underline();
        }

        element.into_any_element()
    }

    fn markdown_code_token_color(
        &self,
        default_color: Hsla,
        token: MarkdownCodeTokenKind,
        cx: &mut Context<Self>,
    ) -> Hsla {
        let is_dark = cx.theme().mode.is_dark();
        let github = |dark: u32, light: u32| {
            let hex = if is_dark {
                format!("#{dark:06x}")
            } else {
                format!("#{light:06x}")
            };
            Hsla::parse_hex(hex.as_str()).unwrap_or(default_color)
        };

        match token {
            MarkdownCodeTokenKind::Plain => default_color,
            MarkdownCodeTokenKind::Keyword => github(0xff7b72, 0xcf222e),
            MarkdownCodeTokenKind::String => github(0xa5d6ff, 0x0a3069),
            MarkdownCodeTokenKind::Number => github(0x79c0ff, 0x0550ae),
            MarkdownCodeTokenKind::Comment => github(0x8b949e, 0x57606a),
            MarkdownCodeTokenKind::Function => github(0xd2a8ff, 0x8250df),
            MarkdownCodeTokenKind::TypeName => github(0xffa657, 0x953800),
            MarkdownCodeTokenKind::Constant => github(0x79c0ff, 0x0550ae),
            MarkdownCodeTokenKind::Variable => github(0xffa657, 0x953800),
            MarkdownCodeTokenKind::Operator => github(0xff7b72, 0xcf222e),
        }
    }
}
