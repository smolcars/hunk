fn next_review_workspace_search_target(
    matches: &[crate::app::review_workspace_session::ReviewWorkspaceSearchTarget],
    current_excerpt_id: Option<hunk_editor::WorkspaceExcerptId>,
    current_row: usize,
    forward: bool,
) -> Option<crate::app::review_workspace_session::ReviewWorkspaceSearchTarget> {
    if matches.is_empty() {
        return None;
    }

    let current_surface_order = current_excerpt_id.and_then(|excerpt_id| {
        matches
            .iter()
            .find(|candidate| candidate.excerpt_id == excerpt_id)
            .map(|candidate| candidate.surface_order)
    });

    if forward {
        matches
            .iter()
            .find(|target| {
                Some(target.excerpt_id) == current_excerpt_id && target.row_index > current_row
                    || current_surface_order
                        .is_some_and(|surface_order| target.surface_order > surface_order)
            })
            .or_else(|| matches.first())
            .cloned()
    } else {
        matches
            .iter()
            .rev()
            .find(|target| {
                Some(target.excerpt_id) == current_excerpt_id && target.row_index < current_row
                    || current_surface_order
                        .is_some_and(|surface_order| target.surface_order < surface_order)
            })
            .or_else(|| matches.last())
            .cloned()
    }
}

impl DiffViewer {
    pub(crate) fn active_editor_search_match_count(&self) -> usize {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self.review_surface.workspace_search_matches.len();
        }

        self.files_editor.borrow().search_match_count()
    }

    fn sync_review_workspace_search_query(&mut self, query: Option<&str>) {
        let Some(session) = self.review_workspace_session.as_ref() else {
            self.review_surface.clear_workspace_search_matches();
            self.review_surface.clear_workspace_surface_snapshot();
            return;
        };
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            if let Some(editor) = self.review_surface.left_workspace_editor.as_ref() {
                editor.borrow_mut().set_search_query(None);
            }
            if let Some(editor) = self.review_surface.right_workspace_editor.as_ref() {
                editor.borrow_mut().set_search_query(None);
            }
            self.review_surface.clear_workspace_search_matches();
            let _ = self.rebuild_review_surface_display_rows();
            return;
        };
        if let Some(editor) = self.review_surface.left_workspace_editor.as_ref() {
            editor.borrow_mut().set_search_query(Some(query));
        }
        if let Some(editor) = self.review_surface.right_workspace_editor.as_ref() {
            editor.borrow_mut().set_search_query(Some(query));
        }

        let editor_matches = self
            .review_surface
            .right_workspace_editor
            .as_ref()
            .and_then(|editor| editor.borrow().workspace_search_matches(query));
        if let Some(matches) = editor_matches {
            self.review_surface.workspace_search_matches = matches
                .iter()
                .filter_map(|target| {
                    session.review_search_target_for_workspace_match(
                        target.path.as_path(),
                        target.excerpt_id,
                        target.surface_order,
                        target.start,
                        target.end,
                    )
                })
                .collect();
            self.review_surface.workspace_editor_search_matches = matches;
            let _ = self.rebuild_review_surface_display_rows();
            return;
        }

        self.review_surface.workspace_search_matches = session.workspace_search_matches(query);
        self.review_surface.workspace_editor_search_matches.clear();
        let _ = self.rebuild_review_surface_display_rows();
    }

    pub(super) fn sync_editor_search_query(&mut self, cx: &mut Context<Self>) {
        let query = if self.editor_search_visible {
            self.editor_search_input_state.read(cx).value().to_string()
        } else {
            String::new()
        };

        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.sync_review_workspace_search_query(Some(query.as_str()));
        } else {
            self.files_editor
                .borrow_mut()
                .set_search_query(Some(query.as_str()));
        }
        cx.notify();
    }

    pub(super) fn toggle_editor_search(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_search_visible = visible;
        if visible {
            self.editor_search_input_state.update(cx, |state, cx| {
                state.focus(window, cx);
            });
            self.sync_editor_search_query(cx);
        } else {
            self.editor_search_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
            self.editor_replace_input_state.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
            self.review_surface.clear_workspace_search_matches();
            if let Some(editor) = self.review_surface.left_workspace_editor.as_ref() {
                editor.borrow_mut().set_search_query(None);
            }
            if let Some(editor) = self.review_surface.right_workspace_editor.as_ref() {
                editor.borrow_mut().set_search_query(None);
            }
            self.files_editor.borrow_mut().set_search_query(None);
            if self.workspace_view_mode == WorkspaceViewMode::Diff {
                let _ = self.rebuild_review_surface_display_rows();
                self.focus_handle.focus(window, cx);
            } else {
                self.files_editor_focus_handle.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn toggle_editor_search_visibility(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_editor_search(!self.editor_search_visible, window, cx);
    }

    pub(super) fn navigate_editor_search(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            let editor_matches = self.review_surface.workspace_editor_search_matches.clone();
            let selected_target = self
                .review_surface
                .right_workspace_editor
                .as_ref()
                .and_then(|editor| {
                    editor
                        .borrow_mut()
                        .select_next_workspace_search_target(&editor_matches, forward)
                });
            if let Some(target) = selected_target {
                if let Some(left_editor) = self.review_surface.left_workspace_editor.as_ref() {
                    let _ = left_editor
                        .borrow_mut()
                        .activate_workspace_excerpt(target.excerpt_id);
                }
                if let Some(session) = self.review_workspace_session.as_ref()
                    && let Some(review_target) = session.review_search_target_for_workspace_match(
                        target.path.as_path(),
                        target.excerpt_id,
                        target.surface_order,
                        target.start,
                        target.end,
                    )
                {
                    self.select_row_and_scroll(review_target.row_index, false, cx);
                    return;
                }
            }

            let current_excerpt_id = self
                .review_surface
                .left_workspace_editor
                .as_ref()
                .and_then(|editor| editor.borrow().active_workspace_excerpt_id());
            let current_row = self.current_review_surface_row().unwrap_or(0);
            if let Some(target) = next_review_workspace_search_target(
                &self.review_surface.workspace_search_matches,
                current_excerpt_id,
                current_row,
                forward,
            ) {
                self.select_row_and_scroll(target.row_index, false, cx);
            }
            return;
        }

        if self.files_editor.borrow_mut().select_next_search_match(forward) {
            cx.notify();
        }
    }

    pub(super) fn replace_current_editor_search_match(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            if let Some(window) = window {
                self.focus_handle.focus(window, cx);
            }
            return;
        }

        let replacement = self.editor_replace_input_state.read(cx).value().to_string();
        if self
            .files_editor
            .borrow_mut()
            .replace_selected_search_match(replacement.as_str())
        {
            self.sync_editor_dirty_from_input(cx);
            let _ = self.files_editor.borrow_mut().select_next_search_match(true);
            if let Some(window) = window {
                self.files_editor_focus_handle.focus(window, cx);
            }
            self.sync_active_file_editor_tab_state();
            cx.notify();
        }
    }

    pub(super) fn replace_all_editor_search_matches(&mut self, cx: &mut Context<Self>) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return;
        }

        let replacement = self.editor_replace_input_state.read(cx).value().to_string();
        if self
            .files_editor
            .borrow_mut()
            .replace_all_search_matches(replacement.as_str())
        {
            self.sync_editor_dirty_from_input(cx);
            self.sync_active_file_editor_tab_state();
            cx.notify();
        }
    }
}

#[cfg(test)]
mod editor_search_tests {
    use crate::app::review_workspace_session::ReviewWorkspaceSearchTarget;
    use hunk_editor::WorkspaceExcerptId;

    use super::next_review_workspace_search_target;

    #[test]
    fn next_review_workspace_search_target_advances_within_excerpt_before_wrapping() {
        let matches = vec![
            ReviewWorkspaceSearchTarget {
                path: "a.rs".to_string(),
                excerpt_id: WorkspaceExcerptId::new(1),
                surface_order: 0,
                row_index: 10,
                raw_column_range: None,
            },
            ReviewWorkspaceSearchTarget {
                path: "a.rs".to_string(),
                excerpt_id: WorkspaceExcerptId::new(1),
                surface_order: 0,
                row_index: 20,
                raw_column_range: None,
            },
            ReviewWorkspaceSearchTarget {
                path: "b.rs".to_string(),
                excerpt_id: WorkspaceExcerptId::new(2),
                surface_order: 1,
                row_index: 30,
                raw_column_range: None,
            },
        ];

        let next = next_review_workspace_search_target(
            &matches,
            Some(WorkspaceExcerptId::new(1)),
            10,
            true,
        )
        .expect("next match");

        assert_eq!(next.row_index, 20);
    }
}
