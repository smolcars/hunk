impl DiffViewer {
    fn request_selected_diff_reload(&mut self, cx: &mut Context<Self>) {
        if self.workspace_view_mode == WorkspaceViewMode::Diff {
            self.request_review_compare_refresh(cx);
            return;
        }

        let Some(repo_root) = self.repo_root.clone() else {
            self.cancel_patch_reload();
            self.comments_cache.clear();
            self.comment_miss_streaks.clear();
            self.reset_comment_row_match_cache();
            self.clear_comment_ui_state();
            self.reset_diff_surface_rows(Vec::new());
            self.file_line_stats.clear();
            self.recompute_overall_line_stats_from_file_stats();
            return;
        };

        if self.files.is_empty() {
            self.cancel_patch_reload();
            self.reset_diff_surface_rows(vec![message_row(DiffRowKind::Empty, "No changed files.")]);
            self.file_line_stats.clear();
            self.recompute_overall_line_stats_from_file_stats();
            self.reconcile_comments_with_loaded_diff();
            cx.notify();
            return;
        }

        let files = self.files.clone();
        let collapsed_files = self.collapsed_files.clone();
        let previous_file_line_stats = self.file_line_stats.clone();
        let expanded_files = files
            .iter()
            .filter(|file| !collapsed_files.contains(file.path.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let initial_files =
            Self::select_initial_diff_files(&expanded_files, self.selected_path.as_deref());
        let initial_paths = initial_files
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        let remaining_files = expanded_files
            .into_iter()
            .filter(|file| !initial_paths.contains(file.path.as_str()))
            .collect::<Vec<_>>();
        let epoch = self.next_patch_epoch();
        self.invalidate_segment_prefetch();
        self.patch_loading = true;
        if self.diff_rows.is_empty() {
            self.reset_diff_surface_rows(vec![message_row(
                DiffRowKind::Meta,
                format!("Loading diffs for {} files...", files.len()),
            )]);
            cx.notify();
        }

        enum PatchProgressUpdate {
            Loaded {
                batch_ix: Option<usize>,
                total_batches: usize,
                elapsed: Duration,
                stream: DiffStream,
                pending_files: usize,
                finished: bool,
            },
            Error {
                batch_ix: Option<usize>,
                total_batches: usize,
                elapsed: Duration,
                err: anyhow::Error,
            },
        }

        self.patch_task = cx.spawn(async move |this, cx| {
            let (progress_tx, mut progress_rx) = mpsc::unbounded::<PatchProgressUpdate>();
            let patch_loader_task = cx.background_executor().spawn({
                let repo_root = repo_root.clone();
                let files = files.clone();
                let collapsed_files = collapsed_files.clone();
                let previous_file_line_stats = previous_file_line_stats.clone();
                let initial_files = initial_files.clone();
                let remaining_files = remaining_files.clone();
                async move {
                    let total_batches = remaining_files.len().div_ceil(DIFF_PROGRESSIVE_BATCH_FILES);
                    if initial_files.is_empty() {
                        let stream = build_diff_stream_from_patch_map(
                            &files,
                            &collapsed_files,
                            &previous_file_line_stats,
                            &BTreeMap::new(),
                            &BTreeSet::new(),
                        );
                        progress_tx
                            .unbounded_send(PatchProgressUpdate::Loaded {
                                batch_ix: None,
                                total_batches,
                                elapsed: Duration::ZERO,
                                stream,
                                pending_files: 0,
                                finished: true,
                            })
                            .ok();
                        return;
                    }

                    let session_started_at = Instant::now();
                    let session = match open_patch_session(&repo_root) {
                        Ok(session) => session,
                        Err(err) => {
                            progress_tx
                                .unbounded_send(PatchProgressUpdate::Error {
                                    batch_ix: None,
                                    total_batches,
                                    elapsed: session_started_at.elapsed(),
                                    err,
                                })
                                .ok();
                            return;
                        }
                    };

                    let mut loaded_patches = BTreeMap::new();
                    let mut loading_paths = remaining_files
                        .iter()
                        .map(|file| file.path.clone())
                        .collect::<BTreeSet<_>>();

                    let initial_stage_started_at = Instant::now();
                    match load_patches_for_files_from_session(&session, &initial_files) {
                        Ok(stage_patches) => {
                            loaded_patches.extend(stage_patches);
                            for file in &initial_files {
                                loading_paths.remove(file.path.as_str());
                            }
                            let stream = build_diff_stream_from_patch_map(
                                &files,
                                &collapsed_files,
                                &previous_file_line_stats,
                                &loaded_patches,
                                &loading_paths,
                            );
                            progress_tx
                                .unbounded_send(PatchProgressUpdate::Loaded {
                                    batch_ix: None,
                                    total_batches,
                                    elapsed: initial_stage_started_at.elapsed(),
                                    stream,
                                    pending_files: loading_paths.len(),
                                    finished: remaining_files.is_empty(),
                                })
                                .ok();
                        }
                        Err(err) => {
                            progress_tx
                                .unbounded_send(PatchProgressUpdate::Error {
                                    batch_ix: None,
                                    total_batches,
                                    elapsed: initial_stage_started_at.elapsed(),
                                    err,
                                })
                                .ok();
                            return;
                        }
                    }

                    for (batch_ix, batch) in remaining_files
                        .chunks(DIFF_PROGRESSIVE_BATCH_FILES)
                        .enumerate()
                    {
                        let stage_started_at = Instant::now();
                        let stage_files = batch.to_vec();
                        match load_patches_for_files_from_session(&session, &stage_files) {
                            Ok(stage_patches) => {
                                loaded_patches.extend(stage_patches);
                                for file in &stage_files {
                                    loading_paths.remove(file.path.as_str());
                                }
                                let stream = build_diff_stream_from_patch_map(
                                    &files,
                                    &collapsed_files,
                                    &previous_file_line_stats,
                                    &loaded_patches,
                                    &loading_paths,
                                );
                                progress_tx
                                    .unbounded_send(PatchProgressUpdate::Loaded {
                                        batch_ix: Some(batch_ix),
                                        total_batches,
                                        elapsed: stage_started_at.elapsed(),
                                        stream,
                                        pending_files: loading_paths.len(),
                                        finished: batch_ix.saturating_add(1) == total_batches,
                                    })
                                    .ok();
                            }
                            Err(err) => {
                                progress_tx
                                    .unbounded_send(PatchProgressUpdate::Error {
                                        batch_ix: Some(batch_ix),
                                        total_batches,
                                        elapsed: stage_started_at.elapsed(),
                                        err,
                                    })
                                    .ok();
                                return;
                            }
                        }
                    }
                }
            });

            while let Some(update) = progress_rx.next().await {
                let Some(this) = this.upgrade() else {
                    break;
                };
                this.update(cx, move |this, cx| {
                    if epoch != this.patch_epoch {
                        return;
                    }

                    match update {
                        PatchProgressUpdate::Loaded {
                            batch_ix,
                            total_batches,
                            elapsed,
                            stream,
                            pending_files,
                            finished,
                        } => {
                            match batch_ix {
                                Some(batch_ix) => {
                                    debug!(
                                        "progressive diff batch {}/{} loaded in {:?} (rows={}, pending_files={})",
                                        batch_ix.saturating_add(1),
                                        total_batches,
                                        elapsed,
                                        stream.rows.len(),
                                        pending_files
                                    );
                                }
                                None => {
                                    debug!(
                                        "initial diff stream loaded in {:?} (rows={}, files={})",
                                        elapsed,
                                        stream.rows.len(),
                                        stream.file_ranges.len()
                                    );
                                }
                            }

                            if finished {
                                this.patch_loading = false;
                            }
                            this.apply_loaded_diff_stream(stream, cx);
                            cx.notify();
                        }
                        PatchProgressUpdate::Error {
                            batch_ix,
                            total_batches,
                            elapsed,
                            err,
                        } => {
                            this.patch_loading = false;
                            match batch_ix {
                                Some(batch_ix) => {
                                    error!(
                                        "progressive diff batch {}/{} failed after {:?}: {err:#}",
                                        batch_ix.saturating_add(1),
                                        total_batches,
                                        elapsed
                                    );
                                }
                                None => {
                                    error!("initial diff stage failed after {:?}: {err:#}", elapsed);
                                }
                            }
                            this.apply_diff_stream_error(err);
                            cx.notify();
                        }
                    }
                });
            }

            patch_loader_task.await;
        });
    }

    fn select_initial_diff_files(
        files: &[ChangedFile],
        selected_path: Option<&str>,
    ) -> Vec<ChangedFile> {
        if files.is_empty() {
            return Vec::new();
        }

        if let Some(selected_path) = selected_path
            && let Some(file) = files.iter().find(|file| file.path == selected_path)
        {
            return vec![file.clone()];
        }

        vec![files[0].clone()]
    }

    fn apply_loaded_diff_stream(&mut self, stream: DiffStream, cx: &mut Context<Self>) {
        self.file_line_stats = self.apply_loaded_diff_surface_stream(stream);
        if !self.patch_loading || !self.line_stats_loading {
            self.recompute_overall_line_stats_from_file_stats();
        }

        if self.workspace_view_mode == WorkspaceViewMode::Files {
            if self.selected_path.is_none() {
                self.selected_path = self.files.first().map(|file| file.path.clone());
            }
        } else {
            let has_selection = self
                .selected_path
                .as_ref()
                .is_some_and(|path| self.files.iter().any(|file| file.path == *path));
            if !has_selection {
                self.selected_path = self.files.first().map(|file| file.path.clone());
            }
        }

        self.selected_status = self
            .selected_path
            .as_deref()
            .and_then(|selected| self.status_for_path(selected));
        self.rebuild_comment_row_match_cache();

        if self.diff_reload_scroll_behavior == DiffReloadScrollBehavior::RevealSelectedFile {
            self.scroll_selected_file_to_top();
            if !self.patch_loading {
                self.diff_reload_scroll_behavior = DiffReloadScrollBehavior::PreserveViewport;
            }
        }
        self.prime_diff_surface_visible_state(cx);
        if !self.patch_loading {
            self.reconcile_comments_with_loaded_diff();
        }
    }

    fn apply_diff_stream_error(&mut self, err: anyhow::Error) {
        self.reset_diff_surface_rows(vec![message_row(
            DiffRowKind::Meta,
            format!("Failed to load diff stream: {err:#}"),
        )]);
        self.diff_reload_scroll_behavior = DiffReloadScrollBehavior::PreserveViewport;
        self.clamp_comment_rows_to_diff();
        self.rebuild_comment_row_match_cache();
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotStageALoadPath {
    WithFingerprintWithoutRefresh,
    IfChangedWithoutRefresh,
    WithFingerprintRefreshWorkingCopy,
    IfChangedRefreshWorkingCopy,
}

enum SnapshotRefreshStageA {
    Unchanged(RepoSnapshotFingerprint),
    Loaded {
        fingerprint: RepoSnapshotFingerprint,
        workflow: Box<WorkflowSnapshot>,
        loaded_without_refresh: bool,
    },
}

fn snapshot_stage_a_load_path(
    behavior: SnapshotRefreshBehavior,
    prefer_stale_first: bool,
) -> SnapshotStageALoadPath {
    match (behavior, prefer_stale_first) {
        (SnapshotRefreshBehavior::ReadOnly, true) => {
            SnapshotStageALoadPath::WithFingerprintWithoutRefresh
        }
        (SnapshotRefreshBehavior::ReadOnly, false) => {
            SnapshotStageALoadPath::IfChangedWithoutRefresh
        }
        (SnapshotRefreshBehavior::RefreshWorkingCopy, true) => {
            SnapshotStageALoadPath::WithFingerprintWithoutRefresh
        }
        (SnapshotRefreshBehavior::RefreshWorkingCopy, false) => {
            SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
        }
    }
}

fn snapshot_stage_a_fallback_load_path(
    prefer_stale_first: bool,
) -> SnapshotStageALoadPath {
    if prefer_stale_first {
        SnapshotStageALoadPath::WithFingerprintRefreshWorkingCopy
    } else {
        SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
    }
}

fn load_snapshot_stage_a_for_path(
    load_path: SnapshotStageALoadPath,
    source_dir: &std::path::Path,
    previous_fingerprint: Option<&RepoSnapshotFingerprint>,
) -> Result<SnapshotRefreshStageA> {
    match load_path {
        SnapshotStageALoadPath::WithFingerprintWithoutRefresh => {
            let (fingerprint, workflow) =
                load_workflow_snapshot_with_fingerprint_without_refresh(source_dir)?;
            Ok(SnapshotRefreshStageA::Loaded {
                fingerprint,
                workflow: Box::new(workflow),
                loaded_without_refresh: true,
            })
        }
        SnapshotStageALoadPath::IfChangedWithoutRefresh => {
            let (fingerprint, workflow) = load_workflow_snapshot_if_changed_without_refresh(
                source_dir,
                previous_fingerprint,
            )?;
            match workflow {
                Some(workflow) => Ok(SnapshotRefreshStageA::Loaded {
                    fingerprint,
                    workflow: Box::new(workflow),
                    loaded_without_refresh: true,
                }),
                None => Ok(SnapshotRefreshStageA::Unchanged(fingerprint)),
            }
        }
        SnapshotStageALoadPath::WithFingerprintRefreshWorkingCopy => {
            let (fingerprint, workflow) = load_workflow_snapshot_with_fingerprint(source_dir)?;
            Ok(SnapshotRefreshStageA::Loaded {
                fingerprint,
                workflow: Box::new(workflow),
                loaded_without_refresh: false,
            })
        }
        SnapshotStageALoadPath::IfChangedRefreshWorkingCopy => {
            let (fingerprint, workflow) =
                load_workflow_snapshot_if_changed(source_dir, previous_fingerprint)?;
            match workflow {
                Some(workflow) => Ok(SnapshotRefreshStageA::Loaded {
                    fingerprint,
                    workflow: Box::new(workflow),
                    loaded_without_refresh: false,
                }),
                None => Ok(SnapshotRefreshStageA::Unchanged(fingerprint)),
            }
        }
    }
}

fn should_send_ai_prompt_from_input_event(event: &InputEvent) -> bool {
    matches!(event, InputEvent::PressEnter { secondary: false })
}

#[cfg(test)]
mod ai_input_tests {
    use super::{
        SnapshotStageALoadPath, SnapshotRefreshBehavior, should_send_ai_prompt_from_input_event,
        snapshot_stage_a_fallback_load_path, snapshot_stage_a_load_path,
    };
    use gpui_component::input::InputEvent;

    #[test]
    fn enter_sends_prompt() {
        assert!(should_send_ai_prompt_from_input_event(&InputEvent::PressEnter {
            secondary: false,
        }));
    }

    #[test]
    fn secondary_enter_does_not_send_prompt() {
        assert!(!should_send_ai_prompt_from_input_event(
            &InputEvent::PressEnter { secondary: true }
        ));
    }

    #[test]
    fn non_enter_events_do_not_send_prompt() {
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Change));
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Focus));
        assert!(!should_send_ai_prompt_from_input_event(&InputEvent::Blur));
    }

    #[test]
    fn refresh_working_copy_uses_full_if_changed_path() {
        assert_eq!(
            snapshot_stage_a_load_path(SnapshotRefreshBehavior::RefreshWorkingCopy, false),
            SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
        );
    }

    #[test]
    fn refresh_working_copy_fallback_keeps_full_refresh_path() {
        assert_eq!(
            snapshot_stage_a_fallback_load_path(false),
            SnapshotStageALoadPath::IfChangedRefreshWorkingCopy
        );
    }
}
