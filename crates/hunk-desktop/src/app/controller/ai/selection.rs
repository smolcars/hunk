impl DiffViewer {
    pub(super) fn ai_set_pressed_markdown_link(
        &mut self,
        pressed_link: Option<AiPressedMarkdownLink>,
    ) {
        self.ai_pressed_markdown_link = pressed_link;
    }

    pub(super) fn ai_mark_pressed_markdown_link_dragged(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
    ) {
        let Some(pressed_link) = self.ai_pressed_markdown_link.as_mut() else {
            return;
        };
        let drag_delta = position - pressed_link.mouse_down_position;
        if drag_delta.x.abs() > px(3.0) || drag_delta.y.abs() > px(3.0) {
            pressed_link.dragged = true;
        }
    }

    pub(super) fn ai_take_pressed_markdown_link(&mut self) -> Option<AiPressedMarkdownLink> {
        self.ai_pressed_markdown_link.take()
    }

    pub(super) fn ai_copy_text_action(
        &mut self,
        text: String,
        success_message: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        gpui_component::WindowExt::push_notification(
            window,
            crate::app::notifications::success(success_message),
            cx,
        );
        cx.notify();
    }

    pub(super) fn ai_text_selection_range_for_surface(
        &self,
        surface_id: &str,
    ) -> Option<std::ops::Range<usize>> {
        self.ai_text_selection
            .as_ref()
            .and_then(|selection| selection.range_for_surface(surface_id))
    }

    pub(super) fn ai_begin_text_selection(
        &mut self,
        row_id: String,
        selection_surfaces: std::sync::Arc<[AiTextSelectionSurfaceSpec]>,
        surface_id: &str,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.ai_text_selection = Some(AiTextSelection::new(
            row_id,
            selection_surfaces.as_ref(),
            surface_id,
            index,
        ));
        self.ai_sync_primary_text_selection(cx);
        cx.notify();
    }

    pub(super) fn ai_update_text_selection(
        &mut self,
        surface_id: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.ai_text_selection.as_mut() else {
            return;
        };
        if !selection.dragging {
            return;
        }

        let previous_range = selection.range();
        selection.set_head_for_surface(surface_id, index);
        if selection.range() != previous_range {
            self.ai_sync_primary_text_selection(cx);
            cx.notify();
        }
    }

    pub(super) fn ai_end_text_selection(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.ai_text_selection.as_mut() else {
            return;
        };
        if !selection.dragging {
            return;
        }

        selection.dragging = false;
        cx.notify();
    }

    pub(super) fn ai_clear_text_selection(&mut self, cx: &mut Context<Self>) {
        if self.ai_text_selection.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn ai_clear_text_selection_for_rows(
        &mut self,
        row_ids: &BTreeSet<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.ai_text_selection.as_ref() else {
            return;
        };
        if row_ids.contains(selection.row_id.as_str()) {
            self.ai_clear_text_selection(cx);
        }
    }

    pub(super) fn ai_copy_selected_text(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(selection_text) = self
            .ai_text_selection
            .as_ref()
            .and_then(AiTextSelection::selected_text)
        else {
            return false;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(selection_text));
        true
    }

    pub(super) fn ai_select_all_text(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.ai_text_selection.as_mut() else {
            return false;
        };
        if selection.full_text.is_empty() {
            return false;
        }

        selection.select_all();
        self.ai_sync_primary_text_selection(cx);
        cx.notify();
        true
    }

    pub(super) fn ai_copy_message_action(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_copy_text_action(message, "Copied message.", window, cx);
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn ai_sync_primary_text_selection(&self, cx: &mut Context<Self>) {
        let Some(selection_text) = self
            .ai_text_selection
            .as_ref()
            .and_then(AiTextSelection::selected_text)
        else {
            return;
        };

        cx.write_to_primary(ClipboardItem::new_string(selection_text));
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    fn ai_sync_primary_text_selection(&self, _: &mut Context<Self>) {}
}
