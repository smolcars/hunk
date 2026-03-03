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

    fn auto_refresh_interval(&self) -> Duration {
        if self.config.auto_refresh_interval_ms == 0 {
            return Duration::ZERO;
        }

        let base_ms = self.config.auto_refresh_interval_ms.max(250);
        let backoff_factor =
            1_u64 << self.auto_refresh_unmodified_streak.min(Self::AUTO_REFRESH_BACKOFF_STEPS);
        let interval_ms = (base_ms.saturating_mul(backoff_factor))
            .min(Self::AUTO_REFRESH_MAX_INTERVAL_MS);
        Duration::from_millis(interval_ms)
    }

    fn should_ignore_repo_watch_path(path: &std::path::Path, repo_root: &std::path::Path) -> bool {
        let Ok(relative_path) = path.strip_prefix(repo_root) else {
            return false;
        };

        relative_path.components().any(|component| {
            let component = component.as_os_str();
            if component == ".jj" || component == ".git" {
                return true;
            }

            component
                .to_str()
                .is_some_and(Self::is_hunk_temp_save_component)
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

        let Some(repo_root) = self.repo_root.clone().or_else(|| self.project_path.clone()) else {
            return;
        };
        let (event_tx, mut event_rx) = mpsc::unbounded::<notify::Result<notify::Event>>();
        let repo_root_path = repo_root.clone();
        let repo_root_for_cb = repo_root.to_string_lossy().to_string();
        let watcher = notify::recommended_watcher(move |result| {
            event_tx.unbounded_send(result).ok();
        });

        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(err) => {
                error!("failed to start file watch for {}: {err}", repo_root_for_cb);
                return;
            }
        };

        if let Err(err) = watcher.watch(&repo_root, notify::RecursiveMode::Recursive) {
            error!("failed to watch repository at {}: {err}", repo_root_for_cb);
            return;
        }

        self.repo_watch_task = cx.spawn(async move |this, cx| {
            while let Some(event) = event_rx.next().await {
                let Ok(event) = event else {
                    continue;
                };

                if event
                    .paths
                    .iter()
                    .all(|path| Self::should_ignore_repo_watch_path(path, &repo_root_path))
                {
                    continue;
                }

                if let Some(this) = this.upgrade() {
                    this.update(cx, |this, cx| {
                        this.schedule_repo_watch_refresh(cx);
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

    fn schedule_repo_watch_refresh(&mut self, cx: &mut Context<Self>) {
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
                    this.request_snapshot_refresh_internal(true, cx);
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
                        this.request_snapshot_refresh(cx);
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
        assert!(DiffViewer::should_ignore_repo_watch_path(
            repo_root.join(".jj/working_copy").as_path(),
            repo_root.as_path()
        ));
        assert!(DiffViewer::should_ignore_repo_watch_path(
            repo_root.join(".git/index").as_path(),
            repo_root.as_path()
        ));
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
}
