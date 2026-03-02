impl DiffViewer {
    pub(super) fn redo_last_operation(&mut self, cx: &mut Context<Self>) {
        if !self.can_redo_operation {
            self.git_status_message = Some("No undone operation is available to redo.".to_string());
            cx.notify();
            return;
        }

        if self.prevent_unsaved_editor_discard(None, cx) {
            return;
        }

        self.run_git_action("Redo operation", cx, move |repo_root| {
            redo_last_jj_operation(&repo_root)?;
            Ok("Redid the latest undone operation".to_string())
        });
    }
}
