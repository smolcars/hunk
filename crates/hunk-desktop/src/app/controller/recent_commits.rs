impl DiffViewer {
    fn hydrate_recent_commits_cache_if_available(&mut self, cx: &mut Context<Self>) {
        let Some(expected_root) = self
            .project_path
            .clone()
            .or_else(|| self.state.active_project_path().cloned())
        else {
            return;
        };
        let cache_key = expected_root.to_string_lossy().to_string();
        let Some(cache) = self
            .state
            .git_recent_commits_cache_by_repo
            .get(cache_key.as_str())
            .cloned()
        else {
            return;
        };
        let Some(root) = cache.root.clone() else {
            return;
        };
        let cached_project_root =
            hunk_git::worktree::primary_repo_root(root.as_path()).unwrap_or_else(|_| root.clone());
        if cached_project_root != expected_root {
            return;
        }
        let Ok(current_fingerprint) = load_recent_authored_commits_fingerprint(
            root.as_path(),
            DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
        ) else {
            return;
        };
        if current_fingerprint.root() != root.as_path()
            || current_fingerprint.head_ref_name() != cache.head_ref_name.as_deref()
            || current_fingerprint.head_commit_id() != cache.head_commit_id.as_deref()
            || current_fingerprint.base_tip_id() != cache.base_tip_id.as_deref()
        {
            debug!(
                "skipping recent commits cache hydration for {} due to scope mismatch",
                root.display()
            );
            return;
        }

        self.recent_commits = cache
            .commits
            .into_iter()
            .map(|commit| RecentCommitSummary {
                commit_id: commit.commit_id,
                subject: commit.subject,
                committed_unix_time: commit.committed_unix_time,
            })
            .collect();
        self.recent_commits_error = None;
        debug!(
            "hydrated recent commits cache for {} (commits={})",
            root.display(),
            self.recent_commits.len(),
        );
        cx.notify();
    }

    fn persist_recent_commits_cache(&mut self) {
        let Some(root) = self.selected_git_workspace_root() else {
            return;
        };
        let Some(cache_key) = self.current_workspace_project_key() else {
            return;
        };

        let mut cache = CachedRecentCommitsState {
            root: Some(root),
            head_ref_name: self
                .last_recent_commits_fingerprint
                .as_ref()
                .and_then(|fingerprint| fingerprint.head_ref_name().map(str::to_string)),
            head_commit_id: self
                .last_recent_commits_fingerprint
                .as_ref()
                .and_then(|fingerprint| fingerprint.head_commit_id().map(str::to_string)),
            base_tip_id: self
                .last_recent_commits_fingerprint
                .as_ref()
                .and_then(|fingerprint| fingerprint.base_tip_id().map(str::to_string)),
            commits: self
                .recent_commits
                .iter()
                .map(|commit| CachedRecentCommitState {
                    commit_id: commit.commit_id.clone(),
                    subject: commit.subject.clone(),
                    committed_unix_time: commit.committed_unix_time,
                })
                .collect(),
            cached_unix_time: 0,
        };

        if let Some(previous) = self
            .state
            .git_recent_commits_cache_by_repo
            .get(cache_key.as_str())
        {
            let mut previous_without_time = previous.clone();
            previous_without_time.cached_unix_time = 0;
            if previous_without_time == cache {
                return;
            }
        }

        cache.cached_unix_time = Self::workflow_cache_unix_time();
        self.state
            .git_recent_commits_cache_by_repo
            .insert(cache_key, cache);
        self.persist_state();
    }

    fn clear_recent_commits_cache(&mut self) {
        let Some(cache_key) = self.current_workspace_project_key() else {
            return;
        };

        if self
            .state
            .git_recent_commits_cache_by_repo
            .remove(cache_key.as_str())
            .is_some()
        {
            self.persist_state();
        }
    }

    fn reset_recent_commits_state(&mut self) {
        self.next_recent_commits_epoch();
        self.recent_commits.clear();
        self.recent_commits_error = None;
        self.recent_commits_task = Task::ready(());
        self.recent_commits_loading = false;
        self.recent_commits_active_request = None;
        self.pending_recent_commits_refresh = None;
        self.last_recent_commits_fingerprint = None;
    }

    fn next_recent_commits_epoch(&mut self) -> usize {
        self.recent_commits_epoch = self.recent_commits_epoch.saturating_add(1);
        self.recent_commits_epoch
    }

    fn enqueue_recent_commits_refresh(&mut self, request: RecentCommitsRefreshRequest) {
        self.pending_recent_commits_refresh = Some(
            self.pending_recent_commits_refresh
                .map_or(request, |pending| pending.merge(request)),
        );
    }

    fn active_recent_commits_refresh_request(&self) -> RecentCommitsRefreshRequest {
        self.recent_commits_active_request
            .unwrap_or(RecentCommitsRefreshRequest::background())
    }

    fn finish_recent_commits_refresh_loading(&mut self) {
        self.recent_commits_loading = false;
        self.recent_commits_active_request = None;
    }

    fn maybe_run_pending_recent_commits_refresh(&mut self, cx: &mut Context<Self>) {
        if self.recent_commits_loading {
            return;
        }
        let Some(request) = self.pending_recent_commits_refresh.take() else {
            return;
        };
        debug!(
            "recent commits running queued refresh: force={} priority={}",
            request.force,
            request.priority.as_str()
        );
        self.request_recent_commits_refresh_internal(request, cx);
    }

    fn request_recent_commits_refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        let request = if force {
            RecentCommitsRefreshRequest::user(true)
        } else {
            RecentCommitsRefreshRequest::background()
        };
        self.request_recent_commits_refresh_internal(request, cx);
    }

    fn request_recent_commits_refresh_internal(
        &mut self,
        request: RecentCommitsRefreshRequest,
        cx: &mut Context<Self>,
    ) {
        let request = self
            .pending_recent_commits_refresh
            .take()
            .map_or(request, |pending| request.merge(pending));

        if self.recent_commits_loading {
            if request.is_more_urgent_than(self.active_recent_commits_refresh_request()) {
                debug!(
                    "recent commits refresh preempted: epoch={} active_priority={} next_priority={} force={}",
                    self.recent_commits_epoch,
                    self.active_recent_commits_refresh_request().priority.as_str(),
                    request.priority.as_str(),
                    request.force
                );
                self.recent_commits_task = Task::ready(());
                self.recent_commits_active_request = None;
            } else {
                self.enqueue_recent_commits_refresh(request);
                debug!(
                    "recent commits refresh deferred: queued refresh while epoch={} is still loading (force={} priority={})",
                    self.recent_commits_epoch,
                    request.force,
                    request.priority.as_str()
                );
                return;
            }
        }

        let source_dir_result = self
            .selected_git_workspace_root()
            .map(Ok)
            .unwrap_or_else(|| std::env::current_dir().context("failed to resolve current directory"));
        let previous_fingerprint = if request.force {
            None
        } else {
            self.last_recent_commits_fingerprint.clone()
        };
        let epoch = self.next_recent_commits_epoch();
        self.recent_commits_loading = true;
        self.recent_commits_active_request = Some(request);
        let show_loading_state = self.recent_commits.is_empty();
        let refresh_root = self
            .selected_git_workspace_root()
            .unwrap_or_else(|| PathBuf::from("."));
        debug!(
            "recent commits refresh start: epoch={} force={} priority={} root={}",
            epoch,
            request.force,
            request.priority.as_str(),
            refresh_root.display()
        );
        if show_loading_state {
            cx.notify();
        }

        self.recent_commits_task = cx.spawn(async move |this, cx| {
            let started_at = Instant::now();
            let result = match source_dir_result {
                Ok(source_dir) => {
                    cx.background_executor()
                        .spawn(async move {
                            if let Some(previous_fingerprint) = previous_fingerprint.as_ref() {
                                load_recent_authored_commits_if_changed(
                                    &source_dir,
                                    DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
                                    Some(previous_fingerprint),
                                )
                            } else {
                                load_recent_authored_commits_with_fingerprint(
                                    &source_dir,
                                    DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT,
                                )
                                .map(|(fingerprint, snapshot)| (fingerprint, Some(snapshot)))
                            }
                        })
                        .await
                }
                Err(err) => Err(err),
            };

            if let Some(this) = this.upgrade() {
                this.update(cx, move |this, cx| {
                    if epoch != this.recent_commits_epoch {
                        return;
                    }

                    this.finish_recent_commits_refresh_loading();
                    match result {
                        Ok((fingerprint, Some(snapshot))) => {
                            debug!(
                                "recent commits refresh complete: epoch={} force={} priority={} elapsed_ms={} commits={}",
                                epoch,
                                request.force,
                                request.priority.as_str(),
                                started_at.elapsed().as_millis(),
                                snapshot.commits.len()
                            );
                            this.last_recent_commits_fingerprint = Some(fingerprint);
                            this.recent_commits = snapshot.commits;
                            this.recent_commits_error = None;
                            this.persist_recent_commits_cache();
                        }
                        Ok((fingerprint, None)) => {
                            debug!(
                                "recent commits refresh skipped: epoch={} force={} priority={} elapsed_ms={} (no ref changes)",
                                epoch,
                                request.force,
                                request.priority.as_str(),
                                started_at.elapsed().as_millis()
                            );
                            this.last_recent_commits_fingerprint = Some(fingerprint);
                            this.recent_commits_error = None;
                        }
                        Err(err) => {
                            error!(
                                "recent commits refresh failed: epoch={} force={} priority={} elapsed_ms={} err={err:#}",
                                epoch,
                                request.force,
                                request.priority.as_str(),
                                started_at.elapsed().as_millis()
                            );
                            if Self::is_missing_repository_error(&err) {
                                this.reset_recent_commits_state();
                                this.clear_recent_commits_cache();
                            } else {
                                this.recent_commits_error = Some(Self::format_error_chain(&err));
                            }
                        }
                    }

                    cx.notify();
                    this.maybe_run_pending_recent_commits_refresh(cx);
                });
            }
        });
    }

    fn apply_optimistic_recent_commit(
        &mut self,
        commit: &hunk_git::mutation::CreatedCommit,
    ) {
        self.recent_commits
            .retain(|existing| existing.commit_id != commit.commit_id);
        self.recent_commits.insert(
            0,
            RecentCommitSummary {
                commit_id: commit.commit_id.clone(),
                subject: commit.subject.clone(),
                committed_unix_time: commit.committed_unix_time,
            },
        );
        self.recent_commits
            .truncate(DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT);
        self.recent_commits_error = None;
        self.persist_recent_commits_cache();
    }
}
