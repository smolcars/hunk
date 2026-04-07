enum AiWorkspaceOverlayButtonKind {
    Copy {
        text: String,
        success_message: &'static str,
        message_copy: bool,
    },
    RunInTerminal {
        command: String,
        cwd: Option<std::path::PathBuf>,
    },
}

struct AiWorkspaceOverlayButton {
    id: String,
    tooltip: &'static str,
    kind: AiWorkspaceOverlayButtonKind,
}

struct AiWorkspaceOverlayButtonCluster {
    id: String,
    left_px: usize,
    top_px: usize,
    status_label: Option<String>,
    status_color: Option<gpui::Hsla>,
    buttons: Vec<AiWorkspaceOverlayButton>,
}

#[derive(Clone, Copy)]
struct AiWorkspaceOverlayColors {
    muted_foreground: gpui::Hsla,
    border: gpui::Hsla,
    background: gpui::Hsla,
    accent: gpui::Hsla,
    success: gpui::Hsla,
    danger: gpui::Hsla,
}

impl DiffViewer {
    fn current_ai_workspace_surface_snapshot(
        &mut self,
    ) -> Option<ai_workspace_session::AiWorkspaceSurfaceSnapshot> {
        let scroll_top_px = self.current_ai_workspace_surface_scroll_top_px();
        let viewport_bounds = self.ai_workspace_surface_scroll_handle.bounds();
        let viewport_height_px = viewport_bounds.size.height.max(Pixels::ZERO).as_f32().round()
            as usize;
        let viewport_width_px = viewport_bounds.size.width.max(Pixels::ZERO).as_f32().round()
            as usize;
        let snapshot_result = {
            let session = self.ai_workspace_session.as_mut()?;
            session.surface_snapshot_with_stats(
                scroll_top_px,
                viewport_height_px.max(1),
                viewport_width_px.max(1),
            )
        };
        if let Some(duration) = snapshot_result.geometry_rebuild_duration {
            self.record_ai_workspace_surface_geometry_rebuild_timing(duration);
        }
        Some(snapshot_result.snapshot)
    }

    fn render_ai_workspace_surface_scroller(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let viewport_bounds = self.ai_workspace_surface_scroll_handle.bounds();
        let viewport_width_px = viewport_bounds.size.width.max(Pixels::ZERO).as_f32().round()
            as usize;
        let surface = self.current_ai_workspace_surface_snapshot()?;
        let scroll_handle = self.ai_workspace_surface_scroll_handle.clone();
        let viewport_height_px = surface.viewport.total_surface_height_px;
        let hovered_block_id = self.ai_hovered_workspace_block_id.clone();
        let workspace_root = self
            .ai_workspace_cwd()
            .or_else(|| self.selected_git_workspace_root())
            .or_else(|| self.repo_root.clone());

        Some(
            div()
                .id("ai-workspace-surface-scroll")
                .size_full()
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(viewport_height_px as f32))
                        .when_some(surface.viewport.visible_pixel_range.clone(), |this, range| {
                            this.child(
                                div()
                                    .absolute()
                                    .top(px(range.start as f32))
                                    .left_0()
                                    .right_0()
                                    .h(px(range.len() as f32))
                                    .child(
                                        crate::app::ai_workspace_surface::AiWorkspaceSurfaceElement {
                                            view: cx.entity(),
                                            snapshot: std::rc::Rc::new(surface.clone()),
                                            selection: self.ai_workspace_selection.clone(),
                                            ui_font_family: cx.theme().font_family.clone(),
                                            mono_font_family: cx.theme().mono_font_family.clone(),
                                            workspace_root: workspace_root.clone(),
                                        }
                                        .into_any_element(),
                                    ),
                            )
                            .children(ai_workspace_overlay_action_clusters(
                                cx.entity(),
                                &surface,
                                viewport_width_px.max(1),
                                hovered_block_id.as_deref(),
                                AiWorkspaceOverlayColors {
                                    muted_foreground: cx.theme().muted_foreground,
                                    border: cx.theme().border,
                                    background: cx.theme().secondary_hover,
                                    accent: cx.theme().accent,
                                    success: cx.theme().success,
                                    danger: cx.theme().danger,
                                },
                            ))
                        }),
                )
                .into_any_element(),
        )
    }
}

fn ai_workspace_overlay_action_clusters(
    view: Entity<DiffViewer>,
    surface: &ai_workspace_session::AiWorkspaceSurfaceSnapshot,
    viewport_width_px: usize,
    hovered_block_id: Option<&str>,
    colors: AiWorkspaceOverlayColors,
) -> Vec<AnyElement> {
    let mut actions = Vec::new();

    for block in &surface.viewport.visible_blocks {
        let message_copy = block.block.copy_tooltip == Some("Copy message");
        let block_hovered = hovered_block_id == Some(block.block.id.as_str());
        let mut block_buttons = Vec::new();
        if let Some(command) = block.block.run_in_terminal_command.clone() {
            block_buttons.push(AiWorkspaceOverlayButton {
                id: format!("ai-workspace-run-terminal-{}", block.block.id),
                tooltip: "Run in terminal",
                kind: AiWorkspaceOverlayButtonKind::RunInTerminal {
                    command,
                    cwd: block.block.run_in_terminal_cwd.clone(),
                },
            });
        }
        if let Some(copy_text) = block.block.copy_text.clone()
            && (!message_copy || block_hovered)
        {
            block_buttons.push(AiWorkspaceOverlayButton {
                id: format!("ai-workspace-copy-{}", block.block.id),
                tooltip: block.block.copy_tooltip.unwrap_or("Copy"),
                kind: AiWorkspaceOverlayButtonKind::Copy {
                    text: copy_text,
                    success_message: block.block.copy_success_message.unwrap_or("Copied."),
                    message_copy: block.block.copy_tooltip == Some("Copy message"),
                },
            });
        }
        if !block_buttons.is_empty() {
            let additional_top_px = match block.block.action_area {
                ai_workspace_session::AiWorkspaceBlockActionArea::Header => 0,
                ai_workspace_session::AiWorkspaceBlockActionArea::Preview => {
                    ai_workspace_preview_top_px(block).saturating_sub(6)
                }
            };
            let (left_px, top_px) = ai_workspace_overlay_cluster_position(
                block,
                viewport_width_px,
                additional_top_px,
                block_buttons.len(),
                block
                    .block
                    .status_label
                    .as_deref()
                    .filter(|_| {
                        matches!(
                            block.block.action_area,
                            ai_workspace_session::AiWorkspaceBlockActionArea::Preview
                        )
                    }),
                matches!(
                    block.block.action_area,
                    ai_workspace_session::AiWorkspaceBlockActionArea::Header
                ),
            );
            actions.push(ai_workspace_overlay_button_cluster(
                view.clone(),
                AiWorkspaceOverlayButtonCluster {
                    id: format!("ai-workspace-actions-{}", block.block.id),
                    left_px,
                    top_px,
                    status_label: matches!(
                        block.block.action_area,
                        ai_workspace_session::AiWorkspaceBlockActionArea::Preview
                    )
                    .then(|| block.block.status_label.clone())
                    .flatten(),
                    status_color: matches!(
                        block.block.action_area,
                        ai_workspace_session::AiWorkspaceBlockActionArea::Preview
                    )
                    .then(|| {
                        block.block.status_color_role.map(|role| match role {
                            ai_workspace_session::AiWorkspacePreviewColorRole::Accent => {
                                colors.accent
                            }
                            ai_workspace_session::AiWorkspacePreviewColorRole::Added => {
                                colors.success
                            }
                            ai_workspace_session::AiWorkspacePreviewColorRole::Removed => {
                                colors.danger
                            }
                            ai_workspace_session::AiWorkspacePreviewColorRole::Foreground
                            | ai_workspace_session::AiWorkspacePreviewColorRole::Muted => {
                                colors.muted_foreground
                            }
                        })
                    })
                    .flatten(),
                    buttons: block_buttons,
                },
                colors.muted_foreground,
                colors.border,
                colors.background,
            ));
        }

        for (copy_index, copy_region) in block.text_layout.preview_copy_regions.iter().enumerate() {
            let (left_px, top_px) = ai_workspace_overlay_cluster_position(
                block,
                viewport_width_px,
                ai_workspace_preview_top_px(block).saturating_add(
                    ai_workspace_session::AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX
                        * copy_region.line_range.start,
                ),
                1,
                None,
                false,
            );
            actions.push(ai_workspace_overlay_button_cluster(
                view.clone(),
                AiWorkspaceOverlayButtonCluster {
                    id: format!("ai-workspace-copy-region-{}-{copy_index}", block.block.id),
                    left_px,
                    top_px,
                    status_label: None,
                    status_color: None,
                    buttons: vec![AiWorkspaceOverlayButton {
                        id: format!("ai-workspace-copy-{}-{copy_index}", block.block.id),
                        tooltip: copy_region.tooltip,
                        kind: AiWorkspaceOverlayButtonKind::Copy {
                            text: copy_region.text.clone(),
                            success_message: copy_region.success_message,
                            message_copy: false,
                        },
                    }],
                },
                colors.muted_foreground,
                colors.border,
                colors.background,
            ));
        }
    }

    actions
}

fn ai_workspace_overlay_button_cluster(
    view: Entity<DiffViewer>,
    spec: AiWorkspaceOverlayButtonCluster,
    muted_foreground: gpui::Hsla,
    border_color: gpui::Hsla,
    background_color: gpui::Hsla,
) -> AnyElement {
    div()
        .absolute()
        .left(px(spec.left_px as f32))
        .top(px(spec.top_px as f32))
        .child(
            h_flex()
                .id(spec.id)
                .gap_1()
                .px_1()
                .py_1()
                .rounded(px(8.0))
                .border_1()
                .border_color(border_color)
                .bg(background_color)
                .when_some(spec.status_label.clone(), |this, status_label| {
                    this.child(
                        div()
                            .flex_none()
                            .rounded(px(999.0))
                            .border_1()
                            .border_color(spec.status_color.unwrap_or(muted_foreground))
                            .bg(spec.status_color.unwrap_or(muted_foreground).opacity(0.12))
                            .px_1p5()
                            .py_0p5()
                            .text_xs()
                            .text_color(spec.status_color.unwrap_or(muted_foreground))
                            .child(status_label),
                    )
                })
                .children(spec.buttons.into_iter().map(|button| {
                    let view = view.clone();
                    let button_id = button.id;
                    let tooltip = button.tooltip;
                    let kind = button.kind;
                    Button::new(button_id)
                        .ghost()
                        .compact()
                        .rounded(px(7.0))
                        .icon(match &kind {
                            AiWorkspaceOverlayButtonKind::Copy { .. } => {
                                Icon::new(IconName::Copy).size(px(12.0))
                            }
                            AiWorkspaceOverlayButtonKind::RunInTerminal { .. } => {
                                Icon::new(IconName::SquareTerminal).size(px(13.0))
                            }
                        })
                        .text_color(muted_foreground)
                        .min_w(px(22.0))
                        .h(px(20.0))
                        .tooltip(tooltip)
                        .on_click(move |_, window, cx| {
                            view.update(cx, |this, cx| match &kind {
                                AiWorkspaceOverlayButtonKind::Copy {
                                    text,
                                    success_message,
                                    message_copy,
                                } => {
                                    if *message_copy {
                                        this.ai_copy_message_action(text.clone(), window, cx);
                                    } else {
                                        this.ai_copy_text_action(
                                            text.clone(),
                                            success_message,
                                            window,
                                            cx,
                                        );
                                    }
                                }
                                AiWorkspaceOverlayButtonKind::RunInTerminal { command, cwd } => {
                                    this.ai_run_command_in_terminal(
                                        cwd.clone(),
                                        command.clone(),
                                        cx,
                                    );
                                }
                            });
                        })
                })),
        )
        .into_any_element()
}

fn ai_workspace_preview_top_px(block: &ai_workspace_session::AiWorkspaceViewportBlock) -> usize {
    ai_workspace_session::AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX
        + ai_workspace_session::AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX
            * block.text_layout.title_lines.len()
        + if block.text_layout.preview_lines.is_empty() {
            0
        } else {
            ai_workspace_session::AI_WORKSPACE_BLOCK_SECTION_GAP_PX
        }
}

fn ai_workspace_overlay_cluster_position(
    block: &ai_workspace_session::AiWorkspaceViewportBlock,
    viewport_width_px: usize,
    additional_top_px: usize,
    button_count: usize,
    status_label: Option<&str>,
    reserve_toggle_space: bool,
) -> (usize, usize) {
    const BUTTON_WIDTH_PX: usize = 28;
    const BUTTON_GAP_PX: usize = 6;
    const CLUSTER_PADDING_PX: usize = 10;
    const BUTTON_RIGHT_PADDING_PX: usize = 10;
    const STATUS_CHAR_WIDTH_PX: usize = 9;
    const STATUS_PILL_MIN_WIDTH_PX: usize = 64;

    let lane_max_width = if block.block.role == ai_workspace_session::AiWorkspaceBlockRole::User {
        crate::app::ai_workspace_timeline_projection::AI_WORKSPACE_USER_CONTENT_LANE_MAX_WIDTH_PX
    } else {
        crate::app::ai_workspace_timeline_projection::AI_WORKSPACE_CONTENT_LANE_MAX_WIDTH_PX
    };
    let lane_width = viewport_width_px
        .saturating_sub(ai_workspace_session::AI_WORKSPACE_SURFACE_BLOCK_SIDE_PADDING_PX * 2)
        .min(lane_max_width);
    let lane_x = viewport_width_px.saturating_sub(lane_width) / 2;
    let block_x = match block.block.role {
        ai_workspace_session::AiWorkspaceBlockRole::User => lane_x
            .saturating_add(lane_width)
            .saturating_sub(block.text_layout.block_width_px),
        _ => lane_x.saturating_add(usize::from(block.block.nested) * 16),
    };
    let block_right_px = block_x.saturating_add(block.text_layout.block_width_px);
    let cluster_width = CLUSTER_PADDING_PX
        .saturating_add(button_count * BUTTON_WIDTH_PX)
        .saturating_add(button_count.saturating_sub(1) * BUTTON_GAP_PX)
        .saturating_add(status_label.map_or(0, |label| {
            STATUS_PILL_MIN_WIDTH_PX.max(
                label
                    .chars()
                    .count()
                    .saturating_mul(STATUS_CHAR_WIDTH_PX)
                    .saturating_add(32),
            ) + BUTTON_GAP_PX
        }))
        .saturating_add(CLUSTER_PADDING_PX);
    let toggle_reserve = if reserve_toggle_space && block.block.expandable {
        36
    } else {
        0
    };

    (
        block_right_px
            .saturating_sub(cluster_width + BUTTON_RIGHT_PADDING_PX + toggle_reserve),
        block.top_px.saturating_add(4).saturating_add(additional_top_px),
    )
}
