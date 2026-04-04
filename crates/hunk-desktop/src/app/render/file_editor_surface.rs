use gpui::{Keystroke, TextStyle, relative};

use crate::app::theme::{hunk_editor_chrome_colors, hunk_opacity};

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
        let is_editor_focused = self.files_editor_focus_handle.is_focused(window);
        let editor_chrome = hunk_editor_chrome_colors(cx.theme(), is_dark);
        let text_style = TextStyle {
            color: editor_chrome.foreground,
            font_family: cx.theme().mono_font_family.clone(),
            font_size: editor_font_size.into(),
            line_height: relative(1.45),
            ..Default::default()
        };
        let editor_element = crate::app::native_files_editor::FilesEditorElement::new(
            self.files_editor.clone(),
            {
                let view = view.clone();
                move |target, position, window, cx| {
                    view.update(cx, |this, cx| {
                        this.files_editor_focus_handle.focus(window, cx);
                        this.open_workspace_text_context_menu(
                            WorkspaceTextContextMenuTarget::FilesEditor(
                                FilesEditorContextMenuTarget {
                                    can_cut: target.can_cut,
                                    can_copy: target.can_copy,
                                    can_paste: target.can_paste,
                                    can_select_all: target.can_select_all,
                                },
                            ),
                            position,
                            cx,
                        );
                    });
                }
            },
            is_editor_focused,
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

        let surface = crate::app::workspace_surface::WorkspaceSurfaceElement::Files(editor_element);

        v_flex()
            .flex_1()
            .min_h_0()
            .bg(editor_chrome.background)
            .key_context("FilesEditor FilesWorkspace")
            .track_focus(&self.files_editor_focus_handle)
            .on_action(cx.listener(Self::files_editor_copy_action))
            .on_action(cx.listener(Self::files_editor_cut_action))
            .on_action(cx.listener(Self::files_editor_paste_action))
            .on_action(cx.listener(Self::files_editor_move_up_action))
            .on_action(cx.listener(Self::files_editor_move_down_action))
            .on_action(cx.listener(Self::files_editor_move_left_action))
            .on_action(cx.listener(Self::files_editor_move_right_action))
            .on_action(cx.listener(Self::files_editor_select_up_action))
            .on_action(cx.listener(Self::files_editor_select_down_action))
            .on_action(cx.listener(Self::files_editor_select_left_action))
            .on_action(cx.listener(Self::files_editor_select_right_action))
            .on_action(cx.listener(Self::files_editor_move_to_beginning_of_line_action))
            .on_action(cx.listener(Self::files_editor_move_to_end_of_line_action))
            .on_action(cx.listener(Self::files_editor_move_to_beginning_of_document_action))
            .on_action(cx.listener(Self::files_editor_move_to_end_of_document_action))
            .on_action(cx.listener(Self::files_editor_select_to_beginning_of_line_action))
            .on_action(cx.listener(Self::files_editor_select_to_end_of_line_action))
            .on_action(cx.listener(Self::files_editor_select_to_beginning_of_document_action))
            .on_action(cx.listener(Self::files_editor_select_to_end_of_document_action))
            .on_action(cx.listener(Self::files_editor_move_to_previous_word_start_action))
            .on_action(cx.listener(Self::files_editor_move_to_next_word_end_action))
            .on_action(cx.listener(Self::files_editor_select_to_previous_word_start_action))
            .on_action(cx.listener(Self::files_editor_select_to_next_word_end_action))
            .on_action(cx.listener(Self::files_editor_page_up_action))
            .on_action(cx.listener(Self::files_editor_page_down_action))
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

                        if uses_files_editor_action_dispatch(&event.keystroke) {
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
            .child(div().flex_1().min_h_0().child(surface))
            .into_any_element()
    }
}

fn uses_files_editor_action_dispatch(keystroke: &Keystroke) -> bool {
    match keystroke.key.as_str() {
        "up" | "down" | "left" | "right" | "home" | "end" => true,
        "pageup" | "pagedown" => {
            !keystroke.modifiers.shift
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.control
                && !keystroke.modifiers.platform
        }
        _ => false,
    }
}
