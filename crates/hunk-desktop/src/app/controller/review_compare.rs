impl DiffViewer {
    fn subscribe_review_compare_picker_states(&self, cx: &mut Context<Self>) {
        let review_left_picker_state = self.review_left_picker_state.clone();
        cx.subscribe(
            &review_left_picker_state,
            |this, _, event: &SelectEvent<ReviewComparePickerDelegate>, cx| {
                let SelectEvent::Confirm(source_id) = event;
                let Some(source_id) = source_id.clone() else {
                    return;
                };
                this.update_review_compare_selection(Some(source_id), None, cx);
            },
        )
        .detach();

        let review_right_picker_state = self.review_right_picker_state.clone();
        cx.subscribe(
            &review_right_picker_state,
            |this, _, event: &SelectEvent<ReviewComparePickerDelegate>, cx| {
                let SelectEvent::Confirm(source_id) = event;
                let Some(source_id) = source_id.clone() else {
                    return;
                };
                this.update_review_compare_selection(None, Some(source_id), cx);
            },
        )
        .detach();
    }

    fn sync_review_compare_picker_state(
        picker_state: &Entity<SelectState<ReviewComparePickerDelegate>>,
        delegate: ReviewComparePickerDelegate,
        selected_source_id: Option<&str>,
        window: &mut Window,
        cx: &mut App,
    ) {
        picker_state.update(cx, |state, cx| {
            state.set_items(delegate, window, cx);
            if let Some(selected_source_id) = selected_source_id {
                let selected_source_id = selected_source_id.to_string();
                state.set_selected_value(&selected_source_id, window, cx);
            } else {
                state.set_selected_index(None, window, cx);
            }
        });
    }

    fn update_review_compare_picker_states(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delegate = build_review_compare_picker_delegate(&self.review_compare_sources);

        Self::sync_review_compare_picker_state(
            &self.review_left_picker_state,
            delegate.clone(),
            self.review_left_source_id.as_deref(),
            window,
            cx,
        );
        Self::sync_review_compare_picker_state(
            &self.review_right_picker_state,
            delegate,
            self.review_right_source_id.as_deref(),
            window,
            cx,
        );
        cx.notify();
    }

    fn sync_review_compare_picker_states(&mut self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let review_left_picker_state = self.review_left_picker_state.clone();
        let review_right_picker_state = self.review_right_picker_state.clone();
        let delegate = build_review_compare_picker_delegate(&self.review_compare_sources);
        let review_left_source_id = self.review_left_source_id.clone();
        let review_right_source_id = self.review_right_source_id.clone();

        cx.defer(move |cx| {
            if let Err(err) = cx.update_window(window_handle, move |_, window, cx| {
                Self::sync_review_compare_picker_state(
                    &review_left_picker_state,
                    delegate.clone(),
                    review_left_source_id.as_deref(),
                    window,
                    cx,
                );
                Self::sync_review_compare_picker_state(
                    &review_right_picker_state,
                    delegate,
                    review_right_source_id.as_deref(),
                    window,
                    cx,
                );
            }) {
                error!("failed to sync review compare picker state: {err:#}");
            }
        });
    }

    fn review_compare_repo_key(&self) -> Option<String> {
        self.project_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    }

    pub(crate) fn review_compare_source_option(
        &self,
        source_id: &str,
    ) -> Option<&ReviewCompareSourceOption> {
        self.review_compare_sources
            .iter()
            .find(|source| source.id == source_id)
    }

    pub(crate) fn review_compare_source_label(&self, source_id: Option<&str>) -> String {
        source_id
            .and_then(|source_id| self.review_compare_source_option(source_id))
            .map(|source| source.display_name.clone())
            .unwrap_or_else(|| "Select source".to_string())
    }

    pub(crate) fn review_compare_source_detail(&self, source_id: Option<&str>) -> Option<String> {
        source_id
            .and_then(|source_id| self.review_compare_source_option(source_id))
            .map(|source| source.detail.clone())
    }

    fn active_diff_files(&self) -> &[ChangedFile] {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            &self.review_files
        } else {
            &self.files
        }
    }

    pub(crate) fn active_diff_file_line_stats(&self) -> &BTreeMap<String, LineStats> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            &self.review_file_line_stats
        } else {
            &self.file_line_stats
        }
    }

    pub(crate) fn active_diff_file_count(&self) -> usize {
        self.active_diff_files().len()
    }

    pub(crate) fn active_diff_overall_line_stats(&self) -> LineStats {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.review_overall_line_stats
        } else {
            self.overall_line_stats
        }
    }

    fn active_diff_contains_path(&self, path: &str) -> bool {
        self.active_diff_files()
            .iter()
            .any(|file| file.path == path)
    }

    fn active_diff_first_path(&self) -> Option<String> {
        self.active_diff_files().first().map(|file| file.path.clone())
    }

    fn default_review_right_source_id_from_sources(
        &self,
        sources: &[ReviewCompareSourceOption],
    ) -> Option<String> {
        let active_target_id = self.active_workspace_target_id.as_deref()?;
        sources
            .iter()
            .find(|source| source.workspace_target_id.as_deref() == Some(active_target_id))
            .or_else(|| {
                sources
                    .iter()
                    .find(|source| source.workspace_target_id.as_deref() == Some("primary"))
            })
            .or_else(|| sources.first())
            .map(|source| source.id.clone())
    }

    fn default_review_left_source_id_from_sources(
        &self,
        sources: &[ReviewCompareSourceOption],
        right_source_id: Option<&str>,
        default_base_branch_name: Option<&str>,
    ) -> Option<String> {
        if let Some(active_branch_name) = right_source_id
            .and_then(|right_source_id| {
                sources
                    .iter()
                    .find(|source| source.id == right_source_id)
            })
            .and_then(|source| source.branch_name.as_deref())
            .filter(|branch_name| !matches!(*branch_name, "detached" | "unborn"))
            && let Some(source) = sources.iter().find(|source| {
                source.kind == crate::app::review_compare_picker::ReviewCompareSourceKind::Branch
                    && source.branch_name.as_deref() == Some(active_branch_name)
                    && Some(source.id.as_str()) != right_source_id
            })
        {
            return Some(source.id.clone());
        }

        if let Some(default_branch_name) = default_base_branch_name
            && let Some(source) = sources.iter().find(|source| {
                source.kind == crate::app::review_compare_picker::ReviewCompareSourceKind::Branch
                    && source.branch_name.as_deref() == Some(default_branch_name)
                    && Some(source.id.as_str()) != right_source_id
            })
        {
            return Some(source.id.clone());
        }

        for branch_name in ["main", "master"] {
            if let Some(source) = sources.iter().find(|source| {
                source.kind == crate::app::review_compare_picker::ReviewCompareSourceKind::Branch
                    && source.branch_name.as_deref() == Some(branch_name)
                    && Some(source.id.as_str()) != right_source_id
            }) {
                return Some(source.id.clone());
            }
        }

        sources
            .iter()
            .find(|source| {
                source.workspace_target_id.as_deref() == Some("primary")
                    && Some(source.id.as_str()) != right_source_id
            })
            .or_else(|| {
                sources
                    .iter()
                    .find(|source| Some(source.id.as_str()) != right_source_id)
            })
            .map(|source| source.id.clone())
    }

    fn default_review_compare_selection_ids_from_sources(
        &self,
        sources: &[ReviewCompareSourceOption],
        default_base_branch_name: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        let default_right_source_id = self.default_review_right_source_id_from_sources(sources);
        let default_left_source_id = self.default_review_left_source_id_from_sources(
            sources,
            default_right_source_id.as_deref(),
            default_base_branch_name,
        );
        (default_left_source_id, default_right_source_id)
    }

    fn default_review_compare_selection_ids(
        &self,
    ) -> (Option<String>, Option<String>) {
        (
            self.review_default_left_source_id.clone(),
            self.review_default_right_source_id.clone(),
        )
    }

    fn normalize_review_compare_selection_ids(
        sources: &[ReviewCompareSourceOption],
        left_source_id: Option<String>,
        right_source_id: Option<String>,
    ) -> (Option<String>, Option<String>) {
        let contains_id = |candidate: Option<&str>| {
            candidate.is_some_and(|candidate| {
                sources
                    .iter()
                    .any(|source| source.id == candidate)
            })
        };

        let mut left_source_id = left_source_id.filter(|candidate| contains_id(Some(candidate.as_str())));
        let mut right_source_id =
            right_source_id.filter(|candidate| contains_id(Some(candidate.as_str())));

        if right_source_id.is_none() {
            right_source_id = sources.first().map(|source| source.id.clone());
        }
        if left_source_id.is_none() {
            left_source_id = sources
                .iter()
                .find(|source| Some(source.id.as_str()) != right_source_id.as_deref())
                .or_else(|| sources.first())
                .map(|source| source.id.clone());
        }
        if left_source_id == right_source_id
            && let Some(alternative) = sources
                .iter()
                .find(|source| Some(source.id.as_str()) != right_source_id.as_deref())
        {
            left_source_id = Some(alternative.id.clone());
        }

        (left_source_id, right_source_id)
    }

    fn persist_review_compare_selection(&mut self) {
        let Some(repo_key) = self.review_compare_repo_key() else {
            return;
        };

        match (
            self.review_left_source_id.clone(),
            self.review_right_source_id.clone(),
        ) {
            (Some(left_source_id), Some(right_source_id)) => {
                let next = ReviewCompareSelectionState {
                    left_source_id: Some(left_source_id),
                    right_source_id: Some(right_source_id),
                };
                if self
                    .state
                    .review_compare_selection_by_repo
                    .get(repo_key.as_str())
                    == Some(&next)
                {
                    return;
                }
                self.state
                    .review_compare_selection_by_repo
                    .insert(repo_key, next);
            }
            _ => {
                if self
                    .state
                    .review_compare_selection_by_repo
                    .remove(repo_key.as_str())
                    .is_none()
                {
                    return;
                }
            }
        }
        self.persist_state();
    }

    fn refresh_review_compare_sources_from_git_state(&mut self, cx: &mut Context<Self>) {
        let mut seen_ids = BTreeSet::new();
        let mut sources = Vec::new();

        for target in &self.workspace_targets {
            let source = ReviewCompareSourceOption::from_workspace_target(target);
            if seen_ids.insert(source.id.clone()) {
                sources.push(source);
            }
        }

        for branch in &self.branches {
            let source = ReviewCompareSourceOption::from_branch(branch);
            if seen_ids.insert(source.id.clone()) {
                sources.push(source);
            }
        }

        let persisted_selection = self
            .review_compare_repo_key()
            .and_then(|repo_key| {
                self.state
                    .review_compare_selection_by_repo
                    .get(repo_key.as_str())
                    .cloned()
            });
        let default_base_branch_name = self
            .project_path
            .as_deref()
            .and_then(|project_path| resolve_default_base_branch_name(project_path).ok().flatten());
        let (default_left_source_id, default_right_source_id) = self
            .default_review_compare_selection_ids_from_sources(
                &sources,
                default_base_branch_name.as_deref(),
            );
        let left_source_id = self
            .review_left_source_id
            .clone()
            .or_else(|| {
                persisted_selection
                    .as_ref()
                    .and_then(|selection| selection.left_source_id.clone())
            })
            .or(default_left_source_id.clone());
        let right_source_id = self
            .review_right_source_id
            .clone()
            .or_else(|| persisted_selection.and_then(|selection| selection.right_source_id))
            .or(default_right_source_id.clone());
        let (left_source_id, right_source_id) =
            Self::normalize_review_compare_selection_ids(&sources, left_source_id, right_source_id);

        self.review_compare_sources = sources;
        self.review_default_left_source_id = default_left_source_id;
        self.review_default_right_source_id = default_right_source_id;
        self.review_left_source_id = left_source_id;
        self.review_right_source_id = right_source_id;
        self.persist_review_compare_selection();
        self.sync_review_compare_picker_states(cx);
        cx.notify();
    }

    fn selected_review_compare_sources(&self) -> Option<(CompareSource, CompareSource)> {
        let left_source_id = self.review_left_source_id.as_deref()?;
        let right_source_id = self.review_right_source_id.as_deref()?;
        let left_source = self.review_compare_source_option(left_source_id)?;
        let right_source = self.review_compare_source_option(right_source_id)?;
        Some((
            self.review_compare_option_to_git_source(left_source)?,
            self.review_compare_option_to_git_source(right_source)?,
        ))
    }

    fn review_compare_option_to_git_source(
        &self,
        option: &ReviewCompareSourceOption,
    ) -> Option<CompareSource> {
        match option.kind {
            crate::app::review_compare_picker::ReviewCompareSourceKind::WorkspaceTarget => Some(CompareSource::WorkspaceTarget {
                target_id: option.workspace_target_id.clone()?,
                root: option.workspace_root.clone()?,
            }),
            crate::app::review_compare_picker::ReviewCompareSourceKind::Branch => Some(CompareSource::Branch {
                name: option.branch_name.clone()?,
            }),
        }
    }

    fn active_review_compare_is_default_pair(&self) -> bool {
        let (default_left, default_right) = self.default_review_compare_selection_ids();
        self.review_left_source_id == default_left && self.review_right_source_id == default_right
    }

    pub(crate) fn review_compare_reset_available(&self) -> bool {
        !self.review_compare_sources.is_empty() && !self.active_review_compare_is_default_pair()
    }

    pub(crate) fn reset_review_compare_selection(&mut self, cx: &mut Context<Self>) {
        let (default_left_source_id, default_right_source_id) =
            self.default_review_compare_selection_ids();
        self.update_review_compare_selection(default_left_source_id, default_right_source_id, cx);
    }

    pub(crate) fn review_comments_enabled(&self) -> bool {
        self.workspace_view_mode != WorkspaceViewMode::Diff || self.active_review_compare_is_default_pair()
    }

    fn clear_review_compare_loaded_state(&mut self, empty_message: &str, cx: &mut Context<Self>) {
        self.cancel_patch_reload();
        self.review_compare_loading = false;
        self.review_compare_error = None;
        self.review_files.clear();
        self.review_file_status_by_path.clear();
        self.review_file_line_stats.clear();
        self.review_overall_line_stats = LineStats::default();
        self.selected_path = None;
        self.selected_status = None;
        self.comments_cache.clear();
        self.comment_miss_streaks.clear();
        self.reset_comment_row_match_cache();
        self.clear_comment_ui_state();
        self.diff_rows = vec![message_row(DiffRowKind::Empty, empty_message)];
        self.diff_row_metadata.clear();
        self.diff_row_segment_cache.clear();
        self.diff_visible_file_header_lookup.clear();
        self.diff_visible_hunk_header_lookup.clear();
        self.file_row_ranges.clear();
        self.selection_anchor_row = None;
        self.selection_head_row = None;
        self.drag_selecting_rows = false;
        self.invalidate_segment_prefetch();
        self.sync_diff_list_state();
        self.recompute_diff_layout();
        self.request_repo_tree_reload(cx);
        cx.notify();
    }

    fn request_review_compare_refresh(&mut self, cx: &mut Context<Self>) {
        let Some(primary_repo_root) = self.project_path.clone() else {
            self.clear_review_compare_loaded_state("Open a Git repository to compare workspaces.", cx);
            return;
        };
        let Some((left_source, right_source)) = self.selected_review_compare_sources() else {
            self.clear_review_compare_loaded_state("Select two compare sources.", cx);
            return;
        };

        let previous_review_line_stats = self.review_file_line_stats.clone();
        let collapsed_files = self.collapsed_files.clone();
        let left_source_id = self.review_left_source_id.clone();
        let right_source_id = self.review_right_source_id.clone();
        let epoch = self.next_patch_epoch();

        self.review_compare_loading = true;
        self.review_compare_error = None;
        self.patch_loading = false;
        self.diff_rows = vec![message_row(DiffRowKind::Meta, "Loading comparison...")];
        self.diff_row_metadata.clear();
        self.diff_row_segment_cache.clear();
        self.diff_visible_file_header_lookup.clear();
        self.diff_visible_hunk_header_lookup.clear();
        self.file_row_ranges.clear();
        self.selection_anchor_row = None;
        self.selection_head_row = None;
        self.drag_selecting_rows = false;
        self.invalidate_segment_prefetch();
        self.sync_diff_list_state();
        self.recompute_diff_layout();

        self.patch_task = cx.spawn(async move |this, cx| {
            let started_at = Instant::now();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let snapshot =
                        load_compare_snapshot(primary_repo_root.as_path(), &left_source, &right_source)?;
                    let stream = build_diff_stream_from_patch_map(
                        &snapshot.files,
                        &collapsed_files,
                        &previous_review_line_stats,
                        &snapshot.patches_by_path,
                        &BTreeSet::new(),
                    );
                    Ok::<_, anyhow::Error>((snapshot, stream))
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.patch_epoch {
                        return;
                    }

                    this.review_compare_loading = false;
                    match result {
                        Ok((snapshot, stream)) => {
                            debug!(
                                left = left_source_id.as_deref().unwrap_or("unknown"),
                                right = right_source_id.as_deref().unwrap_or("unknown"),
                                files = snapshot.files.len(),
                                changed = snapshot.overall_line_stats.changed(),
                                elapsed_ms = started_at.elapsed().as_millis(),
                                "review compare snapshot loaded"
                            );
                            this.apply_loaded_review_compare_stream(snapshot, stream, cx);
                        }
                        Err(err) => {
                            error!(
                                left = left_source_id.as_deref().unwrap_or("unknown"),
                                right = right_source_id.as_deref().unwrap_or("unknown"),
                                elapsed_ms = started_at.elapsed().as_millis(),
                                "review compare snapshot failed: {err:#}"
                            );
                            this.review_compare_error = Some(Self::format_error_chain(&err));
                            this.clear_review_compare_loaded_state(
                                "Failed to load comparison.",
                                cx,
                            );
                            this.review_compare_error = Some(Self::format_error_chain(&err));
                        }
                    }
                });
            }
        });
    }

    fn apply_loaded_review_compare_stream(
        &mut self,
        snapshot: hunk_git::compare::CompareSnapshot,
        stream: DiffStream,
        cx: &mut Context<Self>,
    ) {
        self.review_compare_error = None;
        self.review_files = snapshot.files;
        self.review_file_status_by_path = self
            .review_files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.review_file_line_stats = snapshot.file_line_stats;
        self.review_overall_line_stats = snapshot.overall_line_stats;
        self.collapsed_files
            .retain(|path| self.review_files.iter().any(|file| file.path == *path));

        self.invalidate_segment_prefetch();
        self.diff_rows = stream.rows;
        self.diff_row_metadata = stream.row_metadata;
        self.diff_row_segment_cache = stream.row_segments;
        self.clamp_comment_rows_to_diff();
        self.clamp_selection_to_rows();
        self.drag_selecting_rows = false;
        self.sync_diff_list_state();
        self.file_row_ranges = stream.file_ranges;
        self.recompute_diff_layout();

        let has_selection = self
            .selected_path
            .as_ref()
            .is_some_and(|path| self.active_diff_contains_path(path.as_str()));
        if !has_selection {
            self.selected_path = self.active_diff_first_path();
        }
        self.selected_status = self
            .selected_path
            .as_deref()
            .and_then(|selected| self.status_for_path(selected));
        self.last_visible_row_start = None;
        self.recompute_diff_visible_header_lookup();
        self.refresh_comments_cache_from_store();
        self.rebuild_comment_row_match_cache();
        if self.review_comments_enabled() {
            self.reconcile_comments_with_loaded_diff();
        }

        if self.scroll_selected_after_reload {
            self.scroll_selected_file_to_top();
            self.scroll_selected_after_reload = false;
        }

        self.request_repo_tree_reload(cx);
        cx.notify();
    }

    fn update_review_compare_selection(
        &mut self,
        next_left_source_id: Option<String>,
        next_right_source_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let left_source_id = next_left_source_id.or_else(|| self.review_left_source_id.clone());
        let right_source_id = next_right_source_id.or_else(|| self.review_right_source_id.clone());
        let (left_source_id, right_source_id) = Self::normalize_review_compare_selection_ids(
            &self.review_compare_sources,
            left_source_id,
            right_source_id,
        );
        if self.review_left_source_id == left_source_id
            && self.review_right_source_id == right_source_id
        {
            return;
        }

        self.review_left_source_id = left_source_id;
        self.review_right_source_id = right_source_id;
        self.persist_review_compare_selection();
        self.sync_review_compare_picker_states(cx);
        self.comments_cache.clear();
        self.comment_miss_streaks.clear();
        self.reset_comment_row_match_cache();
        self.clear_comment_ui_state();
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.scroll_selected_after_reload = true;
            self.request_review_compare_refresh(cx);
        } else {
            cx.notify();
        }
    }
}
