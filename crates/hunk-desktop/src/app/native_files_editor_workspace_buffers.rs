use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use hunk_editor::{EditorCommand, EditorState, Viewport};
use hunk_text::{BufferId, TextBuffer};

use super::FilesEditor;

impl FilesEditor {
    pub(crate) fn open_workspace_documents(
        &mut self,
        documents: Vec<(PathBuf, String)>,
        preferred_path: Option<&Path>,
    ) -> Result<()> {
        self.capture_active_view_state();
        self.workspace_buffers.clear();

        if documents.is_empty() {
            self.clear();
            return Ok(());
        }

        let mut workspace_documents = Vec::with_capacity(documents.len());
        for (path, contents) in documents {
            let buffer = TextBuffer::new(BufferId::new(self.next_buffer_id), contents.as_str());
            self.next_buffer_id = self.next_buffer_id.saturating_add(1);
            workspace_documents.push((path.clone(), buffer.id(), buffer.line_count()));
            self.workspace_buffers.insert(path, buffer);
        }

        self.workspace_session
            .open_full_file_documents(&workspace_documents, preferred_path)?;

        let active_path = self
            .active_path_buf()
            .ok_or_else(|| anyhow!("workspace session should pick an active document"))?;
        let active_buffer = self
            .workspace_buffers
            .remove(active_path.as_path())
            .ok_or_else(|| anyhow!("active workspace buffer should exist"))?;
        self.install_active_buffer(active_path.as_path(), active_buffer)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn activate_workspace_path(&mut self, path: &Path) -> Result<bool> {
        if self.active_path() == Some(path) {
            return Ok(true);
        }

        let Some(next_buffer) = self.workspace_buffers.remove(path) else {
            return Ok(false);
        };

        let previous_path = self.active_path_buf();
        self.capture_active_view_state();
        if !self.workspace_session.activate_path(path) {
            self.workspace_buffers
                .insert(path.to_path_buf(), next_buffer);
            return Ok(false);
        }

        let previous_editor = std::mem::replace(&mut self.editor, EditorState::new(next_buffer));
        if let Some(previous_path) = previous_path {
            self.workspace_buffers
                .insert(previous_path, previous_editor.into_buffer());
        }

        self.finish_active_buffer_install(path)?;
        Ok(true)
    }

    pub(super) fn install_active_buffer(&mut self, path: &Path, buffer: TextBuffer) -> Result<()> {
        self.editor = EditorState::new(buffer);
        self.finish_active_buffer_install(path)
    }

    fn finish_active_buffer_install(&mut self, path: &Path) -> Result<()> {
        self.editor.apply(EditorCommand::SetViewport(Viewport {
            first_visible_row: 0,
            visible_row_count: 1,
            horizontal_offset: 0,
        }));
        self.language_label = self
            .registry
            .language_for_path(path)
            .map(|definition| definition.name.clone())
            .unwrap_or_else(|| "text".to_string());
        self.pointer_selection = None;
        self.fold_candidates.clear();
        self.clear_syntax_highlights();
        self.visible_highlight_cache = None;
        self.row_syntax_cache = None;
        self.semantic_highlight_revision = self.semantic_highlight_revision.saturating_add(1);
        self.apply_path_defaults(path);
        self.refresh_syntax_state()?;
        if self.search_query.is_some() {
            self.editor
                .apply(EditorCommand::SetSearchQuery(self.search_query.clone()));
        }
        self.restore_view_state(path);
        Ok(())
    }
}
