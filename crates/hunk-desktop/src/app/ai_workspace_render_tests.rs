use std::sync::Arc;

use gpui::{Bounds, Pixels, point, px, size};

use super::ai_workspace_render::ai_workspace_hit_test;
use super::ai_workspace_session::{
    AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX, AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX,
    AI_WORKSPACE_BLOCK_SECTION_GAP_PX, AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX,
    AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX, AiWorkspaceBlock, AiWorkspaceBlockActionArea,
    AiWorkspaceBlockKind, AiWorkspaceBlockRole, AiWorkspaceBlockTextLayout,
    AiWorkspaceInlineDiffHitTarget, AiWorkspacePreviewColorRole, AiWorkspacePreviewHitTarget,
    AiWorkspaceSession, AiWorkspaceSourceRow, AiWorkspaceSurfaceSnapshot, AiWorkspaceViewportBlock,
    AiWorkspaceViewportSnapshot, ai_workspace_text_layout_for_block,
};
use crate::app::AiTextSelectionSurfaceSpec;

fn test_block(
    id: &str,
    kind: AiWorkspaceBlockKind,
    role: AiWorkspaceBlockRole,
    title: &str,
    preview: &str,
    open_review_tab: bool,
) -> AiWorkspaceBlock {
    AiWorkspaceBlock {
        id: id.to_string(),
        source_row_id: id.to_string(),
        role,
        kind,
        nested: false,
        mono_preview: false,
        open_review_tab,
        expandable: false,
        expanded: true,
        title: title.to_string(),
        preview: preview.to_string(),
        action_area: AiWorkspaceBlockActionArea::Header,
        copy_text: None,
        copy_tooltip: None,
        copy_success_message: None,
        run_in_terminal_command: None,
        run_in_terminal_cwd: None,
        status_label: None,
        status_color_role: Some(AiWorkspacePreviewColorRole::Muted),
        inline_diff_source: None,
        last_sequence: 1,
    }
}

fn viewport_block(
    block: AiWorkspaceBlock,
    top_px: usize,
    width_px: usize,
) -> (AiWorkspaceViewportBlock, AiWorkspaceBlockTextLayout) {
    let text_layout = ai_workspace_text_layout_for_block(&block, width_px);
    (
        AiWorkspaceViewportBlock {
            block,
            top_px,
            height_px: text_layout.height_px,
            text_layout: text_layout.clone(),
        },
        text_layout,
    )
}

#[test]
fn ai_workspace_hit_test_ignores_gutter_clicks_outside_block_bounds() {
    let (block, layout) = viewport_block(
        test_block(
            "row-1",
            AiWorkspaceBlockKind::DiffSummary,
            AiWorkspaceBlockRole::Tool,
            "Edited src/app.rs",
            "1 file changed +3 -1",
            true,
        ),
        16,
        800,
    );
    let snapshot = AiWorkspaceSurfaceSnapshot {
        selection_scope_id: "thread-1".to_string(),
        selection_surfaces: Arc::<[AiTextSelectionSurfaceSpec]>::from([]),
        scroll_top_px: 0,
        viewport_height_px: 400,
        viewport: AiWorkspaceViewportSnapshot {
            total_surface_height_px: 16 + layout.height_px + 16,
            visible_pixel_range: Some(0..400),
            visible_blocks: vec![block],
        },
    };
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: size(px(800.0), px(400.0)),
    };

    let inside_hit = ai_workspace_hit_test(&snapshot, point(px(48.0), px(28.0)), bounds, None);
    assert!(inside_hit.is_some(), "expected click inside bubble to hit");

    let gutter_hit = ai_workspace_hit_test(&snapshot, point(px(4.0), px(28.0)), bounds, None);
    assert!(
        gutter_hit.is_none(),
        "clicking empty gutter space should not hit the bubble"
    );
}

#[test]
fn ai_workspace_hit_test_exposes_inline_diff_line_targets() {
    let mut block = test_block(
        "row-diff",
        AiWorkspaceBlockKind::DiffSummary,
        AiWorkspaceBlockRole::Tool,
        "Edited src/main.rs",
        "1 file changed +2 -2",
        true,
    );
    block.inline_diff_source = Some(Arc::<str>::from(
        "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,2 @@
-fn old() {}
+fn new() {}
 ",
    ));
    block.expandable = true;
    block.expanded = true;
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        Arc::<[AiWorkspaceSourceRow]>::from([AiWorkspaceSourceRow {
            row_id: "row-diff".to_string(),
            last_sequence: 1,
        }]),
        vec![block],
    );
    let snapshot = session.surface_snapshot_with_stats(0, 640, 800).snapshot;
    let block = snapshot
        .viewport
        .visible_blocks
        .first()
        .expect("expanded diff block should be visible");
    let file_header_index = block
        .text_layout
        .preview_line_hit_targets
        .iter()
        .position(|target| {
            matches!(
                target,
                Some(AiWorkspacePreviewHitTarget::InlineDiff(
                    AiWorkspaceInlineDiffHitTarget::FileHeader { .. }
                ))
            )
        })
        .expect("file header preview target should exist");
    let added_line_index = block
        .text_layout
        .preview_line_hit_targets
        .iter()
        .position(|target| {
            matches!(
                target,
                Some(AiWorkspacePreviewHitTarget::InlineDiff(
                    AiWorkspaceInlineDiffHitTarget::Line { kind, .. }
                )) if *kind == crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffLineKind::Added
            )
        })
        .expect("added line preview target should exist");
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: size(px(800.0), px(400.0)),
    };
    let preview_start_y = AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX
        + AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX
        + block.text_layout.title_lines.len() * AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX
        + AI_WORKSPACE_BLOCK_SECTION_GAP_PX;

    let file_header_hit =
        ai_workspace_hit_test(
            &snapshot,
            point(
                px(400.0),
                px((preview_start_y
                    + file_header_index * AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX
                    + 6) as f32),
            ),
            bounds,
            None,
        )
        .expect("file header hit should resolve");
    assert!(matches!(
        file_header_hit.preview_hit_target,
        Some(AiWorkspacePreviewHitTarget::InlineDiff(
            AiWorkspaceInlineDiffHitTarget::FileHeader { file_index: 0 }
        ))
    ));

    let added_line_hit = ai_workspace_hit_test(
        &snapshot,
        point(
            px(400.0),
            px(
                (preview_start_y + added_line_index * AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX + 6)
                    as f32,
            ),
        ),
        bounds,
        None,
    )
    .expect("added line hit should resolve");
    assert!(matches!(
        added_line_hit.preview_hit_target,
        Some(AiWorkspacePreviewHitTarget::InlineDiff(
            AiWorkspaceInlineDiffHitTarget::Line {
                file_index: 0,
                hunk_index: 0,
                kind: crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffLineKind::Added,
                ..
            }
        ))
    ));
    assert!(
        added_line_hit.toggle_row_id.is_none(),
        "expanded inline diff line hits should stay line-local"
    );
}

#[test]
fn ai_workspace_collapsed_diff_summary_click_toggles_instead_of_opening_review() {
    let mut block = test_block(
        "row-diff",
        AiWorkspaceBlockKind::DiffSummary,
        AiWorkspaceBlockRole::Tool,
        "Edited src/main.rs",
        "1 file changed +2 -2",
        false,
    );
    block.inline_diff_source = Some(Arc::<str>::from(
        "\
diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-fn old() {}
+fn new() {}
",
    ));
    block.expandable = true;
    block.expanded = false;
    let mut session = AiWorkspaceSession::new(
        "thread-1",
        Arc::<[AiWorkspaceSourceRow]>::from([AiWorkspaceSourceRow {
            row_id: "row-diff".to_string(),
            last_sequence: 1,
        }]),
        vec![block],
    );
    let snapshot = session.surface_snapshot_with_stats(0, 640, 800).snapshot;
    let bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: size(px(800.0), px(400.0)),
    };
    let hit = ai_workspace_hit_test(&snapshot, point(px(400.0), px(52.0)), bounds, None)
        .expect("collapsed diff summary click should resolve");

    assert_eq!(hit.toggle_row_id.as_deref(), Some("row-diff"));
    assert!(
        !hit.open_review_tab,
        "collapsed diff summaries should no longer navigate to Review on primary click"
    );
}
