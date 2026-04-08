#[path = "../src/app/markdown_links.rs"]
mod markdown_links_impl;

mod app {
    pub(crate) use super::markdown_links_impl as markdown_links;

    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub(crate) struct AiTextSelectionSurfaceSpec {
        pub(crate) surface_id: String,
        pub(crate) row_id: String,
        pub(crate) text: String,
        pub(crate) separator_before: String,
    }

    impl AiTextSelectionSurfaceSpec {
        pub(crate) fn new(surface_id: impl Into<String>, text: impl Into<String>) -> Self {
            let surface_id = surface_id.into();
            Self {
                row_id: surface_id.clone(),
                surface_id,
                text: text.into(),
                separator_before: String::new(),
            }
        }

        pub(crate) fn with_row_id(mut self, row_id: impl Into<String>) -> Self {
            self.row_id = row_id.into();
            self
        }

        pub(crate) fn with_separator_before(mut self, separator_before: impl Into<String>) -> Self {
            self.separator_before = separator_before.into();
            self
        }
    }
}

#[path = "../src/app/ai_workspace_session.rs"]
mod ai_workspace_session;

use std::sync::Arc;

use ai_workspace_session::{
    AI_WORKSPACE_SURFACE_BLOCK_BOTTOM_PADDING_PX, AI_WORKSPACE_SURFACE_BLOCK_GAP_PX,
    AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX, AiWorkspaceBlock, AiWorkspaceBlockActionArea,
    AiWorkspaceBlockKind, AiWorkspaceBlockRole, AiWorkspaceSession, AiWorkspaceSourceRow,
};

fn block(id: &str, kind: AiWorkspaceBlockKind, preview: &str) -> AiWorkspaceBlock {
    AiWorkspaceBlock {
        id: id.to_string(),
        source_row_id: id.to_string(),
        role: match kind {
            AiWorkspaceBlockKind::Message | AiWorkspaceBlockKind::Plan => {
                AiWorkspaceBlockRole::Assistant
            }
            AiWorkspaceBlockKind::Group
            | AiWorkspaceBlockKind::DiffSummary
            | AiWorkspaceBlockKind::Tool => AiWorkspaceBlockRole::Tool,
            AiWorkspaceBlockKind::Status => AiWorkspaceBlockRole::System,
        },
        kind,
        nested: false,
        mono_preview: false,
        open_side_diff_pane: matches!(kind, AiWorkspaceBlockKind::DiffSummary),
        expandable: matches!(
            kind,
            AiWorkspaceBlockKind::Tool | AiWorkspaceBlockKind::Status | AiWorkspaceBlockKind::Group
        ),
        expanded: matches!(
            kind,
            AiWorkspaceBlockKind::Message
                | AiWorkspaceBlockKind::Plan
                | AiWorkspaceBlockKind::DiffSummary
        ),
        title: id.to_string(),
        preview: preview.to_string(),
        preferred_review_path: matches!(kind, AiWorkspaceBlockKind::DiffSummary)
            .then(|| "src/main.rs".to_string()),
        action_area: AiWorkspaceBlockActionArea::Header,
        copy_text: None,
        copy_tooltip: None,
        copy_success_message: None,
        run_in_terminal_command: None,
        run_in_terminal_cwd: None,
        status_label: None,
        status_color_role: None,
        last_sequence: 1,
    }
}

fn source_rows(entries: &[(&str, u64)]) -> Arc<[AiWorkspaceSourceRow]> {
    Arc::<[AiWorkspaceSourceRow]>::from(
        entries
            .iter()
            .map(|(row_id, last_sequence)| AiWorkspaceSourceRow {
                row_id: (*row_id).to_string(),
                last_sequence: *last_sequence,
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn session_matches_source_thread_and_row_ids() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1), ("row-2", 2)]),
        vec![block("row-1", AiWorkspaceBlockKind::Message, "preview")],
    );

    assert_eq!(session.selection_scope_id(), "ai-workspace-thread:thread-1");
    assert_eq!(session.selection_surfaces_for_width(640).len(), 2);
    assert!(session.matches_source("thread-1", &source_rows(&[("row-1", 1), ("row-2", 2)])));
    assert!(!session.matches_source("thread-2", &source_rows(&[("row-1", 1), ("row-2", 2)])));
    assert!(!session.matches_source("thread-1", &source_rows(&[("row-1", 1)])));
    assert!(!session.matches_source("thread-1", &source_rows(&[("row-1", 1), ("row-2", 3)])));
}

#[test]
fn selection_surfaces_follow_rendered_message_text() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1)]),
        vec![block(
            "row-1",
            AiWorkspaceBlockKind::Message,
            "**bold** and `inline`",
        )],
    );

    let selection_surfaces = session.selection_surfaces_for_width(640);

    assert_eq!(selection_surfaces.len(), 2);
    assert_eq!(
        selection_surfaces[1].surface_id,
        "ai-workspace:row-1:preview"
    );
    assert_eq!(selection_surfaces[1].text, "bold and inline");
}

#[test]
fn diff_summary_surfaces_follow_rendered_summary_text() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-diff", 1)]),
        vec![block(
            "row-diff",
            AiWorkspaceBlockKind::DiffSummary,
            "changes\nsrc/main.rs  +4  -2",
        )],
    );

    let selection_surfaces = session.selection_surfaces_for_width(640);

    assert_eq!(selection_surfaces.len(), 2);
    assert_eq!(selection_surfaces[0].text, "row-diff");
    assert_eq!(selection_surfaces[1].text, "changes\nsrc/main.rs  +4  -2");
}

#[test]
fn surface_snapshot_projects_visible_blocks_and_total_height() {
    let blocks = vec![
        block("row-1", AiWorkspaceBlockKind::Message, "First preview"),
        block(
            "row-2",
            AiWorkspaceBlockKind::DiffSummary,
            "changes\nsrc/main.rs  +4  -2",
        ),
    ];
    let expected_total_height = AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX
        + blocks
            .iter()
            .map(|block| {
                ai_workspace_session::ai_workspace_text_layout_for_block(block, 640).height_px
            })
            .sum::<usize>()
        + AI_WORKSPACE_SURFACE_BLOCK_GAP_PX
        + AI_WORKSPACE_SURFACE_BLOCK_BOTTOM_PADDING_PX;
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1), ("row-2", 1)]),
        blocks,
    );

    let snapshot = session.surface_snapshot_with_stats(0, 640, 640).snapshot;

    assert_eq!(snapshot.viewport.visible_blocks.len(), 2);
    assert_eq!(
        snapshot.viewport.total_surface_height_px,
        expected_total_height
    );
}

#[test]
fn text_layout_cache_hits_after_first_snapshot_for_same_width_bucket() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1)]),
        vec![block(
            "row-1",
            AiWorkspaceBlockKind::Message,
            "Hello **world**",
        )],
    );

    let first = session.surface_snapshot_with_stats(0, 400, 640);
    let second = session.surface_snapshot_with_stats(0, 400, 659);

    assert_eq!(first.text_layout_build_count, 1);
    assert_eq!(second.text_layout_build_count, 0);
    assert!(second.text_layout_cache_hits > 0);
}
