impl DiffViewer {
    pub(crate) fn ai_thread_is_bookmarked(&self, thread_id: &str) -> bool {
        crate::app::ai_bookmarks::thread_is_bookmarked(
            &self.state.ai_bookmarked_thread_ids,
            thread_id,
        )
    }

    pub(super) fn ai_toggle_thread_bookmark_action(
        &mut self,
        thread_id: String,
        cx: &mut Context<Self>,
    ) {
        let visible_threads = self.ai_visible_threads();
        let is_visible_thread = crate::app::ai_bookmarks::visible_threads_contain_thread(
            visible_threads.as_slice(),
            thread_id.as_str(),
        );
        if !is_visible_thread {
            return;
        }

        let changed = if self.ai_thread_is_bookmarked(thread_id.as_str()) {
            self.state.ai_bookmarked_thread_ids.remove(thread_id.as_str())
        } else {
            self.state.ai_bookmarked_thread_ids.insert(thread_id)
        };

        if !changed {
            return;
        }

        self.persist_state();
        self.rebuild_ai_thread_sidebar_state();
        cx.notify();
    }
}
