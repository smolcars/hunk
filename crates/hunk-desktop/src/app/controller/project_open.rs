impl DiffViewer {
    pub(super) fn preferred_project_open_target(&self) -> Option<project_open::ProjectOpenTargetId> {
        project_open::resolve_preferred_project_open_target(
            self.available_project_open_targets.as_slice(),
            self.state.preferred_project_open_target_id.as_deref(),
        )
    }

    pub(super) fn ai_project_open_path(&self) -> Option<std::path::PathBuf> {
        self.ai_workspace_cwd()
            .or_else(|| self.selected_git_workspace_root())
            .or_else(|| self.repo_root.clone())
            .or_else(|| self.project_path.clone())
    }

    pub(super) fn ai_project_open_tooltip(&self) -> String {
        match (
            self.ai_project_open_path(),
            self.preferred_project_open_target(),
        ) {
            (None, _) => "Open a workspace before opening it in an editor.".to_string(),
            (_, None) => "No supported editors or file managers were available.".to_string(),
            (Some(_), Some(target)) => {
                format!("Open this workspace in {}.", target.display_label())
            }
        }
    }

    fn set_preferred_project_open_target(
        &mut self,
        target: project_open::ProjectOpenTargetId,
    ) -> bool {
        let next = Some(target.storage_key().to_string());
        if self.state.preferred_project_open_target_id == next {
            return false;
        }

        self.state.preferred_project_open_target_id = next;
        self.persist_state();
        true
    }

    pub(super) fn open_ai_workspace_in_preferred_project_target(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.preferred_project_open_target() else {
            Self::push_warning_notification(
                "No supported editors or file managers were available.".to_string(),
                None,
                cx,
            );
            return;
        };

        self.open_ai_workspace_in_project_target(target, cx);
    }

    pub(super) fn open_ai_workspace_in_project_target(
        &mut self,
        target: project_open::ProjectOpenTargetId,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.ai_project_open_path() else {
            Self::push_warning_notification(
                "Open a workspace before opening it in an editor.".to_string(),
                None,
                cx,
            );
            return;
        };

        match project_open::open_path_in_project_target(path.as_path(), target) {
            Ok(()) => {
                if self.set_preferred_project_open_target(target) {
                    cx.notify();
                }
            }
            Err(err) => {
                error!(
                    "failed to open workspace '{}' in {}: {err:#}",
                    path.display(),
                    target.display_label(),
                );
                self.available_project_open_targets =
                    project_open::resolve_available_project_open_targets();
                self.invalidate_ai_visible_frame_state_with_reason("project-open");
                Self::push_error_notification(
                    format!("Open in {} failed: {}", target.display_label(), err),
                    cx,
                );
                cx.notify();
            }
        }
    }
}
