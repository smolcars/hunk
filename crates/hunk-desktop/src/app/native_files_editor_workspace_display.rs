use hunk_editor::{
    Viewport, WorkspaceDisplaySnapshot, WorkspaceDocumentId, build_workspace_display_snapshot,
};
use hunk_text::{TextBuffer, TextSnapshot};

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
        Some(build_workspace_display_snapshot(
            layout,
            viewport,
            tab_width,
            show_whitespace,
            |document_id, line| self.workspace_buffer_line_text(document_id, line),
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

fn snapshot_line_text(snapshot: &TextSnapshot, line: usize) -> String {
    let start = snapshot.line_to_byte(line).unwrap_or(0);
    let end = if line + 1 < snapshot.line_count() {
        snapshot
            .line_to_byte(line + 1)
            .unwrap_or(snapshot.byte_len())
    } else {
        snapshot.byte_len()
    };
    snapshot
        .slice(start..end)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string()
}
