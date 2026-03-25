impl DiffViewer {
    fn local_branches_from_cached_branches(
        cached_branches: &[CachedLocalBranchState],
    ) -> Vec<LocalBranch> {
        cached_branches
            .iter()
            .map(|branch| LocalBranch {
                name: branch.name.clone(),
                is_current: branch.is_current,
                tip_unix_time: branch.tip_unix_time,
                attached_workspace_target_id: branch.attached_workspace_target_id.clone(),
                attached_workspace_target_root: branch.attached_workspace_target_root.clone(),
                attached_workspace_target_label: branch.attached_workspace_target_label.clone(),
            })
            .collect()
    }

    fn ai_worktree_base_branch_picker_branches(&self) -> Vec<LocalBranch> {
        let Some(draft_root) = self.ai_draft_workspace_root() else {
            return self.branches.clone();
        };
        let draft_project_root = hunk_git::worktree::primary_repo_root(draft_root.as_path())
            .unwrap_or_else(|_| draft_root.clone());
        let draft_project_key = draft_project_root.to_string_lossy().to_string();
        if self.project_path.as_ref() == Some(&draft_project_root) {
            return self.branches.clone();
        }

        if let Some(project_state) = self.workspace_project_states.get(draft_project_key.as_str()) {
            return project_state.branches.clone();
        }

        self.state
            .git_workflow_cache_by_repo
            .get(draft_project_key.as_str())
            .map(|cache| Self::local_branches_from_cached_branches(&cache.branches))
            .unwrap_or_default()
    }

    fn update_branch_picker_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let delegate = build_branch_picker_delegate(&self.git_workspace.branches);
        let selected_index =
            branch_picker_selected_index(&self.git_workspace.branches, self.checked_out_branch_name());
        Self::set_index_picker_state(
            &self.branch_picker_state,
            delegate,
            selected_index,
            window,
            cx,
        );
        cx.notify();
    }

    fn update_ai_worktree_base_branch_picker_state(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let branches = self.ai_worktree_base_branch_picker_branches();
        let delegate = build_branch_picker_delegate(&branches);
        let selected_index = branch_picker_selected_index(
            &branches,
            self.ai_selected_worktree_base_branch_name(),
        );
        Self::set_index_picker_state(
            &self.ai_worktree_base_branch_picker_state,
            delegate,
            selected_index,
            window,
            cx,
        );
        cx.notify();
    }

    fn sync_branch_picker_state(&mut self, cx: &mut Context<Self>) {
        let branch_picker_state = self.branch_picker_state.clone();
        let delegate = build_branch_picker_delegate(&self.git_workspace.branches);
        let selected_index =
            branch_picker_selected_index(&self.git_workspace.branches, self.checked_out_branch_name());

        Self::sync_index_picker_state(
            branch_picker_state,
            delegate,
            selected_index,
            "failed to sync branch picker state",
            cx,
        );
    }

    fn sync_ai_worktree_base_branch_picker_state(&mut self, cx: &mut Context<Self>) {
        let ai_worktree_base_branch_picker_state = self.ai_worktree_base_branch_picker_state.clone();
        let branches = self.ai_worktree_base_branch_picker_branches();
        let delegate = build_branch_picker_delegate(&branches);
        let selected_index = branch_picker_selected_index(
            &branches,
            self.ai_selected_worktree_base_branch_name(),
        );

        Self::sync_index_picker_state(
            ai_worktree_base_branch_picker_state,
            delegate,
            selected_index,
            "failed to sync AI worktree base branch picker state",
            cx,
        );
    }

    fn update_workspace_target_picker_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let delegate = build_workspace_target_picker_delegate(&self.workspace_targets);
        let selected_index = workspace_target_picker_selected_index(
            &self.workspace_targets,
            self.active_workspace_target_id.as_deref(),
        );
        Self::set_index_picker_state(
            &self.workspace_target_picker_state,
            delegate,
            selected_index,
            window,
            cx,
        );
        cx.notify();
    }

    fn sync_workspace_target_picker_state(&mut self, cx: &mut Context<Self>) {
        let workspace_target_picker_state = self.workspace_target_picker_state.clone();
        let delegate = build_workspace_target_picker_delegate(&self.workspace_targets);
        let selected_index = workspace_target_picker_selected_index(
            &self.workspace_targets,
            self.active_workspace_target_id.as_deref(),
        );

        Self::sync_index_picker_state(
            workspace_target_picker_state,
            delegate,
            selected_index,
            "failed to sync workspace target picker state",
            cx,
        );
    }

    fn set_index_picker_state<D>(
        picker_state: &Entity<HunkPickerState<D>>,
        delegate: D,
        selected_index: Option<usize>,
        window: &mut Window,
        cx: &mut App,
    ) where
        D: crate::app::hunk_picker::HunkPickerDelegate,
    {
        picker_state.update(cx, |state, cx| {
            state.set_items(delegate, window, cx);
            state.set_selected_index(selected_index, window, cx);
        });
    }

    fn sync_index_picker_state<D>(
        picker_state: Entity<HunkPickerState<D>>,
        delegate: D,
        selected_index: Option<usize>,
        error_context: &'static str,
        cx: &mut Context<Self>,
    ) where
        D: crate::app::hunk_picker::HunkPickerDelegate,
    {
        if let Err(err) = Self::update_any_window(cx, move |window, cx| {
            Self::set_index_picker_state(
                &picker_state,
                delegate.clone(),
                selected_index,
                window,
                cx,
            );
        }) {
            error!("{error_context}: {err:#}");
        }
    }

    fn apply_picker_action<D>(
        picker_state: &Entity<HunkPickerState<D>>,
        action: HunkPickerAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool
    where
        D: crate::app::hunk_picker::HunkPickerDelegate,
    {
        if !picker_state.read(cx).is_open() {
            return false;
        }

        picker_state.update(cx, |state, cx| state.apply_action(action, window, cx))
    }

    pub(super) fn handle_hunk_picker_keystroke(
        &mut self,
        action: HunkPickerAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        Self::apply_picker_action(&self.project_picker_state, action, window, cx)
            || Self::apply_picker_action(&self.workspace_target_picker_state, action, window, cx)
            || Self::apply_picker_action(&self.branch_picker_state, action, window, cx)
            || Self::apply_picker_action(
                &self.ai_worktree_base_branch_picker_state,
                action,
                window,
                cx,
            )
            || Self::apply_picker_action(&self.review_left_picker_state, action, window, cx)
            || Self::apply_picker_action(&self.review_right_picker_state, action, window, cx)
    }

    fn workspace_catalog_source_root(&self) -> Option<PathBuf> {
        self.project_path.clone().or_else(|| self.repo_root.clone())
    }

    pub(crate) fn primary_repo_root(&self) -> Option<PathBuf> {
        self.repo_root.clone().or_else(|| self.project_path.clone())
    }

    fn primary_workspace_target_id_from_targets(
        targets: &[WorkspaceTargetSummary],
        repo_root: Option<&PathBuf>,
    ) -> Option<String> {
        targets
            .iter()
            .find(|target| target.kind == hunk_git::worktree::WorkspaceTargetKind::PrimaryCheckout)
            .or_else(|| {
                repo_root.and_then(|repo_root| {
                    targets
                        .iter()
                        .find(|target| target.root == *repo_root)
                })
            })
            .or_else(|| targets.first())
            .map(|target| target.id.clone())
    }

    pub(crate) fn primary_workspace_target(&self) -> Option<&WorkspaceTargetSummary> {
        let primary_target_id = Self::primary_workspace_target_id_from_targets(
            &self.workspace_targets,
            self.repo_root.as_ref(),
        )?;
        self.workspace_targets
            .iter()
            .find(|target| target.id == primary_target_id)
    }

    pub(crate) fn primary_workspace_target_id(&self) -> Option<String> {
        self.primary_workspace_target().map(|target| target.id.clone())
    }

    pub(crate) fn selected_git_workspace_target(&self) -> Option<&WorkspaceTargetSummary> {
        self.active_workspace_target_id
            .as_deref()
            .and_then(|target_id| {
                self.workspace_targets
                    .iter()
                    .find(|target| target.id == target_id)
            })
            .or_else(|| self.primary_workspace_target())
    }

    pub(crate) fn selected_git_workspace_root(&self) -> Option<PathBuf> {
        self.selected_git_workspace_target()
            .map(|target| target.root.clone())
    }

    fn persisted_workspace_target_id(&self) -> Option<String> {
        let project_path = self.project_path.as_ref()?;
        self.state
            .last_workspace_target_by_repo
            .get(project_path.to_string_lossy().as_ref())
            .cloned()
    }

    fn persist_active_workspace_target_id(&mut self) {
        let Some(project_path) = self.project_path.as_ref() else {
            return;
        };

        let key = project_path.to_string_lossy().to_string();
        match self.active_workspace_target_id.clone() {
            Some(active_target_id) => {
                if self
                    .state
                    .last_workspace_target_by_repo
                    .get(key.as_str())
                    == Some(&active_target_id)
                {
                    return;
                }
                self.state
                    .last_workspace_target_by_repo
                    .insert(key, active_target_id);
            }
            None => {
                if self
                    .state
                    .last_workspace_target_by_repo
                    .remove(key.as_str())
                    .is_none()
                {
                    return;
                }
            }
        }
        self.persist_state();
    }

    fn refresh_workspace_targets_from_git_state(&mut self, cx: &mut Context<Self>) {
        let Some(source_root) = self.workspace_catalog_source_root() else {
            self.workspace_targets.clear();
            self.active_workspace_target_id = None;
            self.sync_workspace_target_picker_state(cx);
            self.refresh_review_compare_sources_from_git_state(cx);
            return;
        };

        match hunk_git::worktree::list_workspace_targets(source_root.as_path()) {
            Ok(targets) => {
                let primary_target_id =
                    Self::primary_workspace_target_id_from_targets(&targets, self.repo_root.as_ref());
                let next_active_target_id = self
                    .active_workspace_target_id
                    .clone()
                    .filter(|active_target_id| {
                        targets
                            .iter()
                            .any(|target| target.id == *active_target_id)
                    })
                    .or_else(|| {
                        self.persisted_workspace_target_id().filter(|persisted_target_id| {
                            targets
                                .iter()
                                .any(|target| target.id == *persisted_target_id)
                        })
                    })
                    .or(primary_target_id);
                self.workspace_targets = targets;
                self.active_workspace_target_id = next_active_target_id;
                self.persist_active_workspace_target_id();
                self.sync_workspace_target_picker_state(cx);
                self.sync_ai_workspace_target_from_catalog(cx);
                self.refresh_review_compare_sources_from_git_state(cx);
                if self.workspace_view_mode == WorkspaceViewMode::Ai {
                    self.refresh_ai_repo_thread_catalog(cx);
                }
            }
            Err(err) => {
                debug!(
                    "skipping workspace target refresh for {}: {err:#}",
                    source_root.display()
                );
                self.workspace_targets.clear();
                self.active_workspace_target_id = None;
                self.sync_workspace_target_picker_state(cx);
                self.sync_ai_workspace_target_from_catalog(cx);
                self.refresh_review_compare_sources_from_git_state(cx);
                if self.workspace_view_mode == WorkspaceViewMode::Ai {
                    self.refresh_ai_repo_thread_catalog(cx);
                }
            }
        }
    }

    fn restore_active_workspace_target_root_from_state(&mut self, cx: &mut Context<Self>) {
        let Some(project_path) = self.project_path.clone() else {
            return;
        };

        let Ok(targets) = hunk_git::worktree::list_workspace_targets(project_path.as_path()) else {
            self.workspace_targets.clear();
            self.active_workspace_target_id = None;
            self.sync_workspace_target_picker_state(cx);
            self.refresh_review_compare_sources_from_git_state(cx);
            return;
        };

        let target_id = self
            .persisted_workspace_target_id()
            .filter(|persisted_target_id| {
                targets
                    .iter()
                    .any(|target| target.id == *persisted_target_id)
            })
            .or_else(|| Self::primary_workspace_target_id_from_targets(&targets, self.repo_root.as_ref()));
        let mut targets = targets;
        for workspace_target in &mut targets {
            workspace_target.is_active =
                target_id.as_deref() == Some(workspace_target.id.as_str());
        }

        self.workspace_targets = targets;
        self.active_workspace_target_id = target_id;
        self.persist_active_workspace_target_id();
        self.sync_workspace_target_picker_state(cx);
        self.sync_ai_workspace_target_from_catalog(cx);
        self.refresh_review_compare_sources_from_git_state(cx);
        if self.workspace_view_mode == WorkspaceViewMode::Ai {
            self.refresh_ai_repo_thread_catalog(cx);
        }
        let selected_target_is_primary = self
            .workspace_targets
            .iter()
            .find(|target| target.is_active)
            .is_some_and(|target| {
                matches!(
                    target.kind,
                    hunk_git::worktree::WorkspaceTargetKind::PrimaryCheckout
                )
            });
        if should_request_startup_git_workspace_refresh(selected_target_is_primary) {
            self.request_git_workspace_refresh(true, cx);
        }
    }

    fn activate_workspace_target(&mut self, target_id: String, cx: &mut Context<Self>) {
        let Some(target) = self
            .workspace_targets
            .iter()
            .find(|target| target.id == target_id)
            .cloned()
        else {
            return;
        };
        if self.active_workspace_target_id.as_deref() == Some(target.id.as_str()) {
            return;
        }

        self.workspace_target_switch_loading = true;
        self.active_workspace_target_id = Some(target.id);
        for workspace_target in &mut self.workspace_targets {
            workspace_target.is_active =
                self.active_workspace_target_id.as_deref() == Some(workspace_target.id.as_str());
        }
        self.persist_active_workspace_target_id();
        self.sync_workspace_target_picker_state(cx);
        self.refresh_review_compare_sources_from_git_state(cx);
        self.git_status_message = Some("Switching Git workspace target...".to_string());
        self.start_repo_watch(cx);
        self.request_git_workspace_refresh(true, cx);
        cx.notify();
    }
}
