use gpui::{Bounds, Pixels, point, px, size};

use super::ai_workspace_render::ai_workspace_hit_test;
use super::ai_workspace_session::{
    AiWorkspaceBlock, AiWorkspaceBlockActionArea, AiWorkspaceBlockKind, AiWorkspaceBlockRole,
    AiWorkspaceBlockTextLayout, AiWorkspacePreviewColorRole, AiWorkspaceSurfaceSnapshot,
    AiWorkspaceViewportBlock, AiWorkspaceViewportSnapshot, ai_workspace_text_layout_for_block,
};
use crate::app::AiTextSelectionSurfaceSpec;

fn test_block(
    id: &str,
    kind: AiWorkspaceBlockKind,
    role: AiWorkspaceBlockRole,
    title: &str,
    preview: &str,
    open_side_diff_pane: bool,
) -> AiWorkspaceBlock {
    AiWorkspaceBlock {
        id: id.to_string(),
        source_row_id: id.to_string(),
        role,
        kind,
        nested: false,
        mono_preview: false,
        open_side_diff_pane,
        expandable: false,
        expanded: true,
        title: title.to_string(),
        preview: preview.to_string(),
        preferred_review_path: open_side_diff_pane.then(|| "src/app.rs".to_string()),
        action_area: AiWorkspaceBlockActionArea::Header,
        copy_text: None,
        copy_tooltip: None,
        copy_success_message: None,
        run_in_terminal_command: None,
        run_in_terminal_cwd: None,
        status_label: None,
        status_color_role: Some(AiWorkspacePreviewColorRole::Muted),
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
        selection_surfaces: std::sync::Arc::<[AiTextSelectionSurfaceSpec]>::from([]),
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
fn diff_summary_hit_test_exposes_side_pane_row_id() {
    let (block, layout) = viewport_block(
        test_block(
            "row-diff",
            AiWorkspaceBlockKind::DiffSummary,
            AiWorkspaceBlockRole::Tool,
            "Edited src/main.rs",
            "1 file changed +2 -2",
            true,
        ),
        16,
        800,
    );
    let snapshot = AiWorkspaceSurfaceSnapshot {
        selection_scope_id: "thread-1".to_string(),
        selection_surfaces: std::sync::Arc::<[AiTextSelectionSurfaceSpec]>::from([]),
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

    let hit = ai_workspace_hit_test(&snapshot, point(px(400.0), px(28.0)), bounds, None)
        .expect("diff summary hit should resolve");

    assert_eq!(hit.open_side_diff_pane_row_id.as_deref(), Some("row-diff"));
}
