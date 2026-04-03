use std::path::{Path, PathBuf};

use hunk_editor::{
    WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId, WorkspaceExcerptKind,
    WorkspaceExcerptSpec, WorkspaceLayout, WorkspaceLayoutError,
};
use hunk_text::BufferId;

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

    pub(crate) fn open_full_file_document(
        &mut self,
        path: &Path,
        buffer_id: BufferId,
        line_count: usize,
    ) -> Result<(), WorkspaceLayoutError> {
        let document_id = WorkspaceDocumentId::new(self.next_document_id);
        self.next_document_id = self.next_document_id.saturating_add(1);
        let excerpt_id = WorkspaceExcerptId::new(self.next_excerpt_id);
        self.next_excerpt_id = self.next_excerpt_id.saturating_add(1);

        let layout = WorkspaceLayout::new(
            vec![WorkspaceDocument::new(
                document_id,
                path.to_path_buf(),
                buffer_id,
                line_count,
            )],
            vec![WorkspaceExcerptSpec::new(
                excerpt_id,
                document_id,
                WorkspaceExcerptKind::FullFile,
                0..line_count,
            )],
            0,
        )?;

        self.layout = layout;
        self.active_document_id = Some(document_id);
        self.active_excerpt_id = Some(excerpt_id);
        Ok(())
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
