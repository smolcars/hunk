use crate::app::markdown_links::{
    MarkdownLinkTarget, MarkdownWorkspaceFileLink, resolve_markdown_link_target,
};

impl DiffViewer {
    pub(super) fn activate_markdown_link(
        &mut self,
        raw_target: String,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        let workspace_root = if self.workspace_view_mode == WorkspaceViewMode::Ai {
            self.ai_workspace_cwd()
                .or_else(|| self.selected_git_workspace_root())
                .or_else(|| self.repo_root.clone())
        } else {
            self.selected_git_workspace_root()
                .or_else(|| self.repo_root.clone())
        };
        let Some(target) =
            resolve_markdown_link_target(raw_target.as_str(), workspace_root.as_deref())
        else {
            return false;
        };

        match target {
            MarkdownLinkTarget::ExternalUrl(url) => match open_url_in_browser(url.as_str()) {
                Ok(()) => true,
                Err(err) => {
                    error!("failed to open markdown URL '{}': {err:#}", url);
                    Self::push_error_notification(
                        format!("Open URL failed: {}", err),
                        cx,
                    );
                    false
                }
            },
            MarkdownLinkTarget::WorkspaceFile(link) => {
                self.open_workspace_markdown_file_link(link, window, cx)
            }
        }
    }

    fn open_workspace_markdown_file_link(
        &mut self,
        link: MarkdownWorkspaceFileLink,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(window) = window {
            self.focus_handle.focus(window, cx);
        }

        if self.workspace_view_mode != WorkspaceViewMode::Files {
            self.set_workspace_view_mode(WorkspaceViewMode::Files, cx);
            if self.workspace_view_mode != WorkspaceViewMode::Files {
                return false;
            }
        }

        let path = link.normalized_path;
        self.selected_path = Some(path.clone());
        self.selected_status = self.status_for_path(path.as_str());

        let editor_already_open = self.editor_path.as_deref() == Some(path.as_str())
            && !self.editor_loading
            && self.editor_error.is_none();
        if !editor_already_open {
            self.request_file_editor_reload(path.clone(), cx);
        }

        if let Some(_line) = link.line {
            // Preserve parsed anchors for future line-jump support.
        }

        cx.notify();
        true
    }
}
