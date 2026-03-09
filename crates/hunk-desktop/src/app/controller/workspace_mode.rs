impl DiffViewer {
    fn branch_workspace_target(&self, branch_name: &str) -> Option<(String, String)> {
        self.git_workspace
            .branches
            .iter()
            .find(|branch| branch.name == branch_name)
            .and_then(|branch| {
                Some((
                    branch.attached_workspace_target_id.clone()?,
                    branch
                        .attached_workspace_target_label
                        .clone()
                        .unwrap_or_else(|| branch.name.clone()),
                ))
            })
    }

    fn activate_workspace_target_for_branch(
        &mut self,
        branch_name: &str,
        workspace_target_id: String,
        workspace_target_label: String,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .workspace_targets
            .iter()
            .any(|target| target.id == workspace_target_id)
        {
            self.refresh_workspace_targets_from_git_state(cx);
        }

        if !self
            .workspace_targets
            .iter()
            .any(|target| target.id == workspace_target_id)
        {
            self.set_git_warning_message(
                format!(
                    "Branch {branch_name} is already checked out in {workspace_target_label}, but that workspace target is unavailable."
                ),
                window,
                cx,
            );
            self.sync_branch_picker_state(cx);
            return false;
        }

        self.activate_workspace_target(workspace_target_id, cx);
        true
    }

    pub(super) fn request_activate_or_create_branch_with_dirty_guard(
        &mut self,
        branch_name: String,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        let target_branch = branch_name.trim().to_string();
        let source_branch = self
            .checked_out_branch_name()
            .unwrap_or(self.git_workspace.branch_name.as_str())
            .to_string();

        if let Some(message) = branch_activation::branch_activation_preflight_message(
            target_branch.as_str(),
            source_branch.as_str(),
            self.git_controls_busy(),
        ) {
            self.set_git_warning_message(message, window, cx);
            self.sync_branch_picker_state(cx);
            return false;
        }

        if let Some((workspace_target_id, workspace_target_label)) =
            self.branch_workspace_target(target_branch.as_str())
            && self.active_workspace_target_id.as_deref() != Some(workspace_target_id.as_str())
        {
            return self.activate_workspace_target_for_branch(
                target_branch.as_str(),
                workspace_target_id,
                workspace_target_label,
                window,
                cx,
            );
        }

        if let Some(message) = branch_activation::branch_activation_block_message(
            target_branch.as_str(),
            source_branch.as_str(),
            self.git_controls_busy(),
            self.git_workspace.files.len(),
        ) {
            self.set_git_warning_message(message, window, cx);
            self.sync_branch_picker_state(cx);
            return false;
        }

        self.activate_or_create_branch(target_branch, cx)
    }

    pub(super) fn active_review_action_blocker(&self) -> Option<String> {
        if self.git_action_loading {
            return Some("Another workspace action is in progress.".to_string());
        }
        if !self.can_run_active_branch_actions() {
            return Some("Activate a branch before opening PR/MR.".to_string());
        }
        if !self.git_workspace.branch_has_upstream {
            return Some("Publish this branch before opening PR/MR.".to_string());
        }
        None
    }
}
