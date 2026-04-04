impl DiffViewer {
    pub(super) fn review_surface_snapshot_options(
        &self,
    ) -> review_workspace_session::ReviewWorkspaceSurfaceOptions {
        let comment_open_counts_by_row = self
            .comment_open_row_counts
            .iter()
            .enumerate()
            .filter_map(|(row_ix, count)| (*count > 0).then_some((row_ix, *count)))
            .collect::<BTreeMap<_, _>>();
        let mut comment_affordance_rows = self
            .comment_open_row_counts
            .iter()
            .enumerate()
            .filter_map(|(row_ix, count)| {
                (*count > 0 && self.row_supports_comments(row_ix)).then_some(row_ix)
            })
            .collect::<BTreeSet<_>>();

        if let Some(row_ix) = self.hovered_comment_row.filter(|row_ix| self.row_supports_comments(*row_ix))
        {
            comment_affordance_rows.insert(row_ix);
        }

        let active_comment_editor_row = self
            .active_comment_editor_row
            .filter(|row_ix| self.row_supports_comments(*row_ix));
        if let Some(row_ix) = active_comment_editor_row {
            comment_affordance_rows.insert(row_ix);
        }
        let view_file_enabled_paths = self
            .review_workspace_session
            .as_ref()
            .map(|session| {
                session
                    .file_ranges()
                    .iter()
                    .filter(|range| {
                        self.can_open_file_in_files_workspace(range.path.as_str(), range.status)
                    })
                    .map(|range| range.path.clone())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();

        let search_highlight_columns_by_row = self
            .review_workspace_session
            .as_ref()
            .map(|session| {
                session.build_search_highlight_columns_by_row(
                    &self.review_surface.workspace_search_matches,
                )
            })
            .unwrap_or_default();

        review_workspace_session::ReviewWorkspaceSurfaceOptions {
            comment_affordance_rows,
            comment_open_counts_by_row,
            active_comment_editor_row,
            collapsed_paths: self.collapsed_files.clone(),
            view_file_enabled_paths,
            search_highlight_columns_by_row,
        }
    }

    pub(super) fn refresh_review_surface_snapshot(
        &mut self,
    ) -> Option<review_workspace_session::ReviewWorkspaceVisibleState> {
        if !self.uses_review_workspace_sections_surface() {
            self.review_surface.clear_workspace_surface_snapshot();
            return None;
        }

        let scroll_top_px = self.current_review_surface_scroll_top_px();
        let viewport_height_px = self
            .review_surface
            .diff_scroll_handle
            .bounds()
            .size
            .height
            .max(Pixels::ZERO)
            .as_f32()
            .round() as usize;

        let needs_refresh = self
            .review_surface
            .last_surface_snapshot
            .as_ref()
            .is_none_or(|snapshot| {
                snapshot.scroll_top_px != scroll_top_px
                    || snapshot.viewport_height_px != viewport_height_px
        });
        if needs_refresh {
            let surface_options = self.review_surface_snapshot_options();
            let left_workspace_editor = self.review_surface.left_workspace_editor.clone()?;
            let right_workspace_editor = self.review_surface.right_workspace_editor.clone()?;
            let session = self.review_workspace_session.as_mut()?;
            let Some(display_rows) = Self::review_surface_display_rows(
                session,
                &left_workspace_editor,
                &right_workspace_editor,
                scroll_top_px,
                viewport_height_px,
                8,
            ) else {
                self.review_surface.last_surface_snapshot = None;
                return None;
            };
            let snapshot = session.build_surface_snapshot_with_display_rows(
                scroll_top_px,
                viewport_height_px,
                1,
                8,
                &surface_options,
                &display_rows,
            );
            self.review_surface.last_surface_snapshot = Some(snapshot);
        }

        self.review_surface
            .last_surface_snapshot
            .as_ref()
            .map(|snapshot| snapshot.visible_state.clone())
    }

    fn review_surface_display_rows(
        session: &mut review_workspace_session::ReviewWorkspaceSession,
        left_workspace_editor: &crate::app::native_files_editor::SharedFilesEditor,
        right_workspace_editor: &crate::app::native_files_editor::SharedFilesEditor,
        scroll_top_px: usize,
        viewport_height_px: usize,
        overscan_rows: usize,
    ) -> Option<review_workspace_session::ReviewWorkspaceDisplayRows> {
        let visible_row_range = session.visible_row_range_for_viewport(
            scroll_top_px,
            viewport_height_px,
        )?;
        let first_visible_row = visible_row_range.start.saturating_sub(overscan_rows);
        let last_visible_row = visible_row_range
            .end
            .saturating_add(overscan_rows)
            .min(session.row_count());
        let viewport = session.display_viewport_for_surface_viewport(
            scroll_top_px,
            viewport_height_px,
            overscan_rows,
        )?;
        let mut left_editor = left_workspace_editor.borrow_mut();
        let left_projected =
            left_editor.build_workspace_projected_snapshot(viewport, 4).and_then(projected_review_workspace_side_rows);
        let left_rows = if left_projected.is_some() {
            left_projected
                .as_ref()
                .map(|rows| rows.rows_by_display_row.clone())
                .unwrap_or_default()
        } else {
            let left_snapshot = left_editor.build_workspace_display_snapshot(viewport, 4, false)?;
            left_snapshot
                .visible_rows
                .into_iter()
                .map(|row| (row.row_index, row))
                .collect::<BTreeMap<_, _>>()
        };
        let left_syntax_rows = left_rows.values().cloned().collect::<Vec<_>>();
        let left_syntax_by_display_row =
            left_editor.workspace_display_segments_by_row(&left_syntax_rows)?;
        drop(left_editor);

        let mut right_editor = right_workspace_editor.borrow_mut();
        let right_projected =
            right_editor.build_workspace_projected_snapshot(viewport, 4).and_then(projected_review_workspace_side_rows);
        let right_rows = if right_projected.is_some() {
            right_projected
                .as_ref()
                .map(|rows| rows.rows_by_display_row.clone())
                .unwrap_or_default()
        } else {
            let right_snapshot = right_editor.build_workspace_display_snapshot(viewport, 4, false)?;
            right_snapshot
                .visible_rows
                .into_iter()
                .map(|row| (row.row_index, row))
                .collect::<BTreeMap<_, _>>()
        };
        let right_syntax_rows = right_rows.values().cloned().collect::<Vec<_>>();
        let right_syntax_by_display_row =
            right_editor.workspace_display_segments_by_row(&right_syntax_rows)?;

        let rows = if let (Some(left_projected), Some(right_projected)) =
            (left_projected, right_projected)
        {
            review_workspace_display_row_entries_from_projected_sides(
                &left_projected,
                &right_projected,
            )?
        } else {
            review_workspace_display_row_entries(&left_rows, &right_rows)
        };

        let display_rows = review_workspace_session::ReviewWorkspaceDisplayRows {
            rows,
            left_by_display_row: left_rows,
            right_by_display_row: right_rows,
            left_syntax_by_display_row,
            right_syntax_by_display_row,
        };
        if display_rows.covers_row_range(first_visible_row..last_visible_row) {
            session.refresh_display_geometry_from_display_rows(&display_rows);
            Some(display_rows)
        } else {
            None
        }
    }

    pub(super) fn current_review_surface_snapshot(
        &self,
    ) -> Option<&review_workspace_session::ReviewWorkspaceSurfaceSnapshot> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self.review_surface.last_surface_snapshot.as_ref();
        }

        None
    }

    pub(super) fn current_review_visible_state(
        &self,
    ) -> Option<review_workspace_session::ReviewWorkspaceVisibleState> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self
                .current_review_surface_snapshot()
                .map(|snapshot| snapshot.visible_state.clone());
        }

        None
    }

    pub(super) fn current_review_surface_scroll_top_px(&self) -> usize {
        self.review_surface
            .diff_scroll_handle
            .offset()
            .y
            .min(Pixels::ZERO)
            .abs()
            .as_f32()
            .round() as usize
    }

    pub(super) fn uses_review_workspace_sections_surface(&self) -> bool {
        self.workspace_view_mode == WorkspaceViewMode::Diff && self.review_workspace_session.is_some()
    }

    pub(super) fn current_review_surface_top_row(&self) -> Option<usize> {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return None;
        }

        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return None;
        }

        self.current_review_visible_state().and_then(|state| state.top_row)
    }

    pub(super) fn current_review_visible_row_range(&self) -> Option<std::ops::Range<usize>> {
        if self.workspace_view_mode != WorkspaceViewMode::Diff {
            return None;
        }

        let row_count = self.active_diff_row_count();
        if row_count == 0 {
            return None;
        }

        self.current_review_visible_state()
            .and_then(|state| state.visible_row_range)
    }

    pub(super) fn current_review_surface_scroll_offset(&self) -> Point<Pixels> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self.review_surface.diff_scroll_handle.offset();
        }

        point(px(0.), px(0.))
    }

    pub(super) fn active_diff_row_count(&self) -> usize {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self
                .review_workspace_session
                .as_ref()
                .map(|session| session.row_count())
                .unwrap_or(0);
        }

        self.diff_rows.len()
    }

    pub(super) fn active_diff_row(&self, row_ix: usize) -> Option<&SideBySideRow> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self
                .review_workspace_session
                .as_ref()
                .and_then(|session| session.row(row_ix));
        }

        self.diff_rows.get(row_ix)
    }

    pub(super) fn active_diff_row_metadata(&self, row_ix: usize) -> Option<&DiffStreamRowMeta> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self
                .review_workspace_session
                .as_ref()
                .and_then(|session| session.row_metadata(row_ix));
        }

        self.diff_row_metadata.get(row_ix)
    }

    pub(super) fn active_diff_row_segment_cache(
        &self,
        row_ix: usize,
    ) -> Option<&DiffRowSegmentCache> {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            return self
                .review_workspace_session
                .as_ref()
                .and_then(|session| session.row_segment_cache(row_ix));
        }

        self.diff_row_segment_cache
            .get(row_ix)
            .and_then(Option::as_ref)
    }

    fn recompute_diff_visible_header_lookup(&mut self) {
        let row_count = self.active_diff_row_count();
        self.review_surface.clear_legacy_diff_row_lookups();
        if row_count == 0 {
            return;
        }

        if self.workspace_view_mode == WorkspaceViewMode::Diff
            && let Some(session) = self.review_workspace_session.as_ref()
        {
            debug_assert_eq!(session.row_count(), row_count);
            return;
        }

        self.review_surface.diff_visible_file_header_lookup = vec![None; row_count];
        self.review_surface.diff_visible_hunk_header_lookup = vec![None; row_count];

        let mut current_file_header = None::<usize>;
        let mut current_hunk_header = None::<usize>;
        for row_ix in 0..row_count {
            let containing_file_header = self.file_row_ranges.iter().find_map(|range| {
                if row_ix >= range.start_row && row_ix < range.end_row {
                    Some(range.start_row)
                } else {
                    None
                }
            });

            if let Some(meta) = self.active_diff_row_metadata(row_ix) {
                match meta.kind {
                    DiffStreamRowKind::EmptyState => {
                        current_file_header = None;
                        current_hunk_header = None;
                    }
                    DiffStreamRowKind::FileHeader => {
                        current_file_header = Some(row_ix);
                        current_hunk_header = None;
                    }
                    DiffStreamRowKind::CoreHunkHeader => {
                        current_file_header = current_file_header.or(containing_file_header);
                        current_hunk_header = Some(row_ix);
                    }
                    _ => {
                        if containing_file_header.is_some() {
                            current_file_header = containing_file_header;
                        }
                    }
                }
            } else if self
                .active_diff_row(row_ix)
                .is_some_and(|row| row.kind == DiffRowKind::HunkHeader)
            {
                current_file_header = current_file_header.or(containing_file_header);
                current_hunk_header = Some(row_ix);
            } else if containing_file_header.is_some() {
                current_file_header = containing_file_header;
            }

            self.review_surface.diff_visible_file_header_lookup[row_ix] = current_file_header;
            self.review_surface.diff_visible_hunk_header_lookup[row_ix] = current_hunk_header;
        }
    }

    fn next_snapshot_epoch(&mut self) -> usize {
        self.snapshot_epoch = self.snapshot_epoch.saturating_add(1);
        self.snapshot_epoch
    }

    fn next_line_stats_epoch(&mut self) -> usize {
        self.line_stats_epoch = self.line_stats_epoch.saturating_add(1);
        self.line_stats_epoch
    }

    fn cancel_line_stats_refresh(&mut self) {
        self.next_line_stats_epoch();
        self.line_stats_task = Task::ready(());
        self.line_stats_loading = false;
        self.pending_line_stats_refresh = None;
    }

    fn auto_refresh_interval(&self) -> Duration {
        if self.config.auto_refresh_interval_ms == 0 {
            return Duration::ZERO;
        }

        let configured_ms = self
            .config
            .auto_refresh_interval_ms
            .clamp(250, Self::AUTO_REFRESH_MAX_INTERVAL_MS);
        let base_ms = configured_ms.min(Self::AUTO_REFRESH_QUICK_PROBE_MS);
        let backoff_factor =
            1_u64 << self.auto_refresh_unmodified_streak.min(Self::AUTO_REFRESH_BACKOFF_STEPS);
        let interval_ms = base_ms
            .saturating_mul(backoff_factor)
            .min(configured_ms)
            .min(Self::AUTO_REFRESH_MAX_INTERVAL_MS);
        Duration::from_millis(interval_ms)
    }

    fn repo_watch_roots(
        primary_root: Option<&std::path::Path>,
        git_workspace_root: Option<&std::path::Path>,
    ) -> Vec<std::path::PathBuf> {
        let mut roots = Vec::new();

        if let Some(primary_root) = primary_root {
            roots.push(primary_root.to_path_buf());
        }

        if let Some(git_workspace_root) = git_workspace_root
            && roots.iter().all(|root| {
                root.as_path() != git_workspace_root
                    && !git_workspace_root.starts_with(root.as_path())
            })
        {
            roots.push(git_workspace_root.to_path_buf());
        }

        roots
    }

    fn should_ignore_repo_watch_path(path: &std::path::Path, repo_root: &std::path::Path) -> bool {
        let Ok(relative_path) = path.strip_prefix(repo_root) else {
            return false;
        };

        if hunk_git::worktree::repo_relative_path_is_within_managed_worktrees(
            relative_path.to_string_lossy().as_ref(),
        ) {
            return true;
        }

        relative_path.components().any(|component| {
            let component = component.as_os_str();
            component
                .to_str()
                .is_some_and(Self::is_hunk_temp_save_component)
        })
    }

    fn is_repo_watch_metadata_path(path: &std::path::Path, repo_root: &std::path::Path) -> bool {
        let Ok(relative_path) = path.strip_prefix(repo_root) else {
            return false;
        };

        relative_path
            .components()
            .any(|component| component.as_os_str() == ".git")
    }

    fn repo_watch_metadata_changed(
        event_paths: &[std::path::PathBuf],
        repo_root: Option<&std::path::Path>,
    ) -> bool {
        repo_root.is_some_and(|root| {
            event_paths
                .iter()
                .any(|path| Self::is_repo_watch_metadata_path(path, root))
        })
    }

    fn repo_watch_recent_commits_changed(
        event_paths: &[std::path::PathBuf],
        repo_root: Option<&std::path::Path>,
    ) -> bool {
        repo_root.is_some_and(|root| {
            event_paths
                .iter()
                .any(|path| Self::is_repo_watch_recent_commits_path(path, root))
        })
    }

    fn is_repo_watch_recent_commits_path(
        path: &std::path::Path,
        repo_root: &std::path::Path,
    ) -> bool {
        let Ok(relative_path) = path.strip_prefix(repo_root) else {
            return false;
        };
        let relative_path = relative_path.to_string_lossy().replace('\\', "/");

        matches!(
            relative_path.as_str(),
            ".git/HEAD" | ".git/packed-refs" | ".git/reftable"
        ) || relative_path.starts_with(".git/refs/")
            || relative_path.starts_with(".git/logs/")
            || relative_path.ends_with("/HEAD")
                && relative_path.starts_with(".git/worktrees/")
            || relative_path.contains("/refs/")
                && relative_path.starts_with(".git/worktrees/")
            || relative_path.contains("/logs/")
                && relative_path.starts_with(".git/worktrees/")
    }

    fn repo_watch_dirty_path(
        path: &std::path::Path,
        repo_root: &std::path::Path,
    ) -> Option<String> {
        let Ok(relative_path) = path.strip_prefix(repo_root) else {
            return None;
        };
        if relative_path.as_os_str().is_empty() {
            return None;
        }
        if Self::should_ignore_repo_watch_path(path, repo_root)
            || Self::is_repo_watch_metadata_path(path, repo_root)
        {
            return None;
        }

        Some(relative_path.to_string_lossy().replace('\\', "/"))
    }

    fn repo_watch_dirty_path_candidate(
        path: &std::path::Path,
        repo_root: &std::path::Path,
    ) -> Option<(String, bool)> {
        let relative_path = Self::repo_watch_dirty_path(path, repo_root)?;
        let is_dir = std::fs::symlink_metadata(path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        Some((relative_path, is_dir))
    }

    fn repo_watch_non_ignored_dirty_paths(
        event_paths: &[std::path::PathBuf],
        repo_root: Option<&std::path::Path>,
        ignore_matcher: Option<&hunk_git::git::RepoIgnoreMatcher>,
    ) -> BTreeSet<String> {
        let Some(repo_root) = repo_root else {
            return BTreeSet::new();
        };

        let candidates = event_paths
            .iter()
            .filter_map(|path| Self::repo_watch_dirty_path_candidate(path, repo_root))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return BTreeSet::new();
        }

        match ignore_matcher {
            Some(ignore_matcher) => match ignore_matcher.filter_non_ignored_paths(candidates.as_slice()) {
                Ok(paths) => paths,
                Err(err) => {
                    warn!(
                        "failed to filter ignored repo watch paths for {}: {err:#}",
                        repo_root.display()
                    );
                    candidates.into_iter().map(|(path, _)| path).collect()
                }
            },
            None => candidates.into_iter().map(|(path, _)| path).collect(),
        }
    }

    fn queue_dirty_paths<I>(&mut self, paths: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.pending_dirty_paths.extend(paths);
    }

    fn should_refresh_git_workspace_from_repo_watch(
        event_paths: &[std::path::PathBuf],
        primary_root: &std::path::Path,
        git_workspace_root: &std::path::Path,
        ignore_matcher: Option<&hunk_git::git::RepoIgnoreMatcher>,
    ) -> bool {
        if primary_root == git_workspace_root {
            return false;
        }

        event_paths.iter().any(|path| {
            Self::is_repo_watch_metadata_path(path, primary_root)
                || Self::is_repo_watch_metadata_path(path, git_workspace_root)
        })
            || !Self::repo_watch_non_ignored_dirty_paths(
                event_paths,
                Some(git_workspace_root),
                ignore_matcher,
            )
                .is_empty()
    }

    fn should_process_repo_watch_event(event: &notify::Event) -> bool {
        // Linux inotify reports non-mutating access/open events for watched paths. Treating those
        // as dirty-file changes creates a self-sustaining refresh loop while Hunk scans the repo.
        !matches!(event.kind, notify::EventKind::Access(_))
    }

    fn is_hunk_temp_save_component(name: &str) -> bool {
        let Some((_, suffix)) = name.rsplit_once(".hunk-tmp.") else {
            return false;
        };
        let mut parts = suffix.split('.');
        let Some(pid) = parts.next() else {
            return false;
        };
        let Some(nonce) = parts.next() else {
            return false;
        };
        if parts.next().is_some() {
            return false;
        }

        !pid.is_empty()
            && !nonce.is_empty()
            && pid.bytes().all(|byte| byte.is_ascii_digit())
            && nonce.bytes().all(|byte| byte.is_ascii_digit())
    }

    fn start_repo_watch(&mut self, cx: &mut Context<Self>) {
        self.repo_watch_task = Task::ready(());
        self.repo_watch_refresh_task = Task::ready(());
        self.repo_watch_refresh_epoch = 0;
        self.repo_watch_pending_refresh = None;
        self.repo_watch_pending_git_workspace_refresh = false;
        self.repo_watch_pending_recent_commits_refresh = false;

        let primary_root = self.repo_root.clone().or_else(|| self.project_path.clone());
        let git_workspace_root = self.selected_git_workspace_root();
        let watch_roots =
            Self::repo_watch_roots(primary_root.as_deref(), git_workspace_root.as_deref());
        if watch_roots.is_empty() {
            return;
        }

        let (event_tx, mut event_rx) = mpsc::unbounded::<notify::Result<notify::Event>>();
        let watch_roots_for_cb = watch_roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let watcher = notify::recommended_watcher(move |result| {
            event_tx.unbounded_send(result).ok();
        });

        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(err) => {
                error!("failed to start file watch for {}: {err}", watch_roots_for_cb);
                return;
            }
        };

        for watch_root in &watch_roots {
            if let Err(err) = watcher.watch(watch_root, notify::RecursiveMode::Recursive) {
                error!("failed to watch repository at {}: {err}", watch_root.display());
                return;
            }
        }

        let primary_ignore_matcher = primary_root.as_ref().and_then(|root| {
            hunk_git::git::RepoIgnoreMatcher::open(root.as_path())
                .map_err(|err| {
                    warn!(
                        "failed to initialize repo watch ignore matcher for {}: {err:#}",
                        root.display()
                    );
                })
                .ok()
        });
        let git_workspace_ignore_matcher = git_workspace_root
            .as_ref()
            .filter(|git_workspace_root| primary_root.as_deref() != Some(git_workspace_root.as_path()))
            .and_then(|root| {
                hunk_git::git::RepoIgnoreMatcher::open(root.as_path())
                    .map_err(|err| {
                        warn!(
                            "failed to initialize repo watch ignore matcher for {}: {err:#}",
                            root.display()
                        );
                    })
                    .ok()
            });

        self.repo_watch_task = cx.spawn(async move |this, cx| {
            while let Some(event) = event_rx.next().await {
                let Ok(event) = event else {
                    continue;
                };

                if event.paths.is_empty() || !Self::should_process_repo_watch_event(&event) {
                    continue;
                }

                let metadata_changed =
                    Self::repo_watch_metadata_changed(event.paths.as_slice(), primary_root.as_deref());
                let recent_commits_changed = Self::repo_watch_recent_commits_changed(
                    event.paths.as_slice(),
                    primary_root.as_deref(),
                );
                let git_workspace_metadata_changed = git_workspace_root
                    .as_deref()
                    .filter(|git_workspace_root| primary_root.as_deref() != Some(*git_workspace_root))
                    .is_some_and(|git_workspace_root| {
                        Self::repo_watch_metadata_changed(
                            event.paths.as_slice(),
                            Some(git_workspace_root),
                        )
                    });
                let git_workspace_recent_commits_changed = git_workspace_root
                    .as_deref()
                    .filter(|git_workspace_root| primary_root.as_deref() != Some(*git_workspace_root))
                    .is_some_and(|git_workspace_root| {
                        Self::repo_watch_recent_commits_changed(
                            event.paths.as_slice(),
                            Some(git_workspace_root),
                        )
                    });
                let dirty_paths = Self::repo_watch_non_ignored_dirty_paths(
                    event.paths.as_slice(),
                    primary_root.as_deref(),
                    primary_ignore_matcher.as_ref(),
                );
                let request = repo_watch_refresh_request(metadata_changed, !dirty_paths.is_empty());
                let refresh_git_workspace = primary_root
                    .as_ref()
                    .zip(git_workspace_root.as_ref())
                    .is_some_and(|(primary_root, git_workspace_root)| {
                        Self::should_refresh_git_workspace_from_repo_watch(
                            event.paths.as_slice(),
                            primary_root,
                            git_workspace_root,
                            git_workspace_ignore_matcher.as_ref(),
                        )
                    });
                if request.is_none() && !refresh_git_workspace {
                    continue;
                }

                if let Some(this) = this.upgrade() {
                    let primary_root = primary_root.clone();
                    let git_workspace_root = git_workspace_root.clone();
                    this.update(cx, move |this, cx| {
                        if metadata_changed
                            && let Some(primary_root) = primary_root.as_ref()
                        {
                            invalidate_repo_metadata_caches(primary_root.as_path());
                        }
                        if git_workspace_metadata_changed
                            && let Some(git_workspace_root) = git_workspace_root.as_ref()
                        {
                            invalidate_repo_metadata_caches(git_workspace_root.as_path());
                        }
                        if recent_commits_changed || git_workspace_recent_commits_changed {
                            this.repo_watch_pending_recent_commits_refresh = true;
                        }
                        if !dirty_paths.is_empty() {
                            this.queue_dirty_paths(dirty_paths);
                        }
                        this.schedule_repo_watch_refresh(request, refresh_git_workspace, cx);
                    });
                }
            }
            drop(watcher);
        });
    }

    fn next_repo_watch_refresh_epoch(&mut self) -> usize {
        self.repo_watch_refresh_epoch = self.repo_watch_refresh_epoch.saturating_add(1);
        self.repo_watch_refresh_epoch
    }

    fn schedule_repo_watch_refresh(
        &mut self,
        request: Option<SnapshotRefreshRequest>,
        refresh_git_workspace: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(request) = request {
            self.repo_watch_pending_refresh = Some(
                self.repo_watch_pending_refresh
                    .map_or(request, |pending| pending.merge(request)),
            );
        }
        self.repo_watch_pending_git_workspace_refresh |= refresh_git_workspace;
        let epoch = self.next_repo_watch_refresh_epoch();
        self.repo_watch_refresh_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Self::REPO_WATCH_DEBOUNCE)
                .await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.repo_watch_refresh_epoch {
                        return;
                    }
                    let request = this.repo_watch_pending_refresh.take();
                    let refresh_git_workspace =
                        std::mem::take(&mut this.repo_watch_pending_git_workspace_refresh);
                    let refresh_recent_commits =
                        std::mem::take(&mut this.repo_watch_pending_recent_commits_refresh);
                    if let Some(request) = request {
                        this.request_snapshot_refresh_internal(request, cx);
                    }
                    if refresh_git_workspace {
                        this.request_git_workspace_refresh(false, cx);
                    }
                    if refresh_recent_commits {
                        this.request_recent_commits_refresh(false, cx);
                    }
                });
            }
        });
    }

    fn next_patch_epoch(&mut self) -> usize {
        self.patch_epoch = self.patch_epoch.saturating_add(1);
        self.patch_epoch
    }

    fn cancel_patch_reload(&mut self) {
        self.next_patch_epoch();
        self.patch_task = Task::ready(());
        self.patch_loading = false;
    }

    fn next_segment_prefetch_epoch(&mut self) -> usize {
        self.segment_prefetch_epoch = self.segment_prefetch_epoch.saturating_add(1);
        self.segment_prefetch_epoch
    }

    fn invalidate_segment_prefetch(&mut self) {
        self.next_segment_prefetch_epoch();
        self.segment_prefetch_task = Task::ready(());
        self.segment_prefetch_anchor_row = None;
        self.review_surface.last_prefetched_visible_row_range = None;
    }

    fn start_auto_refresh(&mut self, cx: &mut Context<Self>) {
        let epoch = self.next_refresh_epoch();
        if self.config.auto_refresh_interval_ms == 0 {
            return;
        }

        let interval = self.auto_refresh_interval();
        self.schedule_auto_refresh(epoch, interval, cx);
    }

    pub(super) fn restart_auto_refresh(&mut self, cx: &mut Context<Self>) {
        self.auto_refresh_task = Task::ready(());
        self.auto_refresh_unmodified_streak = 0;
        if self.config.auto_refresh_interval_ms == 0 {
            return;
        }

        let epoch = self.next_refresh_epoch();
        let interval = self.auto_refresh_interval();
        self.schedule_auto_refresh(epoch, interval, cx);
    }

    fn next_refresh_epoch(&mut self) -> usize {
        self.refresh_epoch = self.refresh_epoch.saturating_add(1);
        self.refresh_epoch
    }

    fn schedule_auto_refresh(&mut self, epoch: usize, delay: Duration, cx: &mut Context<Self>) {
        if epoch != self.refresh_epoch {
            return;
        }
        if delay == Duration::ZERO || self.config.auto_refresh_interval_ms == 0 {
            return;
        }

        self.auto_refresh_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.config.auto_refresh_interval_ms == 0 {
                        return;
                    }

                    if this.recently_scrolling() {
                        let next_epoch = this.next_refresh_epoch();
                        let next_delay = this.auto_refresh_interval();
                        this.schedule_auto_refresh(next_epoch, next_delay, cx);
                        return;
                    }

                    if this.project_path.is_some() {
                        this.request_snapshot_refresh_workflow_only(false, cx);
                        this.request_recent_commits_refresh(false, cx);
                    }

                    let next_delay = this.auto_refresh_interval();
                    let next_epoch = this.next_refresh_epoch();
                    this.schedule_auto_refresh(next_epoch, next_delay, cx);
                });
            }
        });
    }

    fn recently_scrolling(&self) -> bool {
        self.last_scroll_activity_at.elapsed() < AUTO_REFRESH_SCROLL_DEBOUNCE
    }
}

#[derive(Debug, Clone)]
struct ReviewWorkspaceProjectedSideRow {
    display_row_index: usize,
    row_index: usize,
    raw_row_range: std::ops::Range<usize>,
    row: hunk_editor::WorkspaceDisplayRow,
}

#[derive(Debug, Clone)]
struct ReviewWorkspaceProjectedSideRows {
    rows: Vec<ReviewWorkspaceProjectedSideRow>,
    rows_by_display_row: BTreeMap<usize, hunk_editor::WorkspaceDisplayRow>,
}

fn projected_review_workspace_side_rows(
    snapshot: hunk_editor::WorkspaceProjectedSnapshot,
) -> Option<ReviewWorkspaceProjectedSideRows> {
    let mut rows = Vec::<ReviewWorkspaceProjectedSideRow>::new();
    let mut rows_by_display_row = BTreeMap::<usize, hunk_editor::WorkspaceDisplayRow>::new();
    for row in snapshot.visible_rows {
        let Some(workspace_row_range) = row.workspace_row_range else {
            return None;
        };
        if workspace_row_range.is_empty() {
            return None;
        }
        let raw_row_index = workspace_row_range.start;
        let display_row_index = row.row_index;
        let display_row = hunk_editor::WorkspaceDisplayRow {
            row_index: display_row_index,
            location: row.location,
            raw_start_column: row.raw_start_column,
            raw_end_column: row.raw_end_column,
            raw_column_offsets: row.raw_column_offsets,
            text: row.text,
            whitespace_markers: row.whitespace_markers,
            search_highlights: row.search_highlights,
        };
        rows.push(ReviewWorkspaceProjectedSideRow {
            display_row_index,
            row_index: raw_row_index,
            raw_row_range: workspace_row_range,
            row: display_row.clone(),
        });
        rows_by_display_row.insert(display_row_index, display_row);
    }
    Some(ReviewWorkspaceProjectedSideRows {
        rows,
        rows_by_display_row,
    })
}

#[cfg(test)]
fn projected_workspace_display_rows(
    snapshot: hunk_editor::WorkspaceProjectedSnapshot,
) -> Option<Vec<review_workspace_session::ReviewWorkspaceDisplayRowEntry>> {
    projected_review_workspace_side_rows(snapshot).map(|rows| {
        rows.rows
            .into_iter()
            .map(|row| review_workspace_session::ReviewWorkspaceDisplayRowEntry {
                display_row_index: row.display_row_index,
                row_index: row.row_index,
                raw_row_range: row.raw_row_range,
                left: row.row.clone(),
                right: hunk_editor::WorkspaceDisplayRow {
                    row_index: row.display_row_index,
                    location: None,
                    raw_start_column: 0,
                    raw_end_column: 0,
                    raw_column_offsets: vec![0],
                    text: String::new(),
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                },
            })
            .collect()
    })
}

fn review_workspace_display_row_entries_from_projected_sides(
    left: &ReviewWorkspaceProjectedSideRows,
    right: &ReviewWorkspaceProjectedSideRows,
) -> Option<Vec<review_workspace_session::ReviewWorkspaceDisplayRowEntry>> {
    let right_rows_by_display = right
        .rows
        .iter()
        .map(|row| (row.display_row_index, row))
        .collect::<BTreeMap<_, _>>();
    let mut entries = Vec::with_capacity(left.rows.len());
    for left_row in &left.rows {
        let right_row = right_rows_by_display.get(&left_row.display_row_index)?;
        if right_row.raw_row_range != left_row.raw_row_range {
            return None;
        }
        entries.push(review_workspace_session::ReviewWorkspaceDisplayRowEntry {
            display_row_index: left_row.display_row_index,
            row_index: left_row.row_index,
            raw_row_range: left_row.raw_row_range.clone(),
            left: left_row.row.clone(),
            right: right_row.row.clone(),
        });
    }
    Some(entries)
}

fn review_workspace_display_row_entries(
    left_rows: &BTreeMap<usize, hunk_editor::WorkspaceDisplayRow>,
    right_rows: &BTreeMap<usize, hunk_editor::WorkspaceDisplayRow>,
) -> Vec<review_workspace_session::ReviewWorkspaceDisplayRowEntry> {
    left_rows
        .iter()
        .filter_map(|(display_row_index, left)| {
            Some(review_workspace_session::ReviewWorkspaceDisplayRowEntry {
                display_row_index: *display_row_index,
                row_index: *display_row_index,
                raw_row_range: *display_row_index..display_row_index.saturating_add(1),
                left: left.clone(),
                right: right_rows.get(display_row_index)?.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod review_projection_tests {
    use super::*;

    #[test]
    fn projected_workspace_rows_convert_when_mapping_is_one_to_one() {
        let rows = projected_workspace_display_rows(
            hunk_editor::WorkspaceProjectedSnapshot {
                viewport: hunk_editor::Viewport {
                    first_visible_row: 0,
                    visible_row_count: 2,
                    horizontal_offset: 0,
                },
                total_display_rows: 2,
                visible_rows: vec![
                    hunk_editor::WorkspaceProjectedRow {
                        row_index: 0,
                        workspace_row_range: Some(4..5),
                        location: None,
                        kind: hunk_editor::DisplayRowKind::Text,
                        raw_start_column: 0,
                        raw_end_column: 3,
                        raw_column_offsets: vec![0, 1, 2, 3],
                        start_column: 0,
                        end_column: 3,
                        text: "abc".to_string(),
                        is_wrapped: false,
                        whitespace_markers: Vec::new(),
                        search_highlights: Vec::new(),
                        overlays: Vec::new(),
                    },
                    hunk_editor::WorkspaceProjectedRow {
                        row_index: 1,
                        workspace_row_range: Some(5..6),
                        location: None,
                        kind: hunk_editor::DisplayRowKind::Text,
                        raw_start_column: 0,
                        raw_end_column: 2,
                        raw_column_offsets: vec![0, 1, 2],
                        start_column: 0,
                        end_column: 2,
                        text: "de".to_string(),
                        is_wrapped: false,
                        whitespace_markers: Vec::new(),
                        search_highlights: Vec::new(),
                        overlays: Vec::new(),
                    },
                ],
            },
        )
        .expect("one-to-one rows should convert");

        assert_eq!(
            rows.iter().map(|entry| entry.row_index).collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(rows[0].left.text, "abc");
        assert_eq!(rows[0].display_row_index, 0);
    }

    #[test]
    fn projected_workspace_rows_preserve_duplicate_raw_row_mappings() {
        let rows = projected_workspace_display_rows(
            hunk_editor::WorkspaceProjectedSnapshot {
                viewport: hunk_editor::Viewport {
                    first_visible_row: 0,
                    visible_row_count: 2,
                    horizontal_offset: 0,
                },
                total_display_rows: 2,
                visible_rows: vec![
                    hunk_editor::WorkspaceProjectedRow {
                        row_index: 0,
                        workspace_row_range: Some(4..5),
                        location: None,
                        kind: hunk_editor::DisplayRowKind::Text,
                        raw_start_column: 0,
                        raw_end_column: 80,
                        raw_column_offsets: vec![0; 81],
                        start_column: 0,
                        end_column: 80,
                        text: "left".to_string(),
                        is_wrapped: false,
                        whitespace_markers: Vec::new(),
                        search_highlights: Vec::new(),
                        overlays: Vec::new(),
                    },
                    hunk_editor::WorkspaceProjectedRow {
                        row_index: 1,
                        workspace_row_range: Some(4..5),
                        location: None,
                        kind: hunk_editor::DisplayRowKind::Text,
                        raw_start_column: 80,
                        raw_end_column: 120,
                        raw_column_offsets: vec![0; 41],
                        start_column: 80,
                        end_column: 120,
                        text: "right".to_string(),
                        is_wrapped: true,
                        whitespace_markers: Vec::new(),
                        search_highlights: Vec::new(),
                        overlays: Vec::new(),
                    },
                ],
            },
        );

        let rows = rows.expect("projected rows should preserve wrapped display rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].display_row_index, 0);
        assert_eq!(rows[1].display_row_index, 1);
        assert_eq!(rows[0].row_index, 4);
        assert_eq!(rows[1].row_index, 4);
        assert_eq!(rows[0].raw_row_range, 4..5);
        assert_eq!(rows[1].raw_row_range, 4..5);
    }

    #[test]
    fn projected_workspace_rows_preserve_multi_row_raw_ranges() {
        let rows = projected_workspace_display_rows(
            hunk_editor::WorkspaceProjectedSnapshot {
                viewport: hunk_editor::Viewport {
                    first_visible_row: 0,
                    visible_row_count: 1,
                    horizontal_offset: 0,
                },
                total_display_rows: 1,
                visible_rows: vec![hunk_editor::WorkspaceProjectedRow {
                    row_index: 0,
                    workspace_row_range: Some(4..7),
                    location: None,
                    kind: hunk_editor::DisplayRowKind::FoldPlaceholder {
                        hidden_line_count: 2,
                    },
                    raw_start_column: 0,
                    raw_end_column: 18,
                    raw_column_offsets: (0..=18).collect(),
                    start_column: 0,
                    end_column: 18,
                    text: "… 2 hidden lines".to_string(),
                    is_wrapped: false,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: Vec::new(),
                }],
            },
        )
        .expect("fold placeholder rows should convert");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row_index, 4);
        assert_eq!(rows[0].raw_row_range, 4..7);
        assert_eq!(rows[0].display_row_index, 0);
    }

    #[test]
    fn review_workspace_display_row_entries_preserve_row_order() {
        let left_rows = BTreeMap::from([
            (
                4,
                hunk_editor::WorkspaceDisplayRow {
                    row_index: 4,
                    location: None,
                    raw_start_column: 0,
                    raw_end_column: 3,
                    raw_column_offsets: vec![0, 1, 2, 3],
                    text: "abc".to_string(),
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                },
            ),
            (
                5,
                hunk_editor::WorkspaceDisplayRow {
                    row_index: 5,
                    location: None,
                    raw_start_column: 0,
                    raw_end_column: 2,
                    raw_column_offsets: vec![0, 1, 2],
                    text: "de".to_string(),
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                },
            ),
        ]);
        let right_rows = left_rows.clone();

        let entries = review_workspace_display_row_entries(&left_rows, &right_rows);

        assert_eq!(
            entries.iter().map(|entry| entry.row_index).collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.display_row_index)
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
    }

    #[test]
    fn projected_review_workspace_side_rows_merge_by_display_row_index() {
        let left = projected_review_workspace_side_rows(hunk_editor::WorkspaceProjectedSnapshot {
            viewport: hunk_editor::Viewport {
                first_visible_row: 0,
                visible_row_count: 2,
                horizontal_offset: 0,
            },
            total_display_rows: 2,
            visible_rows: vec![
                hunk_editor::WorkspaceProjectedRow {
                    row_index: 0,
                    workspace_row_range: Some(4..5),
                    location: None,
                    kind: hunk_editor::DisplayRowKind::Text,
                    raw_start_column: 0,
                    raw_end_column: 5,
                    raw_column_offsets: vec![0, 1, 2, 3, 4, 5],
                    start_column: 0,
                    end_column: 5,
                    text: "left".to_string(),
                    is_wrapped: false,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: Vec::new(),
                },
                hunk_editor::WorkspaceProjectedRow {
                    row_index: 1,
                    workspace_row_range: Some(4..5),
                    location: None,
                    kind: hunk_editor::DisplayRowKind::Text,
                    raw_start_column: 5,
                    raw_end_column: 9,
                    raw_column_offsets: vec![0, 1, 2, 3, 4],
                    start_column: 5,
                    end_column: 9,
                    text: "wrap".to_string(),
                    is_wrapped: true,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: Vec::new(),
                },
            ],
        })
        .expect("left projected rows should build");
        let right = projected_review_workspace_side_rows(hunk_editor::WorkspaceProjectedSnapshot {
            viewport: hunk_editor::Viewport {
                first_visible_row: 0,
                visible_row_count: 2,
                horizontal_offset: 0,
            },
            total_display_rows: 2,
            visible_rows: vec![
                hunk_editor::WorkspaceProjectedRow {
                    row_index: 0,
                    workspace_row_range: Some(4..5),
                    location: None,
                    kind: hunk_editor::DisplayRowKind::Text,
                    raw_start_column: 0,
                    raw_end_column: 5,
                    raw_column_offsets: vec![0, 1, 2, 3, 4, 5],
                    start_column: 0,
                    end_column: 5,
                    text: "right".to_string(),
                    is_wrapped: false,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: Vec::new(),
                },
                hunk_editor::WorkspaceProjectedRow {
                    row_index: 1,
                    workspace_row_range: Some(4..5),
                    location: None,
                    kind: hunk_editor::DisplayRowKind::Text,
                    raw_start_column: 5,
                    raw_end_column: 9,
                    raw_column_offsets: vec![0, 1, 2, 3, 4],
                    start_column: 5,
                    end_column: 9,
                    text: "cell".to_string(),
                    is_wrapped: true,
                    whitespace_markers: Vec::new(),
                    search_highlights: Vec::new(),
                    overlays: Vec::new(),
                },
            ],
        })
        .expect("right projected rows should build");

        let entries = review_workspace_display_row_entries_from_projected_sides(&left, &right)
            .expect("projected sides should merge");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].display_row_index, 0);
        assert_eq!(entries[1].display_row_index, 1);
        assert_eq!(entries[0].row_index, 4);
        assert_eq!(entries[1].row_index, 4);
        assert_eq!(entries[0].raw_row_range, 4..5);
        assert_eq!(entries[1].raw_row_range, 4..5);
        assert_eq!(entries[0].left.row_index, 0);
        assert_eq!(entries[1].left.row_index, 1);
        assert_eq!(entries[0].right.row_index, 0);
        assert_eq!(entries[1].right.row_index, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::DiffViewer;
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind};
    use notify::{Event, EventKind};
    use std::path::PathBuf;

    fn fixture_repo_root() -> PathBuf {
        std::env::temp_dir().join("hunk-watch-path-tests")
    }

    #[test]
    fn ignores_internal_vcs_paths_for_repo_watch() {
        let repo_root = fixture_repo_root();
        assert!(!DiffViewer::should_ignore_repo_watch_path(
            repo_root.join(".git/index").as_path(),
            repo_root.as_path()
        ));
        assert!(DiffViewer::is_repo_watch_metadata_path(
            repo_root.join(".git/index").as_path(),
            repo_root.as_path()
        ));
    }

    #[test]
    fn excludes_internal_vcs_paths_from_dirty_file_tracking() {
        let repo_root = fixture_repo_root();
        assert_eq!(
            DiffViewer::repo_watch_dirty_path(
                repo_root.join(".git/index").as_path(),
                repo_root.as_path()
            ),
            None
        );
    }

    #[test]
    fn ignores_hunk_temp_save_paths_for_repo_watch() {
        let repo_root = fixture_repo_root();
        assert!(DiffViewer::should_ignore_repo_watch_path(
            repo_root.join("src/lib.rs.hunk-tmp.123.456").as_path(),
            repo_root.as_path()
        ));
        assert!(DiffViewer::should_ignore_repo_watch_path(
            repo_root.join(".hunk-tmp.1.2").as_path(),
            repo_root.as_path()
        ));
    }

    #[test]
    fn keeps_repo_local_hunkdiff_paths_for_repo_watch() {
        let repo_root = fixture_repo_root();
        assert!(!DiffViewer::should_ignore_repo_watch_path(
            repo_root
                .join(".hunkdiff/worktrees/feature-one/src/lib.rs")
                .as_path(),
            repo_root.as_path()
        ));
        assert_eq!(
            DiffViewer::repo_watch_dirty_path(
                repo_root
                    .join(".hunkdiff/worktrees/feature-one/src/lib.rs")
                    .as_path(),
                repo_root.as_path()
            ),
            Some(".hunkdiff/worktrees/feature-one/src/lib.rs".to_string())
        );
    }

    #[test]
    fn keeps_regular_workspace_paths_for_repo_watch() {
        let repo_root = fixture_repo_root();
        assert!(!DiffViewer::should_ignore_repo_watch_path(
            repo_root.join("src/lib.rs").as_path(),
            repo_root.as_path()
        ));
        assert!(!DiffViewer::should_ignore_repo_watch_path(
            repo_root.join("x.md").as_path(),
            repo_root.as_path()
        ));
        assert!(!DiffViewer::should_ignore_repo_watch_path(
            repo_root.join("src/notes.hunk-tmp.md").as_path(),
            repo_root.as_path()
        ));
    }

    #[test]
    fn repo_watch_roots_include_selected_worktree_root() {
        let repo_root = fixture_repo_root();
        let worktree_root = std::env::temp_dir().join("hunk-watch-path-tests-worktree");

        assert_eq!(
            DiffViewer::repo_watch_roots(Some(repo_root.as_path()), Some(worktree_root.as_path())),
            vec![repo_root, worktree_root]
        );
    }

    #[test]
    fn repo_watch_roots_deduplicate_primary_checkout() {
        let repo_root = fixture_repo_root();

        assert_eq!(
            DiffViewer::repo_watch_roots(Some(repo_root.as_path()), Some(repo_root.as_path())),
            vec![repo_root]
        );
    }

    #[test]
    fn repo_watch_roots_skip_nested_worktree_under_primary_root() {
        let repo_root = fixture_repo_root();
        let worktree_root = repo_root.join("linked-worktree");

        assert_eq!(
            DiffViewer::repo_watch_roots(Some(repo_root.as_path()), Some(worktree_root.as_path())),
            vec![repo_root]
        );
    }

    #[test]
    fn worktree_watch_refreshes_git_workspace_for_selected_root_dirty_paths() {
        let repo_root = fixture_repo_root();
        let worktree_root = std::env::temp_dir().join("hunk-watch-path-tests-worktree");
        let event_paths = vec![worktree_root.join("src/lib.rs")];

        assert!(DiffViewer::should_refresh_git_workspace_from_repo_watch(
            event_paths.as_slice(),
            repo_root.as_path(),
            worktree_root.as_path(),
            None,
        ));
    }

    #[test]
    fn worktree_watch_refreshes_git_workspace_for_primary_metadata_changes() {
        let repo_root = fixture_repo_root();
        let worktree_root = std::env::temp_dir().join("hunk-watch-path-tests-worktree");
        let event_paths = vec![repo_root.join(".git/worktrees/worktree-1/index")];

        assert!(DiffViewer::should_refresh_git_workspace_from_repo_watch(
            event_paths.as_slice(),
            repo_root.as_path(),
            worktree_root.as_path(),
            None,
        ));
    }

    #[test]
    fn repo_watch_detects_metadata_changes_for_selected_worktree_root() {
        let worktree_root = std::env::temp_dir().join("hunk-watch-path-tests-worktree");
        let event_paths = vec![worktree_root.join(".git/HEAD")];

        assert!(DiffViewer::repo_watch_metadata_changed(
            event_paths.as_slice(),
            Some(worktree_root.as_path())
        ));
    }

    #[test]
    fn repo_watch_index_metadata_does_not_refresh_recent_commits() {
        let repo_root = fixture_repo_root();
        let event_paths = vec![repo_root.join(".git/index")];

        assert!(!DiffViewer::repo_watch_recent_commits_changed(
            event_paths.as_slice(),
            Some(repo_root.as_path())
        ));
    }

    #[test]
    fn repo_watch_head_metadata_refreshes_recent_commits() {
        let repo_root = fixture_repo_root();
        let event_paths = vec![repo_root.join(".git/HEAD")];

        assert!(DiffViewer::repo_watch_recent_commits_changed(
            event_paths.as_slice(),
            Some(repo_root.as_path())
        ));
    }

    #[test]
    fn repo_watch_linked_worktree_index_does_not_refresh_recent_commits() {
        let repo_root = fixture_repo_root();
        let event_paths = vec![repo_root.join(".git/worktrees/feature-one/index")];

        assert!(!DiffViewer::repo_watch_recent_commits_changed(
            event_paths.as_slice(),
            Some(repo_root.as_path())
        ));
    }

    #[test]
    fn repo_watch_linked_worktree_head_refreshes_recent_commits() {
        let repo_root = fixture_repo_root();
        let event_paths = vec![repo_root.join(".git/worktrees/feature-one/HEAD")];

        assert!(DiffViewer::repo_watch_recent_commits_changed(
            event_paths.as_slice(),
            Some(repo_root.as_path())
        ));
    }

    #[test]
    fn repo_watch_ignores_access_events() {
        assert!(!DiffViewer::should_process_repo_watch_event(
            &Event::new(EventKind::Access(AccessKind::Open(AccessMode::Read)))
        ));
        assert!(!DiffViewer::should_process_repo_watch_event(
            &Event::new(EventKind::Access(AccessKind::Close(AccessMode::Write)))
        ));
    }

    #[test]
    fn repo_watch_processes_mutating_events() {
        assert!(DiffViewer::should_process_repo_watch_event(&Event::new(
            EventKind::Create(CreateKind::File)
        )));
        assert!(DiffViewer::should_process_repo_watch_event(&Event::new(
            EventKind::Modify(ModifyKind::Data(DataChange::Any))
        )));
        assert!(DiffViewer::should_process_repo_watch_event(&Event::new(
            EventKind::Remove(RemoveKind::File)
        )));
    }

}
