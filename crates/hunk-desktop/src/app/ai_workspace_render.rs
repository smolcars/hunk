use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use gpui::{
    App, Bounds, Entity, Font, FontWeight, Pixels, Point, SharedString, TextRun, TextStyle,
    TransformationMatrix, Window, fill, point, px, relative, size,
};
use gpui_component::ActiveTheme as _;
use gpui_component::{IconName, IconNamed};

use crate::app::markdown_links::{MarkdownLinkRange, resolve_markdown_link_target};
use crate::app::native_files_editor::paint::{
    paint_editor_line, shape_editor_line, single_color_text_run,
};
use crate::app::{AiTextSelectionSurfaceSpec, DiffViewer, ai_workspace_session};

#[derive(Clone)]
struct AiWorkspaceBlockRenderLayout {
    block_bounds: Bounds<Pixels>,
    text_origin_x: Pixels,
    title_origin_y: Pixels,
    preview_origin_y: Pixels,
    title_line_height: Pixels,
    preview_line_height: Pixels,
    title_char_width: Pixels,
    preview_char_width: Pixels,
    preview_mono_char_width: Pixels,
    toggle_bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone)]
struct AiWorkspacePaintLine {
    surface_id: String,
    text: String,
    surface_byte_range: Range<usize>,
    column_byte_offsets: Arc<[usize]>,
    link_ranges: Arc<[MarkdownLinkRange]>,
    style_spans: Arc<[ai_workspace_session::AiWorkspacePreviewStyleSpan]>,
    syntax_spans: Arc<[ai_workspace_session::AiWorkspacePreviewSyntaxSpan]>,
    origin: Point<Pixels>,
    line_height: Pixels,
    char_width: Pixels,
    title: bool,
    preview_kind: ai_workspace_session::AiWorkspacePreviewLineKind,
}

#[derive(Clone)]
pub(crate) struct AiWorkspaceTextHit {
    pub(crate) surface_id: String,
    pub(crate) index: usize,
    pub(crate) link_target: Option<String>,
    pub(crate) selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
}

#[derive(Clone)]
pub(crate) struct AiWorkspaceBlockHit {
    pub(crate) selection: ai_workspace_session::AiWorkspaceSelection,
    pub(crate) text_hit: Option<AiWorkspaceTextHit>,
    pub(crate) toggle_row_id: Option<String>,
    pub(crate) open_side_diff_pane_row_id: Option<String>,
}

fn ai_workspace_primary_click_toggles_block(
    block: &ai_workspace_session::AiWorkspaceViewportBlock,
    selection: &ai_workspace_session::AiWorkspaceSelection,
) -> bool {
    if !block.block.expandable {
        return false;
    }

    matches!(
        block.block.kind,
        ai_workspace_session::AiWorkspaceBlockKind::Group
    ) && matches!(
        selection.region,
        ai_workspace_session::AiWorkspaceSelectionRegion::Block
            | ai_workspace_session::AiWorkspaceSelectionRegion::Title
    )
}

pub(crate) fn ai_workspace_hit_test(
    snapshot: &ai_workspace_session::AiWorkspaceSurfaceSnapshot,
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    workspace_root: Option<&Path>,
) -> Option<AiWorkspaceBlockHit> {
    if !bounds.contains(&position) {
        return None;
    }

    let local_y_px = (position.y - bounds.origin.y)
        .max(Pixels::ZERO)
        .as_f32()
        .round() as usize;
    let surface_y_px = snapshot.scroll_top_px.saturating_add(local_y_px);
    let block = snapshot.viewport.visible_blocks.iter().find(|block| {
        let bottom_px = block.top_px.saturating_add(block.height_px);
        surface_y_px >= block.top_px && surface_y_px < bottom_px
    })?;
    let render_layout = ai_workspace_block_render_layout(bounds, snapshot.scroll_top_px, block);
    if !render_layout.block_bounds.contains(&position) {
        return None;
    }
    let block_local_y_px = surface_y_px.saturating_sub(block.top_px);
    let title_start_px = ai_workspace_session::AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX;
    let title_height_px = block.text_layout.title_lines.len()
        * ai_workspace_session::AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX;
    let preview_start_px = title_start_px
        .saturating_add(title_height_px)
        .saturating_add(if block.text_layout.preview_lines.is_empty() {
            0
        } else {
            ai_workspace_session::AI_WORKSPACE_BLOCK_SECTION_GAP_PX
        });
    let preview_height_px = block.text_layout.preview_lines.len()
        * ai_workspace_session::AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX;

    let (region, line_index) = if block_local_y_px >= title_start_px
        && block_local_y_px < title_start_px.saturating_add(title_height_px)
    {
        let line_index = (block_local_y_px.saturating_sub(title_start_px)
            / ai_workspace_session::AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX)
            .min(block.text_layout.title_lines.len().saturating_sub(1));
        (
            ai_workspace_session::AiWorkspaceSelectionRegion::Title,
            Some(line_index),
        )
    } else if block_local_y_px >= preview_start_px
        && block_local_y_px < preview_start_px.saturating_add(preview_height_px)
    {
        let line_index = (block_local_y_px.saturating_sub(preview_start_px)
            / ai_workspace_session::AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX)
            .min(block.text_layout.preview_lines.len().saturating_sub(1));
        (
            ai_workspace_session::AiWorkspaceSelectionRegion::Preview,
            Some(line_index),
        )
    } else {
        (
            ai_workspace_session::AiWorkspaceSelectionRegion::Block,
            None,
        )
    };

    let selection = ai_workspace_session::AiWorkspaceSelection {
        block_id: block.block.id.clone(),
        block_kind: block.block.kind,
        line_index,
        region,
    };
    let toggle_row_id = render_layout
        .toggle_bounds
        .filter(|toggle_bounds| toggle_bounds.contains(&position))
        .map(|_| block.block.source_row_id.clone())
        .or_else(|| {
            ai_workspace_primary_click_toggles_block(block, &selection)
                .then(|| block.block.source_row_id.clone())
        });
    let text_hit = ai_workspace_text_hit(
        block,
        &render_layout,
        position,
        workspace_root,
        snapshot.selection_surfaces.clone(),
        false,
    );

    Some(AiWorkspaceBlockHit {
        selection,
        text_hit,
        toggle_row_id,
        open_side_diff_pane_row_id: block
            .block
            .open_side_diff_pane
            .then(|| block.block.source_row_id.clone()),
    })
}

pub(crate) fn ai_workspace_drag_text_hit(
    snapshot: &ai_workspace_session::AiWorkspaceSurfaceSnapshot,
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    workspace_root: Option<&Path>,
) -> Option<AiWorkspaceTextHit> {
    if let Some(hit) = ai_workspace_hit_test(snapshot, position, bounds, workspace_root)
        .and_then(|hit| hit.text_hit)
    {
        return Some(hit);
    }

    let local_y_px = (position.y - bounds.origin.y)
        .max(Pixels::ZERO)
        .as_f32()
        .round() as usize;
    let surface_y_px = snapshot.scroll_top_px.saturating_add(local_y_px);
    let target_block = snapshot
        .viewport
        .visible_blocks
        .iter()
        .min_by_key(|block| {
            if surface_y_px < block.top_px {
                block.top_px.saturating_sub(surface_y_px)
            } else if surface_y_px >= block.top_px.saturating_add(block.height_px) {
                surface_y_px.saturating_sub(block.top_px.saturating_add(block.height_px))
            } else {
                0
            }
        })?;
    let render_layout =
        ai_workspace_block_render_layout(bounds, snapshot.scroll_top_px, target_block);
    ai_workspace_text_hit(
        target_block,
        &render_layout,
        position,
        workspace_root,
        snapshot.selection_surfaces.clone(),
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_ai_workspace_block(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    scroll_top_px: usize,
    block: &ai_workspace_session::AiWorkspaceViewportBlock,
    selected: bool,
    hovered: bool,
    view: Entity<DiffViewer>,
    ui_font_family: SharedString,
    mono_font_family: SharedString,
    workspace_root: Option<&Path>,
) {
    let render_layout = ai_workspace_block_render_layout(bounds, scroll_top_px, block);
    let is_dark = cx.theme().mode.is_dark();
    let (background, border, accent, title_color, preview_color, link_color) =
        ai_workspace_block_palette(
            block.block.kind,
            block.block.role,
            selected,
            hovered,
            is_dark,
            cx,
        );

    if background.a > 0.0 {
        window.paint_quad(fill(render_layout.block_bounds, background));
    }
    if selected && border.a > 0.0 {
        window.paint_quad(fill(
            Bounds {
                origin: point(
                    render_layout.block_bounds.origin.x,
                    render_layout.block_bounds.origin.y,
                ),
                size: size(render_layout.block_bounds.size.width, px(1.0)),
            },
            border,
        ));
        window.paint_quad(fill(
            Bounds {
                origin: point(
                    render_layout.block_bounds.origin.x,
                    render_layout.block_bounds.origin.y + render_layout.block_bounds.size.height
                        - px(1.0),
                ),
                size: size(render_layout.block_bounds.size.width, px(1.0)),
            },
            border,
        ));
    }

    if block.block.mono_preview && !block.text_layout.preview_lines.is_empty() {
        let preview_bounds = Bounds {
            origin: point(
                render_layout.block_bounds.origin.x
                    + px(ai_workspace_session::AI_WORKSPACE_BLOCK_TEXT_SIDE_PADDING_PX as f32),
                render_layout.preview_origin_y - px(6.0),
            ),
            size: size(
                render_layout.block_bounds.size.width
                    - px(
                        ai_workspace_session::AI_WORKSPACE_BLOCK_TEXT_SIDE_PADDING_PX as f32 * 2.0,
                    ),
                render_layout.preview_line_height * block.text_layout.preview_lines.len() as f32
                    + px(12.0),
            ),
        };
        window.paint_quad(fill(
            preview_bounds,
            crate::app::theme::hunk_blend(
                cx.theme().background,
                cx.theme().muted,
                is_dark,
                0.10,
                0.14,
            ),
        ));
        window.paint_quad(fill(
            Bounds {
                origin: preview_bounds.origin,
                size: size(preview_bounds.size.width, px(1.0)),
            },
            crate::app::theme::hunk_opacity(cx.theme().border, is_dark, 0.85, 0.68),
        ));
    }

    if let Some(toggle_bounds) = render_layout.toggle_bounds {
        let toggle_icon_size = px(14.0);
        let toggle_icon = if block.block.expanded {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let toggle_icon_path = SharedString::from(toggle_icon.path().to_string());
        let toggle_icon_bounds = Bounds {
            origin: point(
                toggle_bounds.origin.x + toggle_bounds.size.width - px(18.0),
                toggle_bounds.origin.y + px(5.0),
            ),
            size: size(toggle_icon_size, toggle_icon_size),
        };
        let _ = window.paint_svg(
            toggle_icon_bounds,
            toggle_icon_path,
            None,
            TransformationMatrix::default(),
            accent,
            cx,
        );
    }

    let lines = ai_workspace_paint_lines_for_block(block, &render_layout, workspace_root);
    let current_selection = view.read(cx).ai_text_selection.clone();
    let selection_background =
        crate::app::theme::hunk_text_selection_background(cx.theme(), is_dark);

    for line in &lines {
        ai_workspace_paint_selection(
            line,
            current_selection
                .as_ref()
                .and_then(|selection| selection.range_for_surface(line.surface_id.as_str())),
            selection_background,
            window,
        );
        if !line.title
            && line.preview_kind == ai_workspace_session::AiWorkspacePreviewLineKind::Rule
        {
            window.paint_quad(fill(
                Bounds {
                    origin: point(
                        line.origin.x,
                        line.origin.y + (line.line_height - px(1.0)) / 2.0,
                    ),
                    size: size(
                        render_layout.block_bounds.size.width
                            - px(
                                ai_workspace_session::AI_WORKSPACE_BLOCK_TEXT_SIDE_PADDING_PX
                                    as f32
                                    * 2.0,
                            ),
                        px(1.0),
                    ),
                },
                crate::app::theme::hunk_opacity(cx.theme().border, is_dark, 0.8, 0.95),
            ));
            continue;
        }
        if let Some(diff_background) =
            ai_workspace_preview_line_background(line.preview_kind, cx.theme(), is_dark)
        {
            window.paint_quad(fill(
                Bounds {
                    origin: point(line.origin.x - px(4.0), line.origin.y + px(1.0)),
                    size: size(
                        render_layout.block_bounds.size.width
                            - px(
                                ai_workspace_session::AI_WORKSPACE_BLOCK_TEXT_SIDE_PADDING_PX
                                    as f32
                                    * 2.0,
                            )
                            + px(8.0),
                        (line.line_height - px(2.0)).max(Pixels::ZERO),
                    ),
                },
                diff_background,
            ));
        }
        if line.text.is_empty() {
            continue;
        }
        let (font_family, font_weight, color) = if line.title
            || line.preview_kind == ai_workspace_session::AiWorkspacePreviewLineKind::Heading
        {
            (ui_font_family.clone(), FontWeight::SEMIBOLD, title_color)
        } else if line.preview_kind == ai_workspace_session::AiWorkspacePreviewLineKind::Code {
            (mono_font_family.clone(), FontWeight::NORMAL, preview_color)
        } else if line.preview_kind == ai_workspace_session::AiWorkspacePreviewLineKind::Quote {
            (
                ui_font_family.clone(),
                FontWeight::NORMAL,
                cx.theme().muted_foreground,
            )
        } else {
            (ui_font_family.clone(), FontWeight::NORMAL, preview_color)
        };
        let style = TextStyle {
            color,
            font_family,
            font_size: if line.preview_kind
                == ai_workspace_session::AiWorkspacePreviewLineKind::Heading
            {
                px(13.0).into()
            } else {
                px(12.0).into()
            },
            font_weight,
            line_height: if line.title {
                relative(1.35)
            } else if line.preview_kind == ai_workspace_session::AiWorkspacePreviewLineKind::Heading
            {
                relative(1.40)
            } else {
                relative(1.45)
            },
            ..Default::default()
        };
        let font = style.font();
        let runs = ai_workspace_text_runs_for_line(line, color, link_color, font, cx.theme());
        let shape = shape_editor_line(
            window,
            SharedString::from(line.text.clone()),
            style.font_size.to_pixels(window.rem_size()),
            &runs,
        );
        paint_editor_line(window, cx, &shape, line.origin, line.line_height);
    }
}

fn ai_workspace_block_render_layout(
    bounds: Bounds<Pixels>,
    scroll_top_px: usize,
    block: &ai_workspace_session::AiWorkspaceViewportBlock,
) -> AiWorkspaceBlockRenderLayout {
    let role = block.block.role;
    let surface_top = px(block.top_px as f32 - scroll_top_px as f32);
    let block_height = px(block.height_px as f32);
    let horizontal_padding =
        px(ai_workspace_session::AI_WORKSPACE_SURFACE_BLOCK_SIDE_PADDING_PX as f32);
    let lane_max_width = if role == ai_workspace_session::AiWorkspaceBlockRole::User {
        crate::app::ai_workspace_timeline_projection::AI_WORKSPACE_USER_CONTENT_LANE_MAX_WIDTH_PX
    } else {
        crate::app::ai_workspace_timeline_projection::AI_WORKSPACE_CONTENT_LANE_MAX_WIDTH_PX
    };
    let lane_width = (bounds.size.width.as_f32() - horizontal_padding.as_f32() * 2.0)
        .clamp(0.0, lane_max_width as f32);
    let lane_x = bounds.origin.x + (bounds.size.width - px(lane_width)) / 2.0;
    let block_width = px(block.text_layout.block_width_px as f32);
    let nested_indent = if block.block.nested {
        px(16.0)
    } else {
        px(0.0)
    };
    let block_x = match role {
        ai_workspace_session::AiWorkspaceBlockRole::User => lane_x + px(lane_width) - block_width,
        _ => lane_x + nested_indent,
    };
    let block_bounds = Bounds {
        origin: point(block_x, bounds.origin.y + surface_top),
        size: size(block_width, block_height),
    };
    let text_origin_x = block_bounds.origin.x
        + px(ai_workspace_session::AI_WORKSPACE_BLOCK_TEXT_SIDE_PADDING_PX as f32);
    let title_origin_y = block_bounds.origin.y
        + px(ai_workspace_session::AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX as f32);
    let title_line_height =
        px(ai_workspace_session::AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX as f32);
    let preview_origin_y = title_origin_y
        + title_line_height * block.text_layout.title_lines.len() as f32
        + px(ai_workspace_session::AI_WORKSPACE_BLOCK_SECTION_GAP_PX as f32);
    let preview_line_height =
        px(ai_workspace_session::AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX as f32);
    let text_width_px =
        ai_workspace_session::ai_workspace_block_text_width_px(block.text_layout.block_width_px);
    let title_char_width = px((text_width_px as f32)
        / ai_workspace_session::ai_workspace_chars_per_line(text_width_px, true, false).max(1)
            as f32);
    let preview_char_width = px((text_width_px as f32)
        / ai_workspace_session::ai_workspace_chars_per_line(text_width_px, false, false).max(1)
            as f32);
    let preview_mono_char_width = px((text_width_px as f32)
        / ai_workspace_session::ai_workspace_chars_per_line(text_width_px, false, true).max(1)
            as f32);
    let toggle_bounds = block.block.expandable.then_some(Bounds {
        origin: point(block_bounds.origin.x, block_bounds.origin.y + px(4.0)),
        size: size(block_bounds.size.width, px(24.0)),
    });

    AiWorkspaceBlockRenderLayout {
        block_bounds,
        text_origin_x,
        title_origin_y,
        preview_origin_y,
        title_line_height,
        preview_line_height,
        title_char_width,
        preview_char_width,
        preview_mono_char_width,
        toggle_bounds,
    }
}

fn ai_workspace_paint_lines_for_block(
    block: &ai_workspace_session::AiWorkspaceViewportBlock,
    render_layout: &AiWorkspaceBlockRenderLayout,
    workspace_root: Option<&Path>,
) -> Vec<AiWorkspacePaintLine> {
    let title_surface_id = ai_workspace_title_surface_id(block.block.id.as_str());
    let preview_surface_id = ai_workspace_preview_surface_id(block.block.id.as_str());
    let title_surface_text = block.text_layout.title_lines.join("\n");
    let preview_surface_text = block.text_layout.preview_lines.join("\n");
    let mut lines = Vec::new();

    let mut title_offset = 0usize;
    for (line_index, line_text) in block.text_layout.title_lines.iter().enumerate() {
        let line_len = line_text.len();
        lines.push(AiWorkspacePaintLine {
            surface_id: title_surface_id.clone(),
            text: line_text.clone(),
            surface_byte_range: title_offset..title_offset.saturating_add(line_len),
            column_byte_offsets: Arc::<[usize]>::from(ai_workspace_column_byte_offsets(line_text)),
            link_ranges: Arc::<[MarkdownLinkRange]>::from(ai_workspace_link_ranges(
                line_text,
                workspace_root,
            )),
            style_spans: Arc::<[ai_workspace_session::AiWorkspacePreviewStyleSpan]>::from(
                block
                    .text_layout
                    .title_line_style_spans
                    .get(line_index)
                    .cloned()
                    .unwrap_or_default(),
            ),
            syntax_spans: Arc::from([]),
            origin: point(
                render_layout.text_origin_x,
                render_layout.title_origin_y + render_layout.title_line_height * line_index as f32,
            ),
            line_height: render_layout.title_line_height,
            char_width: render_layout.title_char_width,
            title: true,
            preview_kind: ai_workspace_session::AiWorkspacePreviewLineKind::Normal,
        });
        title_offset = title_offset.saturating_add(line_len).saturating_add(1);
    }

    let mut preview_offset = 0usize;
    for (line_index, line_text) in block.text_layout.preview_lines.iter().enumerate() {
        let line_len = line_text.len();
        let preview_kind = block
            .text_layout
            .preview_line_kinds
            .get(line_index)
            .copied()
            .unwrap_or(ai_workspace_session::AiWorkspacePreviewLineKind::Normal);
        lines.push(AiWorkspacePaintLine {
            surface_id: preview_surface_id.clone(),
            text: line_text.clone(),
            surface_byte_range: preview_offset..preview_offset.saturating_add(line_len),
            column_byte_offsets: Arc::<[usize]>::from(ai_workspace_column_byte_offsets(line_text)),
            link_ranges: Arc::<[MarkdownLinkRange]>::from(
                block
                    .text_layout
                    .preview_line_link_ranges
                    .get(line_index)
                    .cloned()
                    .filter(|ranges| !ranges.is_empty())
                    .unwrap_or_else(|| ai_workspace_link_ranges(line_text, workspace_root)),
            ),
            style_spans: Arc::<[ai_workspace_session::AiWorkspacePreviewStyleSpan]>::from(
                block
                    .text_layout
                    .preview_line_style_spans
                    .get(line_index)
                    .cloned()
                    .unwrap_or_default(),
            ),
            syntax_spans: Arc::<[ai_workspace_session::AiWorkspacePreviewSyntaxSpan]>::from(
                block
                    .text_layout
                    .preview_line_syntax_spans
                    .get(line_index)
                    .cloned()
                    .unwrap_or_default(),
            ),
            origin: point(
                render_layout.text_origin_x,
                render_layout.preview_origin_y
                    + render_layout.preview_line_height * line_index as f32,
            ),
            line_height: render_layout.preview_line_height,
            char_width: if preview_kind.is_monospace() {
                render_layout.preview_mono_char_width
            } else {
                render_layout.preview_char_width
            },
            title: false,
            preview_kind,
        });
        preview_offset = preview_offset.saturating_add(line_len).saturating_add(1);
    }

    if title_surface_text.is_empty() && preview_surface_text.is_empty() {
        return Vec::new();
    }

    lines
}

fn ai_workspace_text_hit(
    block: &ai_workspace_session::AiWorkspaceViewportBlock,
    render_layout: &AiWorkspaceBlockRenderLayout,
    position: Point<Pixels>,
    workspace_root: Option<&Path>,
    selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
    clamp_to_nearest: bool,
) -> Option<AiWorkspaceTextHit> {
    let lines = ai_workspace_paint_lines_for_block(block, render_layout, workspace_root);
    let line = if let Some(line) = lines.iter().find(|line| {
        let line_bounds = Bounds {
            origin: point(line.origin.x, line.origin.y),
            size: size(
                px(
                    (line.column_byte_offsets.len().saturating_sub(1) as f32 + 1.0)
                        * line.char_width.as_f32(),
                ),
                line.line_height,
            ),
        };
        position.y >= line_bounds.origin.y
            && position.y < line_bounds.origin.y + line_bounds.size.height
            && position.x >= render_layout.text_origin_x
            && position.x
                < render_layout.block_bounds.origin.x + render_layout.block_bounds.size.width
    }) {
        line
    } else if clamp_to_nearest {
        lines.iter().min_by_key(|line| {
            let line_top = line.origin.y;
            let line_bottom = line.origin.y + line.line_height;
            if position.y < line_top {
                (line_top - position.y).as_f32().round() as usize
            } else if position.y > line_bottom {
                (position.y - line_bottom).as_f32().round() as usize
            } else {
                0
            }
        })?
    } else {
        return None;
    };
    let relative_x = (position.x - render_layout.text_origin_x).max(Pixels::ZERO);
    let max_column = line.column_byte_offsets.len().saturating_sub(1);
    let unclamped_column = (relative_x / line.char_width).floor() as usize;
    let column = unclamped_column.min(max_column);
    let index = line.surface_byte_range.start + line.column_byte_offsets[column];
    let line_local_index = line.column_byte_offsets[column];
    let link_target = line
        .link_ranges
        .iter()
        .find(|range| range.range.contains(&line_local_index))
        .map(|range| range.raw_target.clone());

    Some(AiWorkspaceTextHit {
        surface_id: line.surface_id.clone(),
        index,
        link_target,
        selection_surfaces,
    })
}

fn ai_workspace_title_surface_id(block_id: &str) -> String {
    format!("ai-workspace:{block_id}:title")
}

fn ai_workspace_preview_surface_id(block_id: &str) -> String {
    format!("ai-workspace:{block_id}:preview")
}

fn ai_workspace_column_byte_offsets(text: &str) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(text.chars().count() + 1);
    offsets.push(0);
    for (byte_index, ch) in text.char_indices() {
        offsets.push(byte_index + ch.len_utf8());
    }
    offsets
}

fn ai_workspace_link_ranges(text: &str, workspace_root: Option<&Path>) -> Vec<MarkdownLinkRange> {
    let mut link_ranges = Vec::new();
    let mut segment_start = None;

    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = segment_start.take() {
                ai_workspace_push_link_range(&mut link_ranges, text, start, index, workspace_root);
            }
            continue;
        }

        if segment_start.is_none() {
            segment_start = Some(index);
        }
    }

    if let Some(start) = segment_start {
        ai_workspace_push_link_range(&mut link_ranges, text, start, text.len(), workspace_root);
    }

    link_ranges
}

fn ai_workspace_push_link_range(
    link_ranges: &mut Vec<MarkdownLinkRange>,
    text: &str,
    start: usize,
    end: usize,
    workspace_root: Option<&Path>,
) {
    let Some((range, raw_target)) =
        ai_workspace_normalize_link_candidate(text, start..end, workspace_root)
    else {
        return;
    };

    if let Some(previous) = link_ranges.last_mut()
        && previous.raw_target == raw_target
        && previous.range.end == range.start
    {
        previous.range.end = range.end;
        return;
    }

    link_ranges.push(MarkdownLinkRange { range, raw_target });
}

fn ai_workspace_normalize_link_candidate(
    text: &str,
    mut range: Range<usize>,
    workspace_root: Option<&Path>,
) -> Option<(Range<usize>, String)> {
    let trimmed_start = text[range.clone()]
        .find(|ch: char| !matches!(ch, '(' | '[' | '{' | '<' | '"' | '\''))
        .map(|offset| range.start + offset)?;
    range.start = trimmed_start;

    let trimmed_slice = &text[range.clone()];
    let trimmed_end = trimmed_slice
        .trim_end_matches(|ch: char| {
            matches!(ch, '.' | ',' | ';' | ')' | ']' | '}' | '>' | '"' | '\'')
        })
        .len();
    range.end = range.start + trimmed_end;
    if range.is_empty() {
        return None;
    }

    let raw_target = text[range.clone()].to_string();
    resolve_markdown_link_target(raw_target.as_str(), workspace_root, None)
        .map(|_| (range, raw_target))
}

include!("ai_workspace_render_paint.rs");
