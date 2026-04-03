#[allow(dead_code)]
#[path = "../src/app/native_files_editor_workspace.rs"]
mod workspace_editor_session;

use hunk_editor::{
    WorkspaceDocument, WorkspaceDocumentId, WorkspaceExcerptId, WorkspaceExcerptKind,
    WorkspaceExcerptSpec, WorkspaceLayout,
};
use hunk_text::BufferId;
use std::path::Path;
use workspace_editor_session::WorkspaceEditorSession;

fn build_layout() -> WorkspaceLayout {
    WorkspaceLayout::new(
        vec![
            WorkspaceDocument::new(
                WorkspaceDocumentId::new(10),
                "src/main.rs",
                BufferId::new(10),
                12,
            ),
            WorkspaceDocument::new(
                WorkspaceDocumentId::new(20),
                "src/lib.rs",
                BufferId::new(20),
                24,
            ),
        ],
        vec![
            WorkspaceExcerptSpec::new(
                WorkspaceExcerptId::new(100),
                WorkspaceDocumentId::new(10),
                WorkspaceExcerptKind::DiffHunk,
                0..4,
            ),
            WorkspaceExcerptSpec::new(
                WorkspaceExcerptId::new(200),
                WorkspaceDocumentId::new(20),
                WorkspaceExcerptKind::DiffHunk,
                8..16,
            ),
        ],
        0,
    )
    .expect("layout should build")
}

#[test]
fn workspace_editor_session_prefers_requested_path_when_opening_layout() {
    let mut session = WorkspaceEditorSession::new();
    session.open_workspace_layout(build_layout(), Some(Path::new("src/lib.rs")));

    assert_eq!(session.active_path(), Some(Path::new("src/lib.rs")));
    assert_eq!(
        session.active_document_id(),
        Some(WorkspaceDocumentId::new(20))
    );
    assert_eq!(
        session.active_excerpt_id(),
        Some(WorkspaceExcerptId::new(200))
    );
}

#[test]
fn workspace_editor_session_can_switch_active_path_inside_existing_layout() {
    let mut session = WorkspaceEditorSession::new();
    session.open_workspace_layout(build_layout(), Some(Path::new("src/main.rs")));

    assert!(session.activate_path(Path::new("src/lib.rs")));
    assert_eq!(session.active_path(), Some(Path::new("src/lib.rs")));
    assert_eq!(
        session.active_excerpt_id(),
        Some(WorkspaceExcerptId::new(200))
    );
    assert!(!session.activate_path(Path::new("missing.rs")));
}

#[test]
fn workspace_editor_session_opens_multiple_full_file_documents() {
    let mut session = WorkspaceEditorSession::new();
    session
        .open_full_file_documents(
            &[
                (Path::new("src/main.rs").to_path_buf(), BufferId::new(1), 12),
                (Path::new("src/lib.rs").to_path_buf(), BufferId::new(2), 24),
            ],
            Some(Path::new("src/lib.rs")),
        )
        .expect("workspace documents should open");

    let layout = session.layout().expect("workspace layout should exist");
    assert_eq!(layout.documents().len(), 2);
    assert_eq!(layout.excerpts().len(), 2);
    assert_eq!(session.active_path(), Some(Path::new("src/lib.rs")));
}
