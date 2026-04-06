#[path = "../src/app/markdown_links.rs"]
mod markdown_links_impl;

mod app {
    pub(crate) use super::markdown_links_impl as markdown_links;
}

#[path = "../src/app/ai_workspace_session.rs"]
mod ai_workspace_session;

use std::sync::Arc;

use ai_workspace_session::{
    AI_WORKSPACE_SURFACE_BLOCK_BOTTOM_PADDING_PX, AI_WORKSPACE_SURFACE_BLOCK_GAP_PX,
    AI_WORKSPACE_SURFACE_BLOCK_SIDE_PADDING_PX, AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX,
    AiWorkspaceBlock, AiWorkspaceBlockKind, AiWorkspaceBlockRole, AiWorkspaceSelection,
    AiWorkspaceSelectionRegion, AiWorkspaceSession, AiWorkspaceSourceRow,
    ai_workspace_text_layout_for_block,
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
        expandable: matches!(
            kind,
            AiWorkspaceBlockKind::Tool | AiWorkspaceBlockKind::Status
        ),
        expanded: matches!(
            kind,
            AiWorkspaceBlockKind::Message | AiWorkspaceBlockKind::Plan
        ),
        title: id.to_string(),
        preview: preview.to_string(),
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
    let session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1), ("row-2", 2)]),
        vec![block("row-1", AiWorkspaceBlockKind::Message, "preview")],
    );

    assert!(session.matches_source("thread-1", &source_rows(&[("row-1", 1), ("row-2", 2)])));
    assert!(!session.matches_source("thread-2", &source_rows(&[("row-1", 1), ("row-2", 2)])));
    assert!(!session.matches_source("thread-1", &source_rows(&[("row-1", 1)])));
    assert!(!session.matches_source("thread-1", &source_rows(&[("row-1", 1), ("row-2", 3)])));
}

#[test]
fn surface_snapshot_projects_visible_blocks_and_total_height() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1), ("row-2", 2), ("row-3", 3)]),
        vec![
            block("row-1", AiWorkspaceBlockKind::Message, "first preview"),
            block("row-2", AiWorkspaceBlockKind::DiffSummary, "diff preview"),
            block("row-3", AiWorkspaceBlockKind::Status, ""),
        ],
    );

    let snapshot = session.surface_snapshot(0, 220, 640);

    assert_eq!(snapshot.viewport.visible_blocks.len(), 3);
    let first = snapshot
        .viewport
        .visible_blocks
        .first()
        .expect("first visible block");
    let second = snapshot
        .viewport
        .visible_blocks
        .get(1)
        .expect("second visible block");
    let third = snapshot
        .viewport
        .visible_blocks
        .get(2)
        .expect("third visible block");
    assert_eq!(first.top_px, AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX);
    assert_eq!(
        second.top_px,
        first.top_px + first.height_px + AI_WORKSPACE_SURFACE_BLOCK_GAP_PX
    );
    assert_eq!(
        third.top_px,
        second.top_px + second.height_px + AI_WORKSPACE_SURFACE_BLOCK_GAP_PX
    );
    assert_eq!(
        snapshot.viewport.total_surface_height_px,
        third.top_px + third.height_px + AI_WORKSPACE_SURFACE_BLOCK_BOTTOM_PADDING_PX
    );
}

#[test]
fn surface_snapshot_limits_visible_blocks_to_requested_range() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1), ("row-2", 2), ("row-3", 3)]),
        vec![
            block("row-1", AiWorkspaceBlockKind::Message, "first preview"),
            block("row-2", AiWorkspaceBlockKind::Message, "second preview"),
            block("row-3", AiWorkspaceBlockKind::Message, "third preview"),
        ],
    );

    let snapshot = session.surface_snapshot(96, 90, 640);
    let visible_ids = snapshot
        .viewport
        .visible_blocks
        .iter()
        .map(|entry| entry.block.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(visible_ids, vec!["row-2", "row-3"]);
}

#[test]
fn surface_snapshot_supports_all_block_kinds_and_roles() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[
            ("row-user", 1),
            ("row-group", 2),
            ("row-plan", 3),
            ("row-tool", 4),
            ("row-status", 5),
        ]),
        vec![
            AiWorkspaceBlock {
                id: "row-user".to_string(),
                source_row_id: "row-user".to_string(),
                role: AiWorkspaceBlockRole::User,
                kind: AiWorkspaceBlockKind::Message,
                expandable: false,
                expanded: true,
                title: "You".to_string(),
                preview: "prompt".to_string(),
                last_sequence: 1,
            },
            block("row-group", AiWorkspaceBlockKind::Group, "group"),
            block("row-plan", AiWorkspaceBlockKind::Plan, "plan"),
            block("row-tool", AiWorkspaceBlockKind::Tool, "tool"),
            block("row-status", AiWorkspaceBlockKind::Status, "status"),
        ],
    );

    let snapshot = session.surface_snapshot(0, 640, 800);
    let ids = snapshot
        .viewport
        .visible_blocks
        .iter()
        .map(|entry| entry.block.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![
            "row-user",
            "row-group",
            "row-plan",
            "row-tool",
            "row-status"
        ]
    );
}

#[test]
fn selection_matches_block_and_helpers_remain_addressable() {
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        source_rows(&[("row-1", 1), ("row-2", 2)]),
        vec![
            block("row-1", AiWorkspaceBlockKind::Message, "first preview"),
            block("row-2", AiWorkspaceBlockKind::DiffSummary, "diff preview"),
        ],
    );
    let selection = AiWorkspaceSelection {
        block_id: "row-2".to_string(),
        block_kind: AiWorkspaceBlockKind::DiffSummary,
        line_index: Some(1),
        region: AiWorkspaceSelectionRegion::Preview,
    };

    assert!(selection.matches_block("row-2"));
    assert!(!selection.matches_block("row-1"));
    assert_eq!(
        AiWorkspaceSelectionRegion::Block,
        AiWorkspaceSelectionRegion::Block
    );
    assert_eq!(
        AiWorkspaceSelectionRegion::Title,
        AiWorkspaceSelectionRegion::Title
    );
    assert_eq!(session.block_count(), 2);
    assert_eq!(session.block_index("row-2"), Some(1));
    assert_eq!(
        session.block_at(0).map(|block| block.id.as_str()),
        Some("row-1")
    );
    assert_eq!(
        session.block("row-2").map(|block| block.preview.as_str()),
        Some("diff preview")
    );
    let geometry = session
        .block_geometry("row-2", 640)
        .expect("row-2 geometry should exist");
    assert!(geometry.top_px >= AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX);
    assert!(geometry.bottom_px() >= geometry.top_px);
    assert_eq!(AI_WORKSPACE_SURFACE_BLOCK_SIDE_PADDING_PX, 16);
}

#[test]
fn narrower_widths_increase_wrapped_height() {
    let block = AiWorkspaceBlock {
        id: "row-1".to_string(),
        source_row_id: "row-1".to_string(),
        role: AiWorkspaceBlockRole::Assistant,
        kind: AiWorkspaceBlockKind::Message,
        expandable: false,
        expanded: true,
        title: "Assistant".to_string(),
        preview: "This is a longer assistant message preview that should wrap across multiple lines when the surface width gets narrower.".to_string(),
        last_sequence: 1,
    };

    let wide = ai_workspace_text_layout_for_block(&block, 800);
    let narrow = ai_workspace_text_layout_for_block(&block, 320);

    assert!(wide.preview_lines.len() <= narrow.preview_lines.len());
    assert!(wide.height_px <= narrow.height_px);
}

#[test]
fn message_blocks_are_no_longer_limited_to_six_preview_lines() {
    let preview = (1..=12)
        .map(|line| format!("line {line}  "))
        .collect::<Vec<_>>()
        .join("\n");
    let block = AiWorkspaceBlock {
        id: "row-message".to_string(),
        source_row_id: "row-message".to_string(),
        role: AiWorkspaceBlockRole::Assistant,
        kind: AiWorkspaceBlockKind::Message,
        expandable: false,
        expanded: true,
        title: "Assistant".to_string(),
        preview,
        last_sequence: 1,
    };

    let layout = ai_workspace_text_layout_for_block(&block, 640);

    assert!(layout.preview_lines.len() > 6);
}

#[test]
fn expanded_tool_blocks_take_more_height_than_collapsed_tool_blocks() {
    let preview = (1..=10)
        .map(|line| format!("tool output line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let collapsed = AiWorkspaceBlock {
        id: "row-tool".to_string(),
        source_row_id: "row-tool".to_string(),
        role: AiWorkspaceBlockRole::Tool,
        kind: AiWorkspaceBlockKind::Tool,
        expandable: true,
        expanded: false,
        title: "Command".to_string(),
        preview: preview.clone(),
        last_sequence: 1,
    };
    let expanded = AiWorkspaceBlock {
        expanded: true,
        ..collapsed.clone()
    };

    let collapsed_layout = ai_workspace_text_layout_for_block(&collapsed, 640);
    let expanded_layout = ai_workspace_text_layout_for_block(&expanded, 640);

    assert!(collapsed_layout.preview_lines.len() < expanded_layout.preview_lines.len());
    assert!(collapsed_layout.height_px < expanded_layout.height_px);
}

#[test]
fn very_narrow_surface_widths_do_not_panic() {
    let block = AiWorkspaceBlock {
        id: "row-narrow".to_string(),
        source_row_id: "row-narrow".to_string(),
        role: AiWorkspaceBlockRole::Assistant,
        kind: AiWorkspaceBlockKind::Message,
        expandable: false,
        expanded: true,
        title: "Assistant".to_string(),
        preview: "narrow viewport".to_string(),
        last_sequence: 1,
    };
    let mut session =
        AiWorkspaceSession::new("thread-1", source_rows(&[("row-narrow", 1)]), vec![block]);

    let snapshot = session.surface_snapshot(0, 200, 1);

    assert_eq!(snapshot.viewport.visible_blocks.len(), 1);
    assert!(snapshot.viewport.total_surface_height_px > 0);
}

#[test]
fn markdown_message_layout_preserves_heading_links_and_inline_styles() {
    let block = AiWorkspaceBlock {
        id: "row-markdown".to_string(),
        source_row_id: "row-markdown".to_string(),
        role: AiWorkspaceBlockRole::Assistant,
        kind: AiWorkspaceBlockKind::Message,
        expandable: false,
        expanded: true,
        title: "Assistant".to_string(),
        preview: "# Heading\nA **bold** [link](https://example.com) with `code`.".to_string(),
        last_sequence: 1,
    };

    let layout = ai_workspace_text_layout_for_block(&block, 640);

    assert_eq!(
        layout.preview_line_kinds.first().copied(),
        Some(ai_workspace_session::AiWorkspacePreviewLineKind::Heading)
    );
    assert!(
        layout
            .preview_line_style_spans
            .iter()
            .flatten()
            .any(|span| span.bold)
    );
    assert!(
        layout
            .preview_line_style_spans
            .iter()
            .flatten()
            .any(|span| span.code)
    );
    assert!(
        layout
            .preview_line_link_ranges
            .iter()
            .flatten()
            .any(|range| range.raw_target == "https://example.com")
    );
}
