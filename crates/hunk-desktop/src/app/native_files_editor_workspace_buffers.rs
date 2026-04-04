use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use hunk_editor::{EditorCommand, EditorState, Viewport, WorkspaceExcerptId, WorkspaceLayout};
use hunk_text::{BufferId, TextBuffer};

use super::FilesEditor;

impl FilesEditor {
    #[allow(dead_code)]
    pub(crate) fn open_workspace_layout_documents(
        &mut self,
        layout: WorkspaceLayout,
        documents: Vec<(PathBuf, String)>,
        preferred_path: Option<&Path>,
    ) -> Result<()> {
        self.capture_active_view_state();

        if documents.is_empty() {
            self.clear();
            return Ok(());
        }

        let workspace_buffers = self.build_workspace_layout_buffers(&layout, documents)?;
        self.workspace_session
            .open_workspace_layout(layout, preferred_path);
        self.install_workspace_buffers_for_open_session(workspace_buffers)
    }

    pub(crate) fn open_workspace_documents(
        &mut self,
        documents: Vec<(PathBuf, String)>,
        preferred_path: Option<&Path>,
    ) -> Result<()> {
        self.capture_active_view_state();

        if documents.is_empty() {
            self.clear();
            return Ok(());
        }

        let workspace_buffers = self.build_workspace_buffers(documents);
        let workspace_documents = workspace_buffers
            .iter()
            .map(|(path, buffer)| (path.clone(), buffer.id(), buffer.line_count()))
            .collect::<Vec<_>>();

        self.workspace_session
            .open_full_file_documents(&workspace_documents, preferred_path)?;
        self.install_workspace_buffers_for_open_session(workspace_buffers)
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

    pub(crate) fn activate_workspace_excerpt(
        &mut self,
        excerpt_id: WorkspaceExcerptId,
    ) -> Result<bool> {
        let Some(layout) = self.workspace_session.layout() else {
            return Ok(false);
        };
        let Some(excerpt) = layout.excerpt(excerpt_id) else {
            return Ok(false);
        };
        let Some(document) = layout.document(excerpt.spec.document_id) else {
            return Ok(false);
        };
        let path = document.path().to_path_buf();

        if self.active_path() == Some(path.as_path()) {
            return Ok(self.workspace_session.activate_excerpt(excerpt_id));
        }

        let Some(next_buffer) = self.workspace_buffers.remove(path.as_path()) else {
            return Ok(false);
        };

        let previous_path = self.active_path_buf();
        self.capture_active_view_state();
        if !self.workspace_session.activate_excerpt(excerpt_id) {
            self.workspace_buffers.insert(path, next_buffer);
            return Ok(false);
        }

        let previous_editor = std::mem::replace(&mut self.editor, EditorState::new(next_buffer));
        if let Some(previous_path) = previous_path {
            self.workspace_buffers
                .insert(previous_path, previous_editor.into_buffer());
        }

        self.finish_active_buffer_install(path.as_path())?;
        Ok(true)
    }

    pub(super) fn install_active_buffer(&mut self, path: &Path, buffer: TextBuffer) -> Result<()> {
        self.editor = EditorState::new(buffer);
        self.finish_active_buffer_install(path)
    }

    fn build_workspace_buffers(
        &mut self,
        documents: Vec<(PathBuf, String)>,
    ) -> BTreeMap<PathBuf, TextBuffer> {
        let mut workspace_buffers = BTreeMap::new();
        for (path, contents) in documents {
            let buffer = TextBuffer::new(BufferId::new(self.next_buffer_id), contents.as_str());
            self.next_buffer_id = self.next_buffer_id.saturating_add(1);
            workspace_buffers.insert(path, buffer);
        }
        workspace_buffers
    }

    fn build_workspace_layout_buffers(
        &mut self,
        layout: &WorkspaceLayout,
        documents: Vec<(PathBuf, String)>,
    ) -> Result<BTreeMap<PathBuf, TextBuffer>> {
        let buffer_id_by_path = layout
            .documents()
            .iter()
            .map(|document| (document.path().to_path_buf(), document.buffer_id))
            .collect::<BTreeMap<_, _>>();

        let mut workspace_buffers = BTreeMap::new();
        for (path, contents) in documents {
            let buffer_id = buffer_id_by_path.get(&path).copied().ok_or_else(|| {
                anyhow!("missing workspace layout document for {}", path.display())
            })?;
            self.next_buffer_id = self.next_buffer_id.max(buffer_id.get().saturating_add(1));
            workspace_buffers.insert(path, TextBuffer::new(buffer_id, contents.as_str()));
        }

        self.validate_workspace_layout_buffers(layout, &workspace_buffers)?;
        Ok(workspace_buffers)
    }

    #[allow(dead_code)]
    fn validate_workspace_layout_buffers(
        &self,
        layout: &WorkspaceLayout,
        workspace_buffers: &BTreeMap<PathBuf, TextBuffer>,
    ) -> Result<()> {
        for document in layout.documents() {
            let buffer = workspace_buffers.get(document.path()).ok_or_else(|| {
                anyhow!("missing workspace buffer for {}", document.path.display())
            })?;
            if buffer.id() != document.buffer_id {
                return Err(anyhow!(
                    "workspace buffer id mismatch for {}: layout={} buffer={}",
                    document.path.display(),
                    document.buffer_id.get(),
                    buffer.id().get(),
                ));
            }
            if buffer.line_count() != document.line_count {
                return Err(anyhow!(
                    "workspace buffer line count mismatch for {}: layout={} buffer={}",
                    document.path.display(),
                    document.line_count,
                    buffer.line_count(),
                ));
            }
        }
        Ok(())
    }

    fn install_workspace_buffers_for_open_session(
        &mut self,
        workspace_buffers: BTreeMap<PathBuf, TextBuffer>,
    ) -> Result<()> {
        self.workspace_buffers = workspace_buffers;
        self.workspace_syntax_by_path.clear();
        let active_path = self
            .active_path_buf()
            .ok_or_else(|| anyhow!("workspace session should pick an active document"))?;
        let active_buffer = self
            .workspace_buffers
            .remove(active_path.as_path())
            .ok_or_else(|| anyhow!("active workspace buffer should exist"))?;
        self.install_active_buffer(active_path.as_path(), active_buffer)
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
