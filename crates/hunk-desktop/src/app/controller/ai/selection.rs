impl DiffViewer {
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
        cx.notify();
        true
    }

    pub(super) fn ai_copy_message_action(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(message));
        gpui_component::WindowExt::push_notification(
            window,
            gpui_component::notification::Notification::success("Copied message."),
            cx,
        );
        cx.notify();
    }
}
