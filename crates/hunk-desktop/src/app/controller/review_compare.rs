#[derive(Clone, Copy)]
struct LoadedReviewCompareReuseState<'a, F> {
    has_loaded_session: bool,
    review_compare_loading: bool,
    review_compare_error: Option<&'a str>,
    current_left_source_id: Option<&'a str>,
    current_right_source_id: Option<&'a str>,
    loaded_left_source_id: Option<&'a str>,
    loaded_right_source_id: Option<&'a str>,
    current_collapsed_files: &'a BTreeSet<String>,
    loaded_collapsed_files: &'a BTreeSet<String>,
    current_snapshot_fingerprint: Option<&'a F>,
    loaded_snapshot_fingerprint: Option<&'a F>,
}

fn should_reuse_loaded_review_compare<F: PartialEq>(
    state: LoadedReviewCompareReuseState<'_, F>,
) -> bool {
    state.has_loaded_session
        && !state.review_compare_loading
        && state.review_compare_error.is_none()
        && state.current_left_source_id == state.loaded_left_source_id
        && state.current_right_source_id == state.loaded_right_source_id
        && state.current_collapsed_files == state.loaded_collapsed_files
        && state.current_snapshot_fingerprint == state.loaded_snapshot_fingerprint
}

fn preferred_review_workspace_path_for_session(
    current_editor_path: Option<&str>,
    current_surface_path: Option<&str>,
    current_range_path: Option<&str>,
    last_selected_path: Option<&str>,
    session: &crate::app::review_workspace_session::ReviewWorkspaceSession,
) -> Option<String> {
    current_editor_path
        .filter(|path| session.contains_path(path))
        .map(str::to_string)
        .or_else(|| current_surface_path.map(str::to_string))
        .or_else(|| current_range_path.map(str::to_string))
        .or_else(|| {
            last_selected_path
                .filter(|path| session.contains_path(path))
                .map(str::to_string)
        })
        .or_else(|| session.first_path().map(ToString::to_string))
}

fn review_compare_branch_source_id(
    sources: &[ReviewCompareSourceOption],
    branch_name: &str,
    excluded_source_id: Option<&str>,
) -> Option<String> {
    sources
        .iter()
        .find(|source| {
            source.kind == crate::app::review_compare_picker::ReviewCompareSourceKind::Branch
                && source.branch_name.as_deref() == Some(branch_name)
                && Some(source.id.as_str()) != excluded_source_id
        })
        .map(|source| source.id.clone())
}

fn review_compare_selection_ids_for_workspace_root(
    sources: &[ReviewCompareSourceOption],
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    workspace_root: &std::path::Path,
    preferred_base_branch_name: Option<&str>,
    default_base_branch_name: Option<&str>,
) -> Option<(Option<String>, Option<String>)> {
    let target = workspace_targets
        .iter()
        .find(|target| target.root.as_path() == workspace_root)?;
    let right_source_id = sources
        .iter()
        .find(|source| source.workspace_target_id.as_deref() == Some(target.id.as_str()))
        .map(|source| source.id.clone())?;

    let preferred_base_branch_name = preferred_base_branch_name
        .map(str::trim)
        .filter(|branch_name| !branch_name.is_empty());
    let default_base_branch_name = default_base_branch_name
        .map(str::trim)
        .filter(|branch_name| !branch_name.is_empty());

    let left_source_id = preferred_base_branch_name
        .and_then(|branch_name| {
            review_compare_branch_source_id(sources, branch_name, Some(right_source_id.as_str()))
        })
        .or_else(|| {
            default_base_branch_name.and_then(|branch_name| {
                review_compare_branch_source_id(sources, branch_name, Some(right_source_id.as_str()))
            })
        })
        .or_else(|| {
            if matches!(target.branch_name.as_str(), "detached" | "unborn") {
                None
            } else {
                review_compare_branch_source_id(
                    sources,
                    target.branch_name.as_str(),
                    Some(right_source_id.as_str()),
                )
            }
        })
        .or_else(|| {
            sources
                .iter()
                .find(|source| {
                    source.workspace_target_id.as_deref()
                        == Some(hunk_git::worktree::PRIMARY_WORKSPACE_TARGET_ID)
                        && source.id != right_source_id
                })
                .map(|source| source.id.clone())
        })
        .or_else(|| {
            sources
                .iter()
                .find(|source| source.id != right_source_id)
                .map(|source| source.id.clone())
        });

    Some(DiffViewer::normalize_review_compare_selection_ids(
        sources,
        left_source_id,
        Some(right_source_id),
    ))
}

fn selected_git_workspace_review_compare_selection_ids(
    sources: &[ReviewCompareSourceOption],
    workspace_targets: &[hunk_git::worktree::WorkspaceTargetSummary],
    workspace_root: Option<&std::path::Path>,
    default_base_branch_name: Option<&str>,
) -> Option<(Option<String>, Option<String>)> {
    let workspace_root = workspace_root?;
    review_compare_selection_ids_for_workspace_root(
        sources,
        workspace_targets,
        workspace_root,
        None,
        default_base_branch_name,
    )
}

fn update_persisted_review_compare_selection(
    persist_selection: bool,
    selections: &mut BTreeMap<String, ReviewCompareSelectionState>,
    repo_key: Option<&str>,
    left_source_id: Option<String>,
    right_source_id: Option<String>,
) -> bool {
    if !persist_selection {
        return false;
    }

    let Some(repo_key) = repo_key else {
        return false;
    };

    match (left_source_id, right_source_id) {
        (Some(left_source_id), Some(right_source_id)) => {
            let next = ReviewCompareSelectionState {
                left_source_id: Some(left_source_id),
                right_source_id: Some(right_source_id),
            };
            if selections.get(repo_key) == Some(&next) {
                return false;
            }
            selections.insert(repo_key.to_string(), next);
            true
        }
        _ => selections.remove(repo_key).is_some(),
    }
}

impl DiffViewer {
    pub(crate) fn current_review_editor_path(&self) -> Option<String> {
        self.review_surface
            .left_workspace_editor()
            .and_then(|editor| editor.borrow().active_workspace_path_buf())
            .map(|path| path.to_string_lossy().to_string())
    }

    fn sync_review_workspace_editor_selection_for_path(&mut self, path: Option<&str>) {
        let Some(path) = path else {
            return;
        };
        if let Some(editor) = self.review_surface.left_workspace_editor() {
            let _ = editor
                .borrow_mut()
                .activate_workspace_path(std::path::Path::new(path));
        }
        if let Some(editor) = self.review_surface.right_workspace_editor() {
            let _ = editor
                .borrow_mut()
                .activate_workspace_path(std::path::Path::new(path));
        }
    }

    pub(crate) fn sync_review_workspace_editor_selection_for_row(&mut self, row_ix: usize) {
        let Some(session) = self.review_workspace_session.as_ref() else {
            return;
        };
        if let Some(excerpt_id) = session.excerpt_id_at_surface_row(row_ix)
        {
            let mut handled = false;
            if let Some(editor) = self.review_surface.left_workspace_editor() {
                handled |= editor
                    .borrow_mut()
                    .activate_workspace_excerpt(excerpt_id)
                    .ok()
                    == Some(true);
            }
            if let Some(editor) = self.review_surface.right_workspace_editor() {
                handled |= editor
                    .borrow_mut()
                    .activate_workspace_excerpt(excerpt_id)
                    .ok()
                    == Some(true);
            }
            if handled {
                return;
            }
        }
        if let Some(path) = session
            .path_at_surface_row(row_ix)
            .map(ToString::to_string)
        {
            self.sync_review_workspace_editor_selection_for_path(Some(path.as_str()));
        }
    }

    pub(crate) fn set_review_selected_file(
        &mut self,
        path: Option<String>,
        status: Option<FileStatus>,
    ) {
        self.sync_review_workspace_editor_selection_for_path(path.as_deref());
        self.review_surface.selected_path = path;
        let _ = status;
    }

    pub(crate) fn current_review_surface_row(&self) -> Option<usize> {
        let session = self.review_workspace_session.as_ref()?;
        let row_count = session.row_count();
        if row_count == 0 {
            return None;
        }
        let max_ix = row_count.saturating_sub(1);
        self.review_surface.selection_head_row
            .or_else(|| {
                self.review_surface
                    .last_surface_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.visible_state.top_row)
            })
            .map(|row_ix| row_ix.min(max_ix))
            .or_else(|| self.current_review_surface_top_row())
    }

    pub(crate) fn current_review_file_range(&self) -> Option<FileRowRange> {
        let session = self.review_workspace_session.as_ref()?;
        self.current_review_editor_path()
            .as_deref()
            .and_then(|path| session.file_range_for_path(path))
            .map(|range| FileRowRange {
                path: range.path.clone(),
                status: range.status,
                start_row: range.start_row,
            })
            .or_else(|| {
                self.current_review_surface_row()
                    .and_then(|row_ix| session.file_at_or_after_surface_row(row_ix))
                    .map(|range| FileRowRange {
                        path: range.path.clone(),
                        status: range.status,
                        start_row: range.start_row,
                    })
            })
            .or_else(|| {
                self.review_surface.selected_path
                    .as_deref()
                    .and_then(|path| self.active_diff_file_range_for_path(path))
            })
            .or_else(|| {
                session.first_file().map(|range| FileRowRange {
                    path: range.path.clone(),
                    status: range.status,
                    start_row: range.start_row,
                })
            })
    }

    pub(crate) fn current_review_path(&self) -> Option<String> {
        if let Some(session) = self.review_workspace_session.as_ref() {
            return preferred_review_workspace_path_for_session(
                self.current_review_editor_path().as_deref(),
                None,
                self.current_review_file_range().map(|range| range.path).as_deref(),
                self.review_surface.selected_path.as_deref(),
                session,
            );
        }

        self.review_surface.selected_path.clone()
    }

    pub(crate) fn should_reuse_loaded_review_compare(&self) -> bool {
        should_reuse_loaded_review_compare(LoadedReviewCompareReuseState {
            has_loaded_session: self.review_workspace_session.is_some(),
            review_compare_loading: self.review_compare_loading,
            review_compare_error: self.review_compare_error.as_deref(),
            current_left_source_id: self.review_left_source_id.as_deref(),
            current_right_source_id: self.review_right_source_id.as_deref(),
            loaded_left_source_id: self.review_loaded_left_source_id.as_deref(),
            loaded_right_source_id: self.review_loaded_right_source_id.as_deref(),
            current_collapsed_files: &self.collapsed_files,
            loaded_collapsed_files: &self.review_loaded_collapsed_files,
            current_snapshot_fingerprint: self.last_snapshot_fingerprint.as_ref(),
            loaded_snapshot_fingerprint: self.review_loaded_snapshot_fingerprint.as_ref(),
        })
    }

    fn subscribe_review_compare_picker_states(&self, cx: &mut Context<Self>) {
        let review_left_picker_state = self.review_left_picker_state.clone();
        cx.subscribe(
            &review_left_picker_state,
            |this, _, event: &HunkPickerEvent<ReviewComparePickerDelegate>, cx| {
                let HunkPickerEvent::Confirm(source_id) = event;
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
            |this, _, event: &HunkPickerEvent<ReviewComparePickerDelegate>, cx| {
                let HunkPickerEvent::Confirm(source_id) = event;
                let Some(source_id) = source_id.clone() else {
                    return;
                };
                this.update_review_compare_selection(None, Some(source_id), cx);
            },
        )
        .detach();
    }

    fn sync_review_compare_picker_state(
        picker_state: &Entity<HunkPickerState<ReviewComparePickerDelegate>>,
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

    fn selected_git_workspace_review_compare_selection(
        &self,
    ) -> Option<(Option<String>, Option<String>)> {
        let default_base_branch_name = self
            .project_path
            .as_deref()
            .and_then(|project_path| resolve_default_base_branch_name(project_path).ok().flatten());
        selected_git_workspace_review_compare_selection_ids(
            &self.review_compare_sources,
            &self.workspace_targets,
            self.selected_git_workspace_root().as_deref(),
            default_base_branch_name.as_deref(),
        )
    }

    pub(super) fn open_git_workspace_change_in_review(
        &mut self,
        path: String,
        cx: &mut Context<Self>,
    ) {
        if let Some((left_source_id, right_source_id)) =
            self.selected_git_workspace_review_compare_selection()
        {
            self.update_review_compare_selection_with_persistence(
                left_source_id,
                right_source_id,
                false,
                cx,
            );
        }
        self.set_review_selected_file(Some(path), None);
        self.set_workspace_view_mode(WorkspaceViewMode::Diff, cx);
    }

    fn active_diff_files(&self) -> &[ChangedFile] {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            &self.review_files
        } else {
            &self.files
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
        if self.workspace_view_mode == WorkspaceViewMode::Diff
            && let Some(session) = self.review_workspace_session.as_ref()
        {
            return session.contains_path(path);
        }

        self.active_diff_files().iter().any(|file| file.path == path)
    }

    fn active_diff_first_path(&self) -> Option<String> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff
            && let Some(session) = self.review_workspace_session.as_ref()
        {
            return session.first_path().map(ToString::to_string);
        }

        self.active_diff_files().first().map(|file| file.path.clone())
    }

    pub(crate) fn active_diff_file_range_for_path(&self, path: &str) -> Option<FileRowRange> {
        self.review_workspace_session
            .as_ref()
            .and_then(|session| session.file_range_for_path(path))
            .map(|range| FileRowRange {
                path: range.path.clone(),
                status: range.status,
                start_row: range.start_row,
            })
    }

    pub(crate) fn active_diff_file_range_at_or_after_row(
        &self,
        row_ix: usize,
    ) -> Option<FileRowRange> {
        self.review_workspace_session
            .as_ref()
            .and_then(|session| session.file_at_or_after_surface_row(row_ix))
            .map(|range| FileRowRange {
                path: range.path.clone(),
                status: range.status,
                start_row: range.start_row,
            })
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
        let repo_key = self.review_compare_repo_key();
        let changed = update_persisted_review_compare_selection(
            true,
            &mut self.state.review_compare_selection_by_repo,
            repo_key.as_deref(),
            self.review_left_source_id.clone(),
            self.review_right_source_id.clone(),
        );
        if changed {
            self.persist_state();
        }
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
        self.workspace_view_mode == WorkspaceViewMode::Diff
            && self.active_review_compare_is_default_pair()
    }

    fn build_review_workspace_editors(
        &self,
        session: &crate::app::review_workspace_session::ReviewWorkspaceSession,
        preferred_path: Option<&str>,
    ) -> anyhow::Result<(
        crate::app::native_files_editor::SharedFilesEditor,
        crate::app::native_files_editor::SharedFilesEditor,
    )> {
        let layout = session.layout().clone();
        let preferred_path = preferred_path.map(std::path::Path::new);
        let left_workspace_editor = std::rc::Rc::new(std::cell::RefCell::new(
            crate::app::native_files_editor::FilesEditor::new(),
        ));
        left_workspace_editor
            .borrow_mut()
            .open_workspace_layout_documents(
                layout.clone(),
                session.editor_documents(
                    crate::app::review_workspace_session::ReviewWorkspaceEditorSide::Left,
                ),
                preferred_path,
            )?;

        let right_workspace_editor = std::rc::Rc::new(std::cell::RefCell::new(
            crate::app::native_files_editor::FilesEditor::new(),
        ));
        right_workspace_editor
            .borrow_mut()
            .open_workspace_layout_documents(
                layout,
                session.editor_documents(
                    crate::app::review_workspace_session::ReviewWorkspaceEditorSide::Right,
                ),
                preferred_path,
            )?;

        Ok((left_workspace_editor, right_workspace_editor))
    }

    fn clear_review_compare_loaded_state(&mut self, empty_message: &str, cx: &mut Context<Self>) {
        self.cancel_patch_reload();
        self.review_compare_loading = false;
        self.review_compare_error = None;
        self.review_workspace_session = None;
        self.review_loaded_left_source_id = None;
        self.review_loaded_right_source_id = None;
        self.review_loaded_collapsed_files.clear();
        self.review_loaded_snapshot_fingerprint = None;
        self.review_surface.clear_workspace_editors();
        self.review_surface.clear_workspace_search_matches();
        self.review_surface.selected_path = None;
        self.review_surface.clear_row_selection();
        self.review_files.clear();
        self.review_file_status_by_path.clear();
        self.review_file_line_stats.clear();
        self.review_overall_line_stats = LineStats::default();
        self.comments_cache.clear();
        self.comment_miss_streaks.clear();
        self.reset_comment_row_match_cache();
        self.clear_comment_ui_state();
        self.reset_review_surface_runtime_state();
        self.review_surface.clear_workspace_surface_snapshot();
        self.review_surface.status_message = Some(empty_message.to_string());
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

        if self.should_reuse_loaded_review_compare() {
            self.review_compare_loading = false;
            self.review_compare_error = None;
            self.review_surface.status_message = None;
            self.review_surface.selected_path = self.current_review_path();
            if self.review_comments_enabled() {
                self.refresh_comments_cache_from_store();
            }
            if self.editor_search_visible {
                self.sync_editor_search_query(cx);
            }
            self.prime_diff_surface_visible_state(false, cx);
            cx.notify();
            return;
        }

        let previous_review_line_stats = self.review_file_line_stats.clone();
        let collapsed_files = self.collapsed_files.clone();
        let left_source_id = self.review_left_source_id.clone();
        let right_source_id = self.review_right_source_id.clone();
        let epoch = self.next_patch_epoch();

        self.review_compare_loading = true;
        self.review_compare_error = None;
        self.patch_loading = false;
        self.reset_review_surface_runtime_state();
        self.review_surface.clear_workspace_surface_snapshot();
        self.review_surface.status_message = Some("Loading comparison...".to_string());

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
        self.review_surface.status_message = None;
        self.review_workspace_session =
            match crate::app::review_workspace_session::ReviewWorkspaceSession::from_compare_snapshot(
                &snapshot,
                &self.collapsed_files,
            ) {
                Ok(session) => {
                    let session = session.with_render_stream(&stream);
                    debug!(
                        workspace_documents = session.layout().documents().len(),
                        workspace_excerpts = session.layout().excerpts().len(),
                        workspace_rows = session.layout().total_rows(),
                        render_rows = session.row_count(),
                        "review workspace session rebuilt"
                    );
                    Some(session)
                }
                Err(err) => {
                    error!("failed to build review workspace session: {err}");
                    self.clear_review_compare_loaded_state(
                        "Failed to build comparison surface.",
                        cx,
                    );
                    self.review_compare_error = Some(err.to_string());
                    return;
                }
            };
        let preferred_selected_path = self
            .current_review_editor_path()
            .or_else(|| self.review_surface.selected_path.clone());
        let Some((left_workspace_editor, right_workspace_editor)) = self
            .review_workspace_session
            .as_ref()
            .map(|session| {
                self.build_review_workspace_editors(session, preferred_selected_path.as_deref())
            })
            .transpose()
            .unwrap_or_else(|err| {
                error!("failed to build review workspace editors: {err:#}");
                self.clear_review_compare_loaded_state(
                    "Failed to build comparison surface.",
                    cx,
                );
                self.review_compare_error = Some(Self::format_error_chain(&err));
                None
            })
        else {
            return;
        };
        self.review_surface
            .set_workspace_owner(left_workspace_editor, right_workspace_editor);
        let seeded_display_rows = self.seed_review_surface_display_rows();
        self.review_files = snapshot.files;
        self.review_file_status_by_path = self
            .review_files
            .iter()
            .map(|file| (file.path.clone(), file.status))
            .collect();
        self.review_loaded_left_source_id = self.review_left_source_id.clone();
        self.review_loaded_right_source_id = self.review_right_source_id.clone();
        self.review_loaded_collapsed_files = self.collapsed_files.clone();
        self.review_loaded_snapshot_fingerprint = self.last_snapshot_fingerprint.clone();
        self.review_file_line_stats = snapshot.file_line_stats;
        self.review_overall_line_stats = snapshot.overall_line_stats;
        self.collapsed_files
            .retain(|path| self.review_files.iter().any(|file| file.path == *path));

        self.apply_loaded_review_workspace_surface();
        debug!(
            seeded_display_rows,
            "review workspace surface projection initialized"
        );

        if let Some(session) = self.review_workspace_session.as_ref() {
            let workspace_row_count = session.file_ranges().last().map(|range| range.end_row).unwrap_or(0);
            let render_row_count = session.row_count();
            if workspace_row_count != render_row_count {
                error!(
                    workspace_rows = workspace_row_count,
                    render_rows = render_row_count,
                    "review workspace session surface rows diverged from render rows"
                );
            }
        }

        let has_selection = preferred_selected_path
            .as_ref()
            .is_some_and(|path| self.active_diff_contains_path(path.as_str()));
        let next_selected_path = if has_selection {
            preferred_selected_path
        } else {
            self.review_workspace_session
                .as_ref()
                .and_then(|session| session.first_path().map(ToString::to_string))
                .or_else(|| self.active_diff_first_path())
        };
        let next_selected_status = next_selected_path
            .as_deref()
            .and_then(|selected| self.status_for_path(selected));
        self.set_review_selected_file(next_selected_path, next_selected_status);
        if self.editor_search_visible {
            self.sync_editor_search_query(cx);
        } else {
            self.review_surface.clear_workspace_search_matches();
        }
        self.refresh_comments_cache_from_store();
        self.rebuild_comment_row_match_cache();
        if self.review_comments_enabled() {
            self.reconcile_comments_with_loaded_diff();
        }

        if self.scroll_selected_after_reload {
            self.scroll_selected_file_to_top();
            self.scroll_selected_after_reload = false;
        }
        self.prime_diff_surface_visible_state(true, cx);

        self.request_repo_tree_reload(cx);
        cx.notify();
    }

    fn update_review_compare_selection(
        &mut self,
        next_left_source_id: Option<String>,
        next_right_source_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.update_review_compare_selection_with_persistence(
            next_left_source_id,
            next_right_source_id,
            true,
            cx,
        );
    }

    fn update_review_compare_selection_with_persistence(
        &mut self,
        next_left_source_id: Option<String>,
        next_right_source_id: Option<String>,
        persist_selection: bool,
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
        if persist_selection {
            self.persist_review_compare_selection();
        }
        self.sync_review_compare_picker_states(cx);
        self.comments_cache.clear();
        self.comment_miss_streaks.clear();
        self.reset_comment_row_match_cache();
        self.clear_comment_ui_state();
        self.review_loaded_left_source_id = None;
        self.review_loaded_right_source_id = None;
        self.review_loaded_collapsed_files.clear();
        self.review_loaded_snapshot_fingerprint = None;
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.scroll_selected_after_reload = true;
            self.request_review_compare_refresh(cx);
        } else {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod review_compare_tests {
    use super::{
        LoadedReviewCompareReuseState, preferred_review_workspace_path_for_session,
        should_reuse_loaded_review_compare,
    };
    use hunk_git::compare::CompareSnapshot;
    use hunk_git::git::{ChangedFile, FileStatus, LineStats};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn loaded_review_compare_reuse_requires_matching_identity() {
        let current_collapsed_files = BTreeSet::new();
        let loaded_collapsed_files = BTreeSet::new();
        let matching_state = LoadedReviewCompareReuseState {
            has_loaded_session: true,
            review_compare_loading: false,
            review_compare_error: None,
            current_left_source_id: Some("left"),
            current_right_source_id: Some("right"),
            loaded_left_source_id: Some("left"),
            loaded_right_source_id: Some("right"),
            current_collapsed_files: &current_collapsed_files,
            loaded_collapsed_files: &loaded_collapsed_files,
            current_snapshot_fingerprint: Some(&1_u8),
            loaded_snapshot_fingerprint: Some(&1_u8),
        };

        assert!(should_reuse_loaded_review_compare(matching_state));
        assert!(!should_reuse_loaded_review_compare(LoadedReviewCompareReuseState {
            loaded_right_source_id: Some("other"),
            ..matching_state
        }));
        assert!(!should_reuse_loaded_review_compare(LoadedReviewCompareReuseState {
            loaded_snapshot_fingerprint: Some(&2_u8),
            ..matching_state
        }));
        let loaded_with_collapse = BTreeSet::from([String::from("src/main.rs")]);
        assert!(!should_reuse_loaded_review_compare(LoadedReviewCompareReuseState {
            loaded_collapsed_files: &loaded_with_collapse,
            ..matching_state
        }));
        assert!(!should_reuse_loaded_review_compare(LoadedReviewCompareReuseState {
            review_compare_loading: true,
            ..matching_state
        }));
        assert!(!should_reuse_loaded_review_compare(LoadedReviewCompareReuseState {
            has_loaded_session: false,
            ..matching_state
        }));
    }

    fn changed_file(path: &str, status: FileStatus) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            status,
            staged: false,
            unstaged: false,
            untracked: false,
        }
    }

    fn review_session(paths: &[&str]) -> crate::app::review_workspace_session::ReviewWorkspaceSession {
        let snapshot = CompareSnapshot {
            files: paths
                .iter()
                .map(|path| changed_file(path, FileStatus::Modified))
                .collect(),
            file_line_stats: BTreeMap::new(),
            overall_line_stats: LineStats::default(),
            patches_by_path: paths
                .iter()
                .map(|path| ((*path).to_string(), String::new()))
                .collect(),
        };
        crate::app::review_workspace_session::ReviewWorkspaceSession::from_compare_snapshot(
            &snapshot,
            &BTreeSet::new(),
        )
        .expect("review workspace session should build")
    }

    #[test]
    fn preferred_review_workspace_path_prefers_current_selection_before_stale_selection() {
        let session = review_session(&["src/main.rs", "src/lib.rs"]);

        assert_eq!(
            preferred_review_workspace_path_for_session(
                None,
                None,
                None,
                Some("src/lib.rs"),
                &session,
            ),
            Some("src/lib.rs".to_string())
        );
    }

    #[test]
    fn preferred_review_workspace_path_skips_missing_entries_and_falls_back_to_first_file() {
        let session = review_session(&["src/main.rs", "src/lib.rs"]);

        assert_eq!(
            preferred_review_workspace_path_for_session(
                None,
                None,
                None,
                Some("also-missing.rs"),
                &session,
            ),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn preferred_review_workspace_path_prefers_editor_session_path_when_available() {
        let session = review_session(&["src/main.rs", "src/lib.rs"]);

        assert_eq!(
            preferred_review_workspace_path_for_session(
                Some("src/lib.rs"),
                Some("src/main.rs"),
                None,
                Some("src/main.rs"),
                &session,
            ),
            Some("src/lib.rs".to_string())
        );
    }
}
