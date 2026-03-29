use gpui::{Pixels, TextStyle, relative};

use crate::app::theme::{hunk_editor_chrome_colors, hunk_opacity};

impl DiffViewer {
    fn render_review_editor_preview(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.review_editor_session.loading {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading editor preview..."),
                )
                .into_any_element();
        }

        if let Some(error) = self.review_editor_session.error.clone() {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .p_4()
                .child(div().text_sm().text_color(cx.theme().danger).child(error))
                .into_any_element();
        }

        if self.review_editor_session.path.is_none() {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Select a reviewed file to open the editor preview."),
                )
                .into_any_element();
        }

        let is_dark = cx.theme().mode.is_dark();
        let layout = self.diff_column_layout();
        let editor_font_size = cx.theme().mono_font_size * 1.2;

        h_flex()
            .flex_1()
            .min_h_0()
            .child(
                self.render_review_editor_side(
                    "review-editor-left",
                    self.review_editor_session.left_editor.clone(),
                    self.review_editor_session.left_present,
                    "Missing in base",
                    editor_font_size,
                    is_dark,
                    layout.map(|layout| layout.left_panel_width),
                    cx,
                ),
            )
            .child(
                self.render_review_editor_side(
                    "review-editor-right",
                    self.review_editor_session.right_editor.clone(),
                    self.review_editor_session.right_present,
                    "Missing in compare",
                    editor_font_size,
                    is_dark,
                    layout.map(|layout| layout.right_panel_width),
                    cx,
                ),
            )
            .into_any_element()
    }

    fn render_review_editor_side(
        &self,
        element_id: &'static str,
        editor: crate::app::native_files_editor::SharedFilesEditor,
        present: bool,
        missing_message: &'static str,
        editor_font_size: Pixels,
        is_dark: bool,
        width: Option<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor_chrome = hunk_editor_chrome_colors(cx.theme(), is_dark);
        let text_style = TextStyle {
            color: editor_chrome.foreground,
            font_family: cx.theme().mono_font_family.clone(),
            font_size: editor_font_size.into(),
            line_height: relative(1.45),
            ..Default::default()
        };
        let element = crate::app::native_files_editor::FilesEditorElement::new(
            editor.clone(),
            Rc::new(|_, _, _, _| {}),
            false,
            text_style,
            crate::app::native_files_editor::FilesEditorPalette {
                background: editor_chrome.background,
                active_line_background: editor_chrome.active_line,
                line_number: editor_chrome.line_number,
                current_line_number: editor_chrome.active_line_number,
                border: hunk_opacity(cx.theme().border, is_dark, 0.92, 0.78),
                default_foreground: editor_chrome.foreground,
                muted_foreground: editor_chrome.line_number,
                selection_background: editor_chrome.selection,
                cursor: cx.theme().primary,
                invisible: editor_chrome.invisible,
                indent_guide: editor_chrome.indent_guide,
                fold_marker: editor_chrome.line_number,
                current_scope: editor_chrome.current_scope,
                bracket_match: editor_chrome.bracket_match,
                diagnostic_error: cx.theme().danger,
                diagnostic_warning: cx.theme().warning,
                diagnostic_info: cx.theme().accent,
                diff_addition: cx.theme().success,
                diff_deletion: cx.theme().danger,
                diff_modification: cx.theme().warning,
            },
        );

        div()
            .flex_1()
            .min_h_0()
            .relative()
            .when_some(width, |this, width| {
                this.w(width).min_w(width).max_w(width).flex_none()
            })
            .bg(editor_chrome.background)
            .on_scroll_wheel(move |event, _, cx| {
                let line_height = (editor_font_size * 1.45).max(px(14.0));
                if let Some((direction, line_count)) =
                    crate::app::native_files_editor::scroll_direction_and_count(event, line_height)
                {
                    editor.borrow_mut().scroll_lines(line_count, direction);
                    cx.stop_propagation();
                    cx.notify();
                }
            })
            .child(div().id(element_id).size_full().child(element))
            .when(!present, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_2()
                        .right_2()
                        .px_2()
                        .py_1()
                        .rounded(px(6.0))
                        .bg(hunk_opacity(editor_chrome.line_number, is_dark, 0.14, 0.10))
                        .text_xs()
                        .text_color(editor_chrome.line_number)
                        .child(missing_message),
                )
            })
            .into_any_element()
    }
}
