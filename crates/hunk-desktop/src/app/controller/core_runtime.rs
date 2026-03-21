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

    fn uses_selective_repo_watch() -> bool {
        cfg!(target_os = "linux")
    }

    fn should_descend_into_repo_watch_dir(
        path: &std::path::Path,
        repo_root: &std::path::Path,
        ignore_matcher: Option<&hunk_git::git::RepoIgnoreMatcher>,
    ) -> bool {
        let Ok(relative_path) = path.strip_prefix(repo_root) else {
            return false;
        };
        if relative_path.as_os_str().is_empty() {
            return true;
        }

        let first_component = relative_path.components().next();
        if relative_path.components().any(|component| component.as_os_str() == ".git")
            && first_component.is_none_or(|component| component.as_os_str() != ".git")
        {
            return false;
        }

        if Self::should_ignore_repo_watch_path(path, repo_root) {
            return false;
        }

        let relative_path = relative_path.to_string_lossy().replace('\\', "/");
        match ignore_matcher {
            Some(ignore_matcher) => match ignore_matcher.is_path_ignored(relative_path.as_str(), true) {
                Ok(is_ignored) => !is_ignored,
                Err(err) => {
                    warn!(
                        "failed to evaluate repo watch directory ignore rules for {} in {}: {err:#}",
                        relative_path,
                        repo_root.display()
                    );
                    true
                }
            },
            None => true,
        }
    }

    fn repo_watch_directories_for_root(
        repo_root: &std::path::Path,
        start_dir: &std::path::Path,
        ignore_matcher: Option<&hunk_git::git::RepoIgnoreMatcher>,
    ) -> Vec<std::path::PathBuf> {
        let mut pending = vec![start_dir.to_path_buf()];
        let mut directories = BTreeSet::new();

        while let Some(dir) = pending.pop() {
            if !directories.insert(dir.clone()) {
                continue;
            }

            let entries = match std::fs::read_dir(dir.as_path()) {
                Ok(entries) => entries,
                Err(err) => {
                    warn!(
                        "failed to enumerate repo watch directory {}: {err:#}",
                        dir.display()
                    );
                    continue;
                }
            };

            for entry_result in entries {
                let entry = match entry_result {
                    Ok(entry) => entry,
                    Err(err) => {
                        warn!(
                            "failed to read repo watch directory entry under {}: {err:#}",
                            dir.display()
                        );
                        continue;
                    }
                };
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(err) => {
                        warn!(
                            "failed to read repo watch entry type for {}: {err:#}",
                            entry.path().display()
                        );
                        continue;
                    }
                };
                if !file_type.is_dir() {
                    continue;
                }

                let child_path = entry.path();
                if !Self::should_descend_into_repo_watch_dir(
                    child_path.as_path(),
                    repo_root,
                    ignore_matcher,
                ) {
                    continue;
                }
                pending.push(child_path);
            }
        }

        directories.into_iter().collect()
    }

    fn watch_repo_directories(
        watcher: &mut notify::RecommendedWatcher,
        directories: &[std::path::PathBuf],
        watched_directories: &mut BTreeSet<std::path::PathBuf>,
    ) -> notify::Result<()> {
        for directory in directories {
            if watched_directories.insert(directory.clone()) {
                watcher.watch(directory, notify::RecursiveMode::NonRecursive)?;
            }
        }
        Ok(())
    }

    fn register_repo_watch_root(
        watcher: &mut notify::RecommendedWatcher,
        watch_root: &std::path::Path,
        ignore_matcher: Option<&hunk_git::git::RepoIgnoreMatcher>,
        watched_directories: &mut BTreeSet<std::path::PathBuf>,
    ) -> notify::Result<()> {
        if Self::uses_selective_repo_watch() {
            let directories =
                Self::repo_watch_directories_for_root(watch_root, watch_root, ignore_matcher);
            return Self::watch_repo_directories(
                watcher,
                directories.as_slice(),
                watched_directories,
            );
        }

        watcher.watch(watch_root, notify::RecursiveMode::Recursive)
    }

    fn register_repo_watch_directories_from_event(
        watcher: &mut notify::RecommendedWatcher,
        event_paths: &[std::path::PathBuf],
        repo_root: Option<&std::path::Path>,
        ignore_matcher: Option<&hunk_git::git::RepoIgnoreMatcher>,
        watched_directories: &mut BTreeSet<std::path::PathBuf>,
    ) {
        if !Self::uses_selective_repo_watch() {
            return;
        }

        let Some(repo_root) = repo_root else {
            return;
        };

        for event_path in event_paths {
            let metadata = match std::fs::symlink_metadata(event_path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if !metadata.is_dir() || !event_path.starts_with(repo_root) {
                continue;
            }
            if !Self::should_descend_into_repo_watch_dir(
                event_path.as_path(),
                repo_root,
                ignore_matcher,
            ) {
                continue;
            }

            let directories = Self::repo_watch_directories_for_root(
                repo_root,
                event_path.as_path(),
                ignore_matcher,
            );
            if let Err(err) = Self::watch_repo_directories(
                watcher,
                directories.as_slice(),
                watched_directories,
            ) {
                warn!(
                    "failed to extend file watch from {} under {}: {err}",
                    event_path.display(),
                    repo_root.display()
                );
            }
        }
    }

    fn unregister_repo_watch_directories_from_event(
        event: &notify::Event,
        watched_directories: &mut BTreeSet<std::path::PathBuf>,
    ) {
        if !Self::uses_selective_repo_watch() {
            return;
        }

        let removed_roots = match event.kind {
            notify::EventKind::Remove(_) => event.paths.clone(),
            notify::EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::From,
            )) => event.paths.first().cloned().into_iter().collect(),
            notify::EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Both,
            )) => event.paths.first().cloned().into_iter().collect(),
            _ => Vec::new(),
        };
        if removed_roots.is_empty() {
            return;
        }

        watched_directories.retain(|watched_directory| {
            removed_roots.iter().all(|removed_root| {
                watched_directory != removed_root && !watched_directory.starts_with(removed_root)
            })
        });
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
        let mut watched_directories = BTreeSet::new();

        for watch_root in &watch_roots {
            let ignore_matcher = if primary_root.as_deref() == Some(watch_root.as_path()) {
                primary_ignore_matcher.as_ref()
            } else if git_workspace_root.as_deref() == Some(watch_root.as_path()) {
                git_workspace_ignore_matcher.as_ref()
            } else {
                None
            };
            if let Err(err) = Self::register_repo_watch_root(
                &mut watcher,
                watch_root,
                ignore_matcher,
                &mut watched_directories,
            ) {
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

                Self::unregister_repo_watch_directories_from_event(
                    &event,
                    &mut watched_directories,
                );
                Self::register_repo_watch_directories_from_event(
                    &mut watcher,
                    event.paths.as_slice(),
                    primary_root.as_deref(),
                    primary_ignore_matcher.as_ref(),
                    &mut watched_directories,
                );
                Self::register_repo_watch_directories_from_event(
                    &mut watcher,
                    event.paths.as_slice(),
                    git_workspace_root.as_deref(),
                    git_workspace_ignore_matcher.as_ref(),
                    &mut watched_directories,
                );

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
    use git2::Repository;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_repo_root() -> PathBuf {
        std::env::temp_dir().join("hunk-watch-path-tests")
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("hunk-{label}-{}-{nanos}", std::process::id()))
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
    fn selective_repo_watch_skips_gitignored_directories() {
        let repo_root = unique_temp_dir("watch-ignore");
        fs::create_dir_all(repo_root.as_path()).expect("create repo root");
        let repo = Repository::init(repo_root.as_path()).expect("init repo");
        let repo_root = repo
            .workdir()
            .expect("repo workdir")
            .to_path_buf();

        fs::write(repo_root.join(".gitignore"), "target-shared/\n").expect("write gitignore");
        fs::create_dir_all(repo_root.join("src/nested")).expect("create source directories");
        fs::create_dir_all(repo_root.join("target-shared/dist/runtime"))
            .expect("create ignored directories");

        let ignore_matcher =
            hunk_git::git::RepoIgnoreMatcher::open(repo_root.as_path()).expect("open matcher");
        let watched = DiffViewer::repo_watch_directories_for_root(
            repo_root.as_path(),
            repo_root.as_path(),
            Some(&ignore_matcher),
        );
        drop(repo);

        assert!(watched.contains(&repo_root));
        assert!(watched.contains(&repo_root.join("src")));
        assert!(watched.contains(&repo_root.join("src/nested")));
        assert!(!watched.contains(&repo_root.join("target-shared")));
        assert!(!watched.contains(&repo_root.join("target-shared/dist")));

        fs::remove_dir_all(repo_root).expect("remove temp repo");
    }

    #[test]
    fn selective_repo_watch_skips_nested_git_directories() {
        let repo_root = unique_temp_dir("watch-nested-git");
        fs::create_dir_all(repo_root.as_path()).expect("create repo root");
        let repo = Repository::init(repo_root.as_path()).expect("init repo");
        let repo_root = repo
            .workdir()
            .expect("repo workdir")
            .to_path_buf();

        fs::create_dir_all(repo_root.join("vendor/example/.git/objects"))
            .expect("create nested git directory");

        let watched = DiffViewer::repo_watch_directories_for_root(
            repo_root.as_path(),
            repo_root.as_path(),
            None,
        );
        drop(repo);

        assert!(watched.contains(&repo_root.join(".git")));
        assert!(watched.contains(&repo_root.join("vendor")));
        assert!(watched.contains(&repo_root.join("vendor/example")));
        assert!(!watched.contains(&repo_root.join("vendor/example/.git")));
        assert!(!watched.contains(&repo_root.join("vendor/example/.git/objects")));

        fs::remove_dir_all(repo_root).expect("remove temp repo");
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

}
