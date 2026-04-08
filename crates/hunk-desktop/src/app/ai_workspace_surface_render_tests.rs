use std::sync::Arc;

use super::ai_workspace_session::{
    AiWorkspaceBlock, AiWorkspaceBlockActionArea, AiWorkspaceBlockKind, AiWorkspaceBlockRole,
    AiWorkspaceBlockTextLayout, AiWorkspacePreviewColorRole, AiWorkspaceViewportBlock,
    ai_workspace_text_layout_for_block,
};
use super::render::{AiWorkspaceOverlayButtonKind, ai_workspace_overlay_buttons_for_block};

fn test_block(
    id: &str,
    kind: AiWorkspaceBlockKind,
    role: AiWorkspaceBlockRole,
    title: &str,
    preview: &str,
) -> AiWorkspaceBlock {
    AiWorkspaceBlock {
        id: id.to_string(),
        source_row_id: id.to_string(),
        role,
        kind,
        nested: false,
        mono_preview: false,
        open_review_tab: false,
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
fn diff_blocks_expose_open_review_and_side_pane_actions() {
    let mut block = test_block(
        "row-diff",
        AiWorkspaceBlockKind::DiffSummary,
        AiWorkspaceBlockRole::Tool,
        "Edited src/main.rs",
        "1 file changed +2 -2",
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
    let (viewport_block, _) = viewport_block(block, 16, 800);

    let buttons = ai_workspace_overlay_buttons_for_block(&viewport_block, false);

    assert!(buttons.iter().any(|button| {
        matches!(
            button.kind,
            AiWorkspaceOverlayButtonKind::OpenSidePane { ref row_id } if row_id == "row-diff"
        )
    }));
    assert!(
        buttons
            .iter()
            .any(|button| { matches!(button.kind, AiWorkspaceOverlayButtonKind::OpenReviewTab) })
    );
}

#[test]
fn message_copy_button_stays_hover_only() {
    let mut block = test_block(
        "row-message",
        AiWorkspaceBlockKind::Message,
        AiWorkspaceBlockRole::Assistant,
        "Assistant",
        "Hello from the AI workspace",
    );
    block.copy_text = Some(block.preview.clone());
    block.copy_tooltip = Some("Copy message");
    block.copy_success_message = Some("Copied message.");
    let (viewport_block, _) = viewport_block(block, 16, 800);

    let unhovered_buttons = ai_workspace_overlay_buttons_for_block(&viewport_block, false);
    assert!(
        unhovered_buttons.is_empty(),
        "message copy should remain hover-only"
    );

    let hovered_buttons = ai_workspace_overlay_buttons_for_block(&viewport_block, true);
    assert_eq!(hovered_buttons.len(), 1);
    assert!(matches!(
        hovered_buttons[0].kind,
        AiWorkspaceOverlayButtonKind::Copy {
            message_copy: true,
            ..
        }
    ));
}
