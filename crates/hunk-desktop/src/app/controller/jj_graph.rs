impl DiffViewer {
    pub(super) fn select_graph_node(&mut self, node_id: String, cx: &mut Context<Self>) {
        self.graph_pending_confirmation = None;
        self.set_graph_selected_node(node_id, false, cx);
    }

    pub(super) fn select_graph_bookmark(
        &mut self,
        node_id: String,
        name: String,
        remote: Option<String>,
        scope: GraphBookmarkScope,
        cx: &mut Context<Self>,
    ) {
        if !self.graph_nodes.iter().any(|node| node.id == node_id) {
            return;
        }
        self.graph_selected_node_id = Some(node_id.clone());
        self.graph_selected_bookmark = Some(GraphBookmarkSelection {
            name,
            remote,
            scope,
        });
        self.graph_pending_confirmation = None;
        self.graph_right_panel_mode = GraphRightPanelMode::SelectedBookmark;
        self.scroll_graph_node_into_view(node_id.as_str());
        cx.notify();
    }

    pub(super) fn activate_graph_bookmark(
        &mut self,
        node_id: String,
        name: String,
        remote: Option<String>,
        scope: GraphBookmarkScope,
        cx: &mut Context<Self>,
    ) {
        self.select_graph_bookmark(node_id, name.clone(), remote, scope, cx);
        if scope != GraphBookmarkScope::Local {
            let message = "Only local bookmarks can be activated for workspace edits.".to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        self.request_activate_or_create_bookmark_with_dirty_guard(name, cx);
    }

    pub(super) fn select_active_graph_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(active_bookmark) = self.graph_active_bookmark.clone() else {
            return;
        };
        let Some((node_id, bookmark_name, bookmark_remote, bookmark_scope)) = self
            .find_graph_bookmark(active_bookmark.as_str(), None)
            .map(|(node_id, bookmark)| {
                (
                    node_id.clone(),
                    bookmark.name.clone(),
                    bookmark.remote.clone(),
                    bookmark.scope,
                )
            })
        else {
            return;
        };
        self.graph_selected_node_id = Some(node_id.clone());
        self.graph_selected_bookmark = Some(GraphBookmarkSelection {
            name: bookmark_name,
            remote: bookmark_remote,
            scope: bookmark_scope,
        });
        self.graph_pending_confirmation = None;
        self.graph_right_panel_mode = GraphRightPanelMode::SelectedBookmark;
        self.scroll_graph_node_into_view(node_id.as_str());
        cx.notify();
    }

    pub(super) fn clear_graph_bookmark_selection(&mut self, cx: &mut Context<Self>) {
        if self.graph_selected_bookmark.is_none() {
            return;
        }
        self.graph_selected_bookmark = None;
        self.graph_pending_confirmation = None;
        self.graph_right_panel_mode = GraphRightPanelMode::ActiveWorkflow;
        cx.notify();
    }

    pub(super) fn set_graph_right_panel_mode_active(&mut self, cx: &mut Context<Self>) {
        if self.graph_right_panel_mode == GraphRightPanelMode::ActiveWorkflow {
            return;
        }
        self.graph_right_panel_mode = GraphRightPanelMode::ActiveWorkflow;
        cx.notify();
    }

    pub(super) fn set_graph_right_panel_mode_selected(&mut self, cx: &mut Context<Self>) {
        if self.graph_right_panel_mode == GraphRightPanelMode::SelectedBookmark {
            return;
        }
        if self.graph_selected_bookmark.is_none() {
            self.git_status_message =
                Some("Select a bookmark chip in the graph before opening bookmark focus mode.".to_string());
            Self::push_warning_notification(
                "Select a bookmark chip in the graph before opening bookmark focus mode."
                    .to_string(),
                cx,
            );
            cx.notify();
            return;
        }
        self.graph_right_panel_mode = GraphRightPanelMode::SelectedBookmark;
        cx.notify();
    }

    pub(super) fn select_graph_focus_revision(&mut self, node_id: String, cx: &mut Context<Self>) {
        if !self
            .graph_focused_revision_ids()
            .iter()
            .any(|focused_id| focused_id == node_id.as_str())
        {
            return;
        }
        self.set_graph_selected_node(node_id, true, cx);
    }

    pub(super) fn graph_selected_node(&self) -> Option<&GraphNode> {
        let selected_id = self.graph_selected_node_id.as_deref()?;
        self.graph_nodes.iter().find(|node| node.id == selected_id)
    }

    pub(super) fn graph_selected_bookmark_ref(&self) -> Option<&GraphBookmarkRef> {
        let selected = self.graph_selected_bookmark.as_ref()?;
        self.graph_nodes
            .iter()
            .flat_map(|node| node.bookmarks.iter())
            .find(|bookmark| Self::graph_bookmark_matches_selection(bookmark, selected))
    }

    pub(super) fn graph_node_is_selected(&self, node_id: &str) -> bool {
        self.graph_selected_node_id.as_deref() == Some(node_id)
    }

    pub(super) fn graph_focused_revision_nodes(&self) -> Vec<&GraphNode> {
        let focused_ids = self.graph_focused_revision_ids();
        focused_ids
            .iter()
            .filter_map(|node_id| self.graph_nodes.iter().find(|node| node.id == *node_id))
            .collect()
    }

    pub(super) fn graph_focused_revision_position(&self) -> Option<usize> {
        let selected_id = self.graph_selected_node_id.as_deref()?;
        let focused_ids = self.graph_focused_revision_ids();
        focused_ids
            .iter()
            .position(|node_id| node_id.as_str() == selected_id)
    }

    pub(super) fn next_bookmark_revision_action(
        &mut self,
        _: &NextBookmarkRevision,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_focused_graph_revision(1, cx);
    }

    pub(super) fn previous_bookmark_revision_action(
        &mut self,
        _: &PreviousBookmarkRevision,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_focused_graph_revision(-1, cx);
    }

    pub(super) fn navigate_focused_graph_revision(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let focused_ids = self.graph_focused_revision_ids();
        if focused_ids.is_empty() {
            return;
        }
        let max_ix = focused_ids.len().saturating_sub(1) as isize;
        let base_ix = self
            .graph_selected_node_id
            .as_deref()
            .and_then(|node_id| {
                focused_ids
                    .iter()
                    .position(|candidate| candidate.as_str() == node_id)
            })
            .unwrap_or(0) as isize;
        let target_ix = (base_ix + delta).clamp(0, max_ix) as usize;
        self.set_graph_selected_node(focused_ids[target_ix].clone(), true, cx);
    }

    pub(super) fn create_graph_bookmark_from_selected_revision(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_node_id) = self.graph_selected_node_id.clone() else {
            self.git_status_message = Some("Select a revision before creating a bookmark.".to_string());
            Self::push_warning_notification(
                "Select a revision before creating a bookmark.".to_string(),
                cx,
            );
            cx.notify();
            return;
        };
        if self.graph_node_is_working_copy_commit(selected_node_id.as_str()) {
            let message =
                "Cannot create a bookmark at the mutable working-copy revision. Select a committed revision."
                    .to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        let input_name = self.graph_action_input_state.read(cx).value().to_string();
        if input_name.trim().is_empty() {
            self.git_status_message = Some("Bookmark name is required.".to_string());
            Self::push_warning_notification("Bookmark name is required.".to_string(), cx);
            cx.notify();
            return;
        }
        let bookmark_name = sanitize_bookmark_name(input_name.as_str());

        self.graph_pending_confirmation = None;
        self.run_git_action("Create bookmark", cx, move |repo_root| {
            create_bookmark_at_revision(&repo_root, &bookmark_name, &selected_node_id)?;
            Ok(format!(
                "Created bookmark {} at {}",
                bookmark_name,
                selected_node_id.chars().take(12).collect::<String>()
            ))
        });
    }

    pub(super) fn fork_graph_bookmark_from_selected_revision(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_node_id) = self.graph_selected_node_id.clone() else {
            self.git_status_message = Some("Select a revision before forking a bookmark.".to_string());
            Self::push_warning_notification(
                "Select a revision before forking a bookmark.".to_string(),
                cx,
            );
            cx.notify();
            return;
        };
        if self.graph_node_is_working_copy_commit(selected_node_id.as_str()) {
            let message =
                "Cannot fork a bookmark at the mutable working-copy revision. Select a committed revision."
                    .to_string();
            self.git_status_message = Some(message.clone());
            Self::push_warning_notification(message, cx);
            cx.notify();
            return;
        }
        let input_name = self.graph_action_input_state.read(cx).value().to_string();
        let inferred_name = self
            .graph_selected_bookmark
            .as_ref()
            .map(|bookmark| format!("{}-fork", bookmark.name))
            .unwrap_or_else(|| "bookmark-fork".to_string());
        let source_name = if input_name.trim().is_empty() {
            inferred_name
        } else {
            input_name
        };
        let mut bookmark_name = sanitize_bookmark_name(source_name.as_str());
        if self
            .graph_selected_bookmark
            .as_ref()
            .is_some_and(|bookmark| bookmark_name == bookmark.name)
        {
            bookmark_name = sanitize_bookmark_name(format!("{bookmark_name}-fork").as_str());
        }

        self.graph_pending_confirmation = None;
        self.run_git_action("Fork bookmark", cx, move |repo_root| {
            create_bookmark_at_revision(&repo_root, &bookmark_name, &selected_node_id)?;
            Ok(format!(
                "Forked bookmark {} at {}",
                bookmark_name,
                selected_node_id.chars().take(12).collect::<String>()
            ))
        });
    }

    pub(super) fn rename_selected_graph_bookmark_from_input(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_bookmark) = self.graph_selected_bookmark.clone() else {
            self.git_status_message = Some("Select a bookmark before renaming.".to_string());
            Self::push_warning_notification("Select a bookmark before renaming.".to_string(), cx);
            cx.notify();
            return;
        };
        if selected_bookmark.scope != GraphBookmarkScope::Local {
            self.git_status_message = Some("Only local bookmarks can be renamed.".to_string());
            Self::push_warning_notification("Only local bookmarks can be renamed.".to_string(), cx);
            cx.notify();
            return;
        }

        let input_name = self.graph_action_input_state.read(cx).value().to_string();
        if input_name.trim().is_empty() {
            self.git_status_message = Some("New bookmark name is required.".to_string());
            Self::push_warning_notification("New bookmark name is required.".to_string(), cx);
            cx.notify();
            return;
        }
        let new_name = sanitize_bookmark_name(input_name.as_str());
        if new_name == selected_bookmark.name {
            self.git_status_message =
                Some("New bookmark name must differ from the current bookmark.".to_string());
            Self::push_warning_notification(
                "New bookmark name must differ from the current bookmark.".to_string(),
                cx,
            );
            cx.notify();
            return;
        }

        let old_name = selected_bookmark.name.clone();
        self.graph_pending_confirmation = None;
        self.run_git_action("Rename bookmark", cx, move |repo_root| {
            rename_bookmark(&repo_root, &old_name, &new_name)?;
            Ok(format!("Renamed bookmark {} to {}", old_name, new_name))
        });
    }

    pub(super) fn arm_move_selected_graph_bookmark_to_selected_revision(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_bookmark) = self.graph_selected_bookmark.clone() else {
            self.git_status_message = Some("Select a bookmark before moving it.".to_string());
            Self::push_warning_notification("Select a bookmark before moving it.".to_string(), cx);
            cx.notify();
            return;
        };
        if selected_bookmark.scope != GraphBookmarkScope::Local {
            self.git_status_message = Some("Only local bookmarks can be moved.".to_string());
            Self::push_warning_notification("Only local bookmarks can be moved.".to_string(), cx);
            cx.notify();
            return;
        }
        let Some(target_node_id) = self.graph_selected_node_id.clone() else {
            self.git_status_message =
                Some("Select a revision before moving bookmark target.".to_string());
            Self::push_warning_notification(
                "Select a revision before moving bookmark target.".to_string(),
                cx,
            );
            cx.notify();
            return;
        };
        self.arm_move_graph_bookmark_to_target(selected_bookmark, target_node_id, cx);
    }

    pub(super) fn cancel_graph_pending_confirmation(&mut self, cx: &mut Context<Self>) {
        if self.graph_pending_confirmation.is_none() {
            return;
        }
        self.graph_pending_confirmation = None;
        cx.notify();
    }

    pub(super) fn confirm_graph_pending_confirmation(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.graph_pending_confirmation.clone() else {
            return;
        };
        self.graph_pending_confirmation = None;

        match pending {
            GraphPendingConfirmation::MoveBookmarkTarget {
                bookmark,
                target_node_id,
            } => {
                let bookmark_name = bookmark.name.clone();
                self.run_git_action("Move bookmark target", cx, move |repo_root| {
                    move_bookmark_to_revision(&repo_root, &bookmark_name, &target_node_id)?;
                    Ok(format!(
                        "Moved bookmark {} to {}. Undo with jj undo or move it again.",
                        bookmark_name,
                        target_node_id.chars().take(12).collect::<String>()
                    ))
                });
            }
        }
    }

    pub(super) fn graph_move_confirmation(&self) -> Option<(&str, &str)> {
        let GraphPendingConfirmation::MoveBookmarkTarget {
            bookmark,
            target_node_id,
        } = self.graph_pending_confirmation.as_ref()?;
        Some((bookmark.name.as_str(), target_node_id.as_str()))
    }

    fn arm_move_graph_bookmark_to_target(
        &mut self,
        bookmark: GraphBookmarkSelection,
        target_node_id: String,
        cx: &mut Context<Self>,
    ) {
        match self.validate_graph_bookmark_drop(&bookmark, target_node_id.as_str()) {
            Ok(()) => {
                self.graph_pending_confirmation = Some(GraphPendingConfirmation::MoveBookmarkTarget {
                    bookmark,
                    target_node_id: target_node_id.clone(),
                });
                self.git_status_message = Some(format!(
                    "Move prepared: confirm retarget bookmark to {}.",
                    target_node_id.chars().take(12).collect::<String>()
                ));
                cx.notify();
            }
            Err(reason) => {
                self.git_status_message = Some(reason.clone());
                Self::push_warning_notification(reason, cx);
                cx.notify();
            }
        }
    }

    fn validate_graph_bookmark_drop(
        &self,
        bookmark: &GraphBookmarkSelection,
        target_node_id: &str,
    ) -> Result<(), String> {
        if self.graph_node_is_working_copy_commit(target_node_id) {
            return Err(
                "Cannot move bookmark to mutable working-copy revision. Select a committed revision instead."
                    .to_string(),
            );
        }
        graph_bookmark_drop_validation(
            &self.graph_nodes,
            bookmark.name.as_str(),
            bookmark.remote.as_deref(),
            bookmark.scope,
            target_node_id,
        )
    }

    fn graph_node_is_working_copy_commit(&self, node_id: &str) -> bool {
        self.graph_working_copy_commit_id.as_deref() == Some(node_id)
    }

    fn graph_focused_revision_ids(&self) -> Vec<String> {
        let Some(selected_bookmark) = self.graph_selected_bookmark.as_ref() else {
            return Vec::new();
        };
        graph_bookmark_revision_chain(
            &self.graph_nodes,
            &self.graph_edges,
            selected_bookmark.name.as_str(),
            selected_bookmark.remote.as_deref(),
            selected_bookmark.scope,
        )
    }

    fn set_graph_selected_node(
        &mut self,
        node_id: String,
        scroll_into_view: bool,
        cx: &mut Context<Self>,
    ) {
        if self.graph_selected_node_id.as_deref() == Some(node_id.as_str()) {
            if scroll_into_view {
                self.scroll_graph_node_into_view(node_id.as_str());
            }
            return;
        }
        if !self.graph_nodes.iter().any(|node| node.id == node_id) {
            return;
        }
        self.graph_selected_node_id = Some(node_id.clone());
        if scroll_into_view {
            self.scroll_graph_node_into_view(node_id.as_str());
        }
        cx.notify();
    }

    fn scroll_graph_node_into_view(&mut self, node_id: &str) {
        let Some(row_ix) = self.graph_nodes.iter().position(|node| node.id == node_id) else {
            return;
        };
        self.graph_list_state.scroll_to(ListOffset {
            item_ix: row_ix,
            offset_in_item: px(0.0),
        });
    }

    fn graph_bookmark_matches_selection(
        bookmark: &GraphBookmarkRef,
        selected: &GraphBookmarkSelection,
    ) -> bool {
        bookmark.name == selected.name
            && bookmark.remote == selected.remote
            && bookmark.scope == selected.scope
    }

    fn graph_bookmark_exists(&self, selected: &GraphBookmarkSelection) -> bool {
        self.graph_nodes
            .iter()
            .flat_map(|node| node.bookmarks.iter())
            .any(|bookmark| Self::graph_bookmark_matches_selection(bookmark, selected))
    }

    fn find_graph_bookmark(
        &self,
        name: &str,
        remote: Option<&str>,
    ) -> Option<(&String, &GraphBookmarkRef)> {
        for node in &self.graph_nodes {
            for bookmark in &node.bookmarks {
                if bookmark.name != name {
                    continue;
                }
                if remote.is_some() && bookmark.remote.as_deref() != remote {
                    continue;
                }
                return Some((&node.id, bookmark));
            }
        }
        None
    }

    fn reconcile_graph_selection_after_snapshot(&mut self) {
        self.graph_selected_node_id = self
            .graph_selected_node_id
            .take()
            .filter(|id| self.graph_nodes.iter().any(|node| node.id == *id))
            .or_else(|| self.graph_working_copy_parent_commit_id.clone())
            .or_else(|| self.graph_nodes.first().map(|node| node.id.clone()));

        self.graph_selected_bookmark = self
            .graph_selected_bookmark
            .take()
            .filter(|selected| self.graph_bookmark_exists(selected));

        self.graph_pending_confirmation =
            self.graph_pending_confirmation
                .take()
                .filter(|pending| match pending {
                    GraphPendingConfirmation::MoveBookmarkTarget {
                        bookmark,
                        target_node_id,
                    } => {
                        bookmark.scope == GraphBookmarkScope::Local
                            && self.graph_bookmark_exists(bookmark)
                            && self.graph_nodes.iter().any(|node| node.id == *target_node_id)
                    }
                });

        if self.graph_selected_bookmark.is_some() {
            let focused_ids = self.graph_focused_revision_ids();
            if let Some(focused_tip_id) = focused_ids.first().cloned() {
                let selected_in_focus = self
                    .graph_selected_node_id
                    .as_deref()
                    .is_some_and(|node_id| {
                        focused_ids
                            .iter()
                            .any(|focused_node_id| focused_node_id.as_str() == node_id)
                    });
                if !selected_in_focus {
                    self.graph_selected_node_id = Some(focused_tip_id);
                }
            }
        }

        if self.graph_right_panel_mode == GraphRightPanelMode::SelectedBookmark
            && self.graph_selected_bookmark.is_none()
        {
            self.graph_right_panel_mode = GraphRightPanelMode::ActiveWorkflow;
        }
    }
}
