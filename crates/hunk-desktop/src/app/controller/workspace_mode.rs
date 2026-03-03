impl DiffViewer {
    pub(super) fn toggle_jj_terms_glossary(&mut self, cx: &mut Context<Self>) {
        self.show_jj_terms_glossary = !self.show_jj_terms_glossary;
        cx.notify();
    }

    pub(super) fn pending_bookmark_switch(&self) -> Option<&PendingBookmarkSwitch> {
        self.pending_bookmark_switch.as_ref()
    }

    pub(super) fn request_activate_or_create_bookmark_with_dirty_guard(
        &mut self,
        bookmark_name: String,
        cx: &mut Context<Self>,
    ) {
        let target_bookmark = bookmark_name.trim().to_string();
        if target_bookmark.is_empty() {
            self.git_status_message = Some("Bookmark name is required.".to_string());
            cx.notify();
            return;
        }
        if self.git_action_loading {
            self.git_status_message = Some("Wait for the current workspace action to finish.".to_string());
            cx.notify();
            return;
        }
        self.graph_right_panel_mode = GraphRightPanelMode::ActiveWorkflow;

        let source_bookmark = self
            .checked_out_bookmark_name()
            .unwrap_or(self.branch_name.as_str())
            .to_string();
        let same_bookmark = source_bookmark == target_bookmark;
        if same_bookmark {
            self.pending_bookmark_switch = None;
            self.git_status_message =
                Some(format!("Bookmark {} is already active.", target_bookmark));
            cx.notify();
            return;
        }

        if !self.files.is_empty() {
            self.pending_bookmark_switch = Some(PendingBookmarkSwitch {
                source_bookmark: source_bookmark.clone(),
                target_bookmark: target_bookmark.clone(),
                changed_file_count: self.files.len(),
                unix_time: Self::now_unix_seconds(),
            });
            self.graph_right_panel_mode = GraphRightPanelMode::ActiveWorkflow;
            self.branch_picker_open = false;
            self.git_status_message = Some(format!(
                "Switching {} -> {} with {} local files. Choose move or snapshot before switching.",
                source_bookmark,
                target_bookmark,
                self.files.len()
            ));
            cx.notify();
            return;
        }

        self.pending_bookmark_switch = None;
        self.activate_or_create_bookmark(target_bookmark, false, cx);
    }

    pub(super) fn confirm_pending_bookmark_switch_move_changes(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_bookmark_switch.take() else {
            self.git_status_message = Some("No pending bookmark switch to confirm.".to_string());
            cx.notify();
            return;
        };
        self.graph_right_panel_mode = GraphRightPanelMode::ActiveWorkflow;
        self.activate_or_create_bookmark(pending.target_bookmark, true, cx);
    }

    pub(super) fn confirm_pending_bookmark_switch_snapshot(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_bookmark_switch.take() else {
            self.git_status_message = Some("No pending bookmark switch to confirm.".to_string());
            cx.notify();
            return;
        };
        self.graph_right_panel_mode = GraphRightPanelMode::ActiveWorkflow;
        self.activate_or_create_bookmark(pending.target_bookmark, false, cx);
    }

    pub(super) fn cancel_pending_bookmark_switch(&mut self, cx: &mut Context<Self>) {
        if self.pending_bookmark_switch.is_none() {
            return;
        }
        self.pending_bookmark_switch = None;
        self.git_status_message = Some("Canceled bookmark switch.".to_string());
        cx.notify();
    }

    pub(super) fn discard_latest_working_copy_recovery_candidate_for_active_bookmark(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(candidate) = self.latest_working_copy_recovery_candidate_for_active_bookmark() else {
            self.git_status_message =
                Some("No captured working-copy record to discard for this bookmark.".to_string());
            cx.notify();
            return;
        };

        let before_len = self.working_copy_recovery_candidates.len();
        self.working_copy_recovery_candidates
            .retain(|existing| existing.source_revision_id != candidate.source_revision_id);
        let removed = before_len.saturating_sub(self.working_copy_recovery_candidates.len());
        self.git_status_message = Some(format!(
            "Discarded {} captured working-copy record{}.",
            removed,
            if removed == 1 { "" } else { "s" }
        ));
        cx.notify();
    }

    pub(super) fn request_activate_selected_graph_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(bookmark_name) = self.selected_local_graph_bookmark_name() else {
            let message = "Select a local bookmark before activating it.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        };

        self.request_activate_or_create_bookmark_with_dirty_guard(bookmark_name, cx);
    }

    pub(super) fn active_review_action_blocker(&self) -> Option<String> {
        if self.git_action_loading {
            return Some("Another workspace action is in progress.".to_string());
        }
        if !self.can_run_active_bookmark_actions() {
            return Some("Activate a bookmark before opening PR/MR.".to_string());
        }
        if !self.branch_has_upstream {
            return Some("Publish this bookmark before opening PR/MR.".to_string());
        }
        if self.branch_ahead_count == 0 {
            return Some("No new revisions to review for this bookmark.".to_string());
        }
        None
    }

    pub(super) fn selected_graph_review_action_blocker(&self) -> Option<String> {
        if self.git_action_loading {
            return Some("Another workspace action is in progress.".to_string());
        }
        let Some(bookmark) = self.graph_selected_bookmark_ref() else {
            return Some("Select a bookmark in the graph first.".to_string());
        };
        if bookmark.scope != GraphBookmarkScope::Local {
            return Some("Select a local bookmark to open PR/MR.".to_string());
        }
        if bookmark.conflicted {
            return Some("Resolve bookmark conflicts before opening PR/MR.".to_string());
        }
        if !bookmark.tracked {
            return Some("Publish this bookmark before opening PR/MR.".to_string());
        }
        if !bookmark.needs_push {
            return Some("No new revisions to review for this bookmark.".to_string());
        }
        None
    }
}
