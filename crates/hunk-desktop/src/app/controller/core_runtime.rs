impl DiffViewer {
    fn recompute_diff_visible_header_lookup(&mut self) {
        let row_count = self.diff_rows.len();
        self.diff_visible_file_header_lookup = vec![None; row_count];
        self.diff_visible_hunk_header_lookup = vec![None; row_count];
        if row_count == 0 {
            return;
        }

        if self.diff_row_metadata.len() == row_count {
            let mut current_file_header = None::<usize>;
            let mut current_hunk_header = None::<usize>;
            for row_ix in 0..row_count {
                let meta = &self.diff_row_metadata[row_ix];
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
                        if current_file_header.is_none() {
                            current_file_header = self.file_row_ranges.iter().find_map(|range| {
                                if row_ix >= range.start_row && row_ix < range.end_row {
                                    Some(range.start_row)
                                } else {
                                    None
                                }
                            });
                        }
                        current_hunk_header = Some(row_ix);
                    }
                    _ => {}
                }

                self.diff_visible_file_header_lookup[row_ix] = current_file_header;
                self.diff_visible_hunk_header_lookup[row_ix] = current_hunk_header;
            }
            return;
        }

        let mut current_hunk_header = None::<usize>;
        for row_ix in 0..row_count {
            if self
                .diff_rows
                .get(row_ix)
                .is_some_and(|row| row.kind == DiffRowKind::HunkHeader)
            {
                current_hunk_header = Some(row_ix);
            }

            let file_header_ix = self.file_row_ranges.iter().find_map(|range| {
                if row_ix >= range.start_row && row_ix < range.end_row {
                    Some(range.start_row)
                } else {
                    None
                }
            });
            self.diff_visible_file_header_lookup[row_ix] = file_header_ix;
            self.diff_visible_hunk_header_lookup[row_ix] = current_hunk_header;
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
    ) -> bool {
        if primary_root == git_workspace_root {
            return false;
        }

        event_paths.iter().any(|path| {
            Self::is_repo_watch_metadata_path(path, primary_root)
                || Self::is_repo_watch_metadata_path(path, git_workspace_root)
                || Self::repo_watch_dirty_path(path, git_workspace_root).is_some()
        })
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

        self.repo_watch_task = cx.spawn(async move |this, cx| {
            while let Some(event) = event_rx.next().await {
                let Ok(event) = event else {
                    continue;
                };

                if event.paths.is_empty() {
                    continue;
                }

                let metadata_changed = primary_root.as_ref().is_some_and(|root| {
                    event
                        .paths
                        .iter()
                        .any(|path| Self::is_repo_watch_metadata_path(path, root))
                });
                let dirty_paths = primary_root
                    .as_ref()
                    .map(|root| {
                        event
                            .paths
                            .iter()
                            .filter_map(|path| Self::repo_watch_dirty_path(path, root))
                            .collect::<BTreeSet<_>>()
                    })
                    .unwrap_or_default();
                let request = repo_watch_refresh_request(metadata_changed, !dirty_paths.is_empty());
                let refresh_git_workspace = primary_root
                    .as_ref()
                    .zip(git_workspace_root.as_ref())
                    .is_some_and(|(primary_root, git_workspace_root)| {
                        Self::should_refresh_git_workspace_from_repo_watch(
                            event.paths.as_slice(),
                            primary_root,
                            git_workspace_root,
                        )
                    });
                if request.is_none() && !refresh_git_workspace {
                    continue;
                }

                if let Some(this) = this.upgrade() {
                    let primary_root = primary_root.clone();
                    this.update(cx, move |this, cx| {
                        if metadata_changed
                            && let Some(primary_root) = primary_root.as_ref()
                        {
                            invalidate_repo_metadata_caches(primary_root.as_path());
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

#[cfg(test)]
mod tests {
    use super::DiffViewer;
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
            worktree_root.as_path()
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
            worktree_root.as_path()
        ));
    }

}
