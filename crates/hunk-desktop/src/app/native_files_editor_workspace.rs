use std::path::{Path, PathBuf};

use hunk_editor::{
    WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId, WorkspaceExcerptKind,
    WorkspaceExcerptSpec, WorkspaceLayout, WorkspaceLayoutError,
};
use hunk_text::BufferId;

#[derive(Clone)]
pub(crate) struct WorkspaceEditorSession {
    next_document_id: u64,
    next_excerpt_id: u64,
    layout: WorkspaceLayout,
    active_document_id: Option<WorkspaceDocumentId>,
    active_excerpt_id: Option<WorkspaceExcerptId>,
}

impl WorkspaceEditorSession {
    pub(crate) fn new() -> Self {
        Self {
            next_document_id: 1,
            next_excerpt_id: 1,
            layout: WorkspaceLayout::new(Vec::new(), Vec::new(), 0)
                .expect("empty workspace layout should be valid"),
            active_document_id: None,
            active_excerpt_id: None,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.layout = WorkspaceLayout::new(Vec::new(), Vec::new(), 0)
            .expect("empty workspace layout should be valid");
        self.active_document_id = None;
        self.active_excerpt_id = None;
    }

    pub(crate) fn open_workspace_layout(
        &mut self,
        layout: WorkspaceLayout,
        preferred_path: Option<&Path>,
    ) {
        self.layout = layout;
        self.active_document_id = preferred_path
            .and_then(|path| self.document_id_for_path(path))
            .or_else(|| self.layout.documents().first().map(|document| document.id));
        self.active_excerpt_id = self
            .active_document_id
            .and_then(|document_id| self.first_excerpt_id_for_document(document_id))
            .or_else(|| {
                self.layout
                    .excerpts()
                    .first()
                    .map(|excerpt| excerpt.spec.id)
            });
        self.sync_id_counters_from_layout();
    }

    pub(crate) fn open_full_file_documents(
        &mut self,
        documents: &[(PathBuf, BufferId, usize)],
        preferred_path: Option<&Path>,
    ) -> Result<(), WorkspaceLayoutError> {
        let mut layout_documents = Vec::with_capacity(documents.len());
        let mut excerpt_specs = Vec::with_capacity(documents.len());

        for (path, buffer_id, line_count) in documents {
            let document_id = WorkspaceDocumentId::new(self.next_document_id);
            self.next_document_id = self.next_document_id.saturating_add(1);
            let excerpt_id = WorkspaceExcerptId::new(self.next_excerpt_id);
            self.next_excerpt_id = self.next_excerpt_id.saturating_add(1);
            layout_documents.push(WorkspaceDocument::new(
                document_id,
                path.clone(),
                *buffer_id,
                *line_count,
            ));
            excerpt_specs.push(WorkspaceExcerptSpec::new(
                excerpt_id,
                document_id,
                WorkspaceExcerptKind::FullFile,
                0..*line_count,
            ));
        }

        let layout = WorkspaceLayout::new(layout_documents, excerpt_specs, 0)?;
        self.open_workspace_layout(layout, preferred_path);
        Ok(())
    }

    pub(crate) fn activate_path(&mut self, path: &Path) -> bool {
        let Some(document_id) = self.document_id_for_path(path) else {
            return false;
        };
        self.activate_document(document_id)
    }

    fn sync_id_counters_from_layout(&mut self) {
        self.next_document_id = self
            .layout
            .documents()
            .iter()
            .map(|document| document.id.get())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_excerpt_id = self
            .layout
            .excerpts()
            .iter()
            .map(|excerpt| excerpt.spec.id.get())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
    }

    fn document_id_for_path(&self, path: &Path) -> Option<WorkspaceDocumentId> {
        self.layout
            .documents()
            .iter()
            .find(|document| document.path() == path)
            .map(|document| document.id)
    }

    fn first_excerpt_id_for_document(
        &self,
        document_id: WorkspaceDocumentId,
    ) -> Option<WorkspaceExcerptId> {
        self.layout
            .excerpts()
            .iter()
            .find(|excerpt| excerpt.spec.document_id == document_id)
            .map(|excerpt| excerpt.spec.id)
    }

    fn activate_document(&mut self, document_id: WorkspaceDocumentId) -> bool {
        if self.active_document_id == Some(document_id) {
            return true;
        }
        let Some(excerpt_id) = self.first_excerpt_id_for_document(document_id) else {
            return false;
        };
        self.active_document_id = Some(document_id);
        self.active_excerpt_id = Some(excerpt_id);
        true
    }

    #[cfg(test)]
    pub(crate) fn active_document_id(&self) -> Option<WorkspaceDocumentId> {
        self.active_document_id
    }

    #[cfg(test)]
    pub(crate) fn active_excerpt_id(&self) -> Option<WorkspaceExcerptId> {
        self.active_excerpt_id
    }

    pub(crate) fn active_document(&self) -> Option<&WorkspaceDocument> {
        self.active_document_id
            .and_then(|document_id| self.layout.document(document_id))
    }

    pub(crate) fn active_path(&self) -> Option<&Path> {
        self.active_document().map(|document| document.path())
    }

    pub(crate) fn active_path_buf(&self) -> Option<PathBuf> {
        self.active_path().map(Path::to_path_buf)
    }

    #[cfg(test)]
    pub(crate) fn layout(&self) -> Option<&WorkspaceLayout> {
        self.active_document_id.map(|_| &self.layout)
    }
}
