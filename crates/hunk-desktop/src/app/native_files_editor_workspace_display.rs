use std::collections::BTreeMap;

use hunk_editor::{Viewport, WorkspaceDisplaySnapshot, WorkspaceDocumentId};
use hunk_text::TextBuffer;

#[allow(clippy::duplicate_mod)]
#[path = "workspace_display_buffers.rs"]
mod workspace_display_buffers;

use workspace_display_buffers::{
    build_workspace_display_snapshot_from_document_snapshots, snapshot_line_text,
};

use super::FilesEditor;

impl FilesEditor {
    #[allow(dead_code)]
    pub(crate) fn build_workspace_display_snapshot(
        &self,
        viewport: Viewport,
        tab_width: usize,
        show_whitespace: bool,
    ) -> Option<WorkspaceDisplaySnapshot> {
        let layout = self.workspace_session.layout()?;
        let document_snapshots = layout
            .documents()
            .iter()
            .filter_map(|document| {
                self.workspace_buffer_for_document(document.id)
                    .map(|buffer| (document.id, buffer.snapshot()))
            })
            .collect::<BTreeMap<_, _>>();
        Some(build_workspace_display_snapshot_from_document_snapshots(
            layout,
            viewport,
            tab_width,
            show_whitespace,
            &document_snapshots,
        ))
    }

    #[allow(dead_code)]
    fn workspace_buffer_line_text(
        &self,
        document_id: WorkspaceDocumentId,
        line: usize,
    ) -> Option<String> {
        let buffer = self.workspace_buffer_for_document(document_id)?;
        Some(snapshot_line_text(&buffer.snapshot(), line))
    }

    #[allow(dead_code)]
    fn workspace_buffer_for_document(
        &self,
        document_id: WorkspaceDocumentId,
    ) -> Option<&TextBuffer> {
        let layout = self.workspace_session.layout()?;
        let document = layout.document(document_id)?;
        if self.active_path() == Some(document.path()) {
            return Some(self.editor.buffer());
        }
        self.workspace_buffers.get(document.path())
    }
}
