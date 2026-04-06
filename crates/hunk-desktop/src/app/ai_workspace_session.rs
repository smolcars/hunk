use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::markdown_links::MarkdownLinkRange;
use crate::app::markdown_links::markdown_inline_text_and_link_ranges;

pub(crate) const AI_WORKSPACE_SURFACE_BLOCK_GAP_PX: usize = 12;
pub(crate) const AI_WORKSPACE_SURFACE_BLOCK_SIDE_PADDING_PX: usize = 16;
pub(crate) const AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX: usize = 16;
pub(crate) const AI_WORKSPACE_SURFACE_BLOCK_BOTTOM_PADDING_PX: usize = 16;
pub(crate) const AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX: usize = 10;
pub(crate) const AI_WORKSPACE_BLOCK_CONTENT_BOTTOM_PADDING_PX: usize = 10;
pub(crate) const AI_WORKSPACE_BLOCK_TEXT_SIDE_PADDING_PX: usize = 14;
pub(crate) const AI_WORKSPACE_BLOCK_SECTION_GAP_PX: usize = 8;
pub(crate) const AI_WORKSPACE_BLOCK_MIN_WIDTH_PX: usize = 200;
pub(crate) const AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX: usize = 16;
pub(crate) const AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX: usize = 18;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiWorkspaceBlockRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiWorkspaceBlockKind {
    Message,
    Group,
    DiffSummary,
    Plan,
    Tool,
    Status,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceBlock {
    pub(crate) id: String,
    pub(crate) source_row_id: String,
    pub(crate) role: AiWorkspaceBlockRole,
    pub(crate) kind: AiWorkspaceBlockKind,
    pub(crate) expandable: bool,
    pub(crate) expanded: bool,
    pub(crate) title: String,
    pub(crate) preview: String,
    pub(crate) last_sequence: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceSourceRow {
    pub(crate) row_id: String,
    pub(crate) last_sequence: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiWorkspaceSelectionRegion {
    Block,
    Title,
    Preview,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceSelection {
    pub(crate) block_id: String,
    pub(crate) block_kind: AiWorkspaceBlockKind,
    pub(crate) line_index: Option<usize>,
    pub(crate) region: AiWorkspaceSelectionRegion,
}
impl AiWorkspaceSelection {
    pub(crate) fn matches_block(&self, block_id: &str) -> bool {
        self.block_id == block_id
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceBlockGeometry {
    pub(crate) block_id: String,
    pub(crate) top_px: usize,
    pub(crate) height_px: usize,
}
impl AiWorkspaceBlockGeometry {
    pub(crate) fn bottom_px(&self) -> usize {
        self.top_px.saturating_add(self.height_px)
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceDisplayGeometry {
    pub(crate) total_surface_height_px: usize,
    pub(crate) blocks: Vec<AiWorkspaceBlockGeometry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceViewportBlock {
    pub(crate) block: AiWorkspaceBlock,
    pub(crate) top_px: usize,
    pub(crate) height_px: usize,
    pub(crate) text_layout: AiWorkspaceBlockTextLayout,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceViewportSnapshot {
    pub(crate) total_surface_height_px: usize,
    pub(crate) visible_pixel_range: Option<Range<usize>>,
    pub(crate) visible_blocks: Vec<AiWorkspaceViewportBlock>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceSurfaceSnapshot {
    pub(crate) scroll_top_px: usize,
    pub(crate) viewport_height_px: usize,
    pub(crate) viewport: AiWorkspaceViewportSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceSurfaceSnapshotResult {
    pub(crate) snapshot: AiWorkspaceSurfaceSnapshot,
    pub(crate) geometry_rebuild_duration: Option<Duration>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceBlockTextLayout {
    pub(crate) block_width_px: usize,
    pub(crate) title_lines: Vec<String>,
    pub(crate) preview_lines: Vec<String>,
    pub(crate) preview_line_kinds: Vec<AiWorkspacePreviewLineKind>,
    pub(crate) preview_line_link_ranges: Vec<Vec<MarkdownLinkRange>>,
    pub(crate) preview_line_style_spans: Vec<Vec<AiWorkspacePreviewStyleSpan>>,
    pub(crate) preview_line_syntax_spans: Vec<Vec<AiWorkspacePreviewSyntaxSpan>>,
    pub(crate) height_px: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AiWorkspacePreviewLineKind {
    #[default]
    Normal,
    Heading,
    Quote,
    Code,
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspacePreviewStyleSpan {
    pub(crate) range: Range<usize>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strikethrough: bool,
    pub(crate) code: bool,
    pub(crate) link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspacePreviewSyntaxSpan {
    pub(crate) range: Range<usize>,
    pub(crate) token: hunk_domain::markdown_preview::MarkdownCodeTokenKind,
}
type AiWorkspacePreviewProjection = (
    Vec<String>,
    Vec<AiWorkspacePreviewLineKind>,
    Vec<Vec<MarkdownLinkRange>>,
    Vec<Vec<AiWorkspacePreviewStyleSpan>>,
    Vec<Vec<AiWorkspacePreviewSyntaxSpan>>,
);
type AiWorkspaceStructuredPreviewLine = (
    String,
    AiWorkspacePreviewLineKind,
    Vec<MarkdownLinkRange>,
    Vec<AiWorkspacePreviewStyleSpan>,
    Vec<AiWorkspacePreviewSyntaxSpan>,
);

#[derive(Debug, Clone)]
pub(crate) struct AiWorkspaceSession {
    thread_id: String,
    source_rows: Arc<[AiWorkspaceSourceRow]>,
    blocks: Vec<AiWorkspaceBlock>,
    geometry_by_width_bucket: BTreeMap<usize, AiWorkspaceDisplayGeometry>,
}

impl AiWorkspaceSession {
    pub(crate) fn new(
        thread_id: impl Into<String>,
        source_rows: Arc<[AiWorkspaceSourceRow]>,
        blocks: Vec<AiWorkspaceBlock>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            source_rows,
            blocks,
            geometry_by_width_bucket: BTreeMap::new(),
        }
    }

    pub(crate) fn matches_source(
        &self,
        thread_id: &str,
        source_rows: &[AiWorkspaceSourceRow],
    ) -> bool {
        self.thread_id == thread_id && self.source_rows.as_ref() == source_rows
    }

    pub(crate) fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub(crate) fn block(&self, block_id: &str) -> Option<&AiWorkspaceBlock> {
        self.blocks.iter().find(|block| block.id == block_id)
    }

    pub(crate) fn block_at(&self, index: usize) -> Option<&AiWorkspaceBlock> {
        self.blocks.get(index)
    }

    pub(crate) fn block_index(&self, block_id: &str) -> Option<usize> {
        self.blocks.iter().position(|block| block.id == block_id)
    }

    pub(crate) fn block_geometry(
        &mut self,
        block_id: &str,
        width_px: usize,
    ) -> Option<AiWorkspaceBlockGeometry> {
        let width_bucket = ai_workspace_width_bucket(width_px);
        let geometry = self
            .geometry_by_width_bucket
            .entry(width_bucket)
            .or_insert_with(|| build_ai_workspace_geometry(self.blocks.as_slice(), width_bucket));
        geometry
            .blocks
            .iter()
            .find(|entry| entry.block_id == block_id)
            .cloned()
    }

    pub(crate) fn surface_snapshot(
        &mut self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        width_px: usize,
    ) -> AiWorkspaceSurfaceSnapshot {
        self.surface_snapshot_with_stats(scroll_top_px, viewport_height_px, width_px)
            .snapshot
    }

    pub(crate) fn surface_snapshot_with_stats(
        &mut self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        width_px: usize,
    ) -> AiWorkspaceSurfaceSnapshotResult {
        let width_bucket = ai_workspace_width_bucket(width_px);
        let geometry_rebuild_started_at =
            (!self.geometry_by_width_bucket.contains_key(&width_bucket)).then(Instant::now);
        let geometry = self
            .geometry_by_width_bucket
            .entry(width_bucket)
            .or_insert_with(|| build_ai_workspace_geometry(self.blocks.as_slice(), width_bucket))
            .clone();
        let geometry_rebuild_duration =
            geometry_rebuild_started_at.map(|started_at| started_at.elapsed());
        let viewport_end_px = scroll_top_px.saturating_add(viewport_height_px);
        let visible_blocks = geometry
            .blocks
            .iter()
            .filter_map(|entry| {
                if entry.bottom_px() <= scroll_top_px || entry.top_px >= viewport_end_px {
                    return None;
                }

                self.blocks
                    .iter()
                    .find(|block| block.id == entry.block_id)
                    .cloned()
                    .map(|block| {
                        let text_layout = ai_workspace_text_layout_for_block(&block, width_bucket);
                        debug_assert_eq!(text_layout.height_px, entry.height_px);
                        AiWorkspaceViewportBlock {
                            block,
                            top_px: entry.top_px,
                            height_px: entry.height_px,
                            text_layout,
                        }
                    })
            })
            .collect::<Vec<_>>();

        AiWorkspaceSurfaceSnapshotResult {
            snapshot: AiWorkspaceSurfaceSnapshot {
                scroll_top_px,
                viewport_height_px,
                viewport: AiWorkspaceViewportSnapshot {
                    total_surface_height_px: geometry.total_surface_height_px,
                    visible_pixel_range: (!visible_blocks.is_empty()).then_some(
                        scroll_top_px..viewport_end_px.min(geometry.total_surface_height_px),
                    ),
                    visible_blocks,
                },
            },
            geometry_rebuild_duration,
        }
    }
}

pub(crate) fn ai_workspace_text_layout_for_block(
    block: &AiWorkspaceBlock,
    surface_width_px: usize,
) -> AiWorkspaceBlockTextLayout {
    let block_width_px = ai_workspace_block_width_px(surface_width_px, block.kind, block.role);
    let text_width_px = ai_workspace_block_text_width_px(block_width_px);
    let title_lines = ai_workspace_wrap_text(
        block.title.as_str(),
        ai_workspace_chars_per_line(text_width_px, true, false),
        2,
    );
    let (
        preview_lines,
        preview_line_kinds,
        preview_line_link_ranges,
        preview_line_style_spans,
        preview_line_syntax_spans,
    ) = if block.kind == AiWorkspaceBlockKind::Message {
        ai_workspace_message_preview_lines(block.preview.as_str(), text_width_px, block)
    } else {
        let preview_lines = ai_workspace_wrap_text(
            block.preview.as_str(),
            ai_workspace_chars_per_line(text_width_px, false, false),
            ai_workspace_preview_line_limit(block),
        );
        let preview_line_style_spans = if block.kind == AiWorkspaceBlockKind::Plan {
            preview_lines
                .iter()
                .map(|line| match line.as_bytes().get(..4) {
                    Some(b"[x] ") => vec![AiWorkspacePreviewStyleSpan {
                        range: 4..line.len(),
                        bold: false,
                        italic: false,
                        strikethrough: true,
                        code: false,
                        link: false,
                    }],
                    Some(b"[>] ") | Some(b"[ ] ") => Vec::new(),
                    _ => vec![AiWorkspacePreviewStyleSpan {
                        range: 0..line.len(),
                        bold: false,
                        italic: true,
                        strikethrough: false,
                        code: false,
                        link: false,
                    }],
                })
                .collect()
        } else {
            Vec::new()
        };
        (
            preview_lines.clone(),
            vec![AiWorkspacePreviewLineKind::Normal; preview_lines.len()],
            Vec::new(),
            preview_line_style_spans,
            Vec::new(),
        )
    };
    let title_height_px = title_lines.len() * AI_WORKSPACE_BLOCK_TITLE_LINE_HEIGHT_PX;
    let preview_height_px = preview_lines.len() * AI_WORKSPACE_BLOCK_PREVIEW_LINE_HEIGHT_PX;
    let preview_gap_px = if preview_lines.is_empty() {
        0
    } else {
        AI_WORKSPACE_BLOCK_SECTION_GAP_PX
    };

    AiWorkspaceBlockTextLayout {
        block_width_px,
        title_lines,
        preview_lines,
        preview_line_kinds,
        preview_line_link_ranges,
        preview_line_style_spans,
        preview_line_syntax_spans,
        height_px: AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX
            + title_height_px
            + preview_gap_px
            + preview_height_px
            + AI_WORKSPACE_BLOCK_CONTENT_BOTTOM_PADDING_PX,
    }
}

fn build_ai_workspace_geometry(
    blocks: &[AiWorkspaceBlock],
    surface_width_px: usize,
) -> AiWorkspaceDisplayGeometry {
    let mut top_px = AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX;
    let mut geometry_blocks = Vec::with_capacity(blocks.len());

    for block in blocks {
        let height_px = ai_workspace_text_layout_for_block(block, surface_width_px).height_px;
        geometry_blocks.push(AiWorkspaceBlockGeometry {
            block_id: block.id.clone(),
            top_px,
            height_px,
        });
        top_px = top_px
            .saturating_add(height_px)
            .saturating_add(AI_WORKSPACE_SURFACE_BLOCK_GAP_PX);
    }

    let total_surface_height_px = if geometry_blocks.is_empty() {
        AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX + AI_WORKSPACE_SURFACE_BLOCK_BOTTOM_PADDING_PX
    } else {
        top_px
            .saturating_sub(AI_WORKSPACE_SURFACE_BLOCK_GAP_PX)
            .saturating_add(AI_WORKSPACE_SURFACE_BLOCK_BOTTOM_PADDING_PX)
    };

    AiWorkspaceDisplayGeometry {
        total_surface_height_px,
        blocks: geometry_blocks,
    }
}

fn ai_workspace_block_width_px(
    surface_width_px: usize,
    kind: AiWorkspaceBlockKind,
    role: AiWorkspaceBlockRole,
) -> usize {
    let available_width_px = surface_width_px
        .saturating_sub(AI_WORKSPACE_SURFACE_BLOCK_SIDE_PADDING_PX * 2)
        .max(180);
    if available_width_px <= AI_WORKSPACE_BLOCK_MIN_WIDTH_PX {
        return available_width_px;
    }
    let desired_width_px = match (kind, role) {
        (AiWorkspaceBlockKind::Message, AiWorkspaceBlockRole::User) => available_width_px.min(680),
        (AiWorkspaceBlockKind::Message, AiWorkspaceBlockRole::Assistant) => {
            available_width_px.min(700)
        }
        (AiWorkspaceBlockKind::Plan, _) => available_width_px.min(700),
        (AiWorkspaceBlockKind::DiffSummary, _) => available_width_px.min(940),
        (AiWorkspaceBlockKind::Tool | AiWorkspaceBlockKind::Group, _) => {
            available_width_px.min(940)
        }
        (_, AiWorkspaceBlockRole::Tool) => available_width_px.min(860),
        (_, AiWorkspaceBlockRole::System) => available_width_px.min(640),
        (_, AiWorkspaceBlockRole::Assistant) => available_width_px.min(700),
        (_, AiWorkspaceBlockRole::User) => available_width_px.min(680),
    };
    desired_width_px.clamp(AI_WORKSPACE_BLOCK_MIN_WIDTH_PX, available_width_px)
}

pub(crate) fn ai_workspace_block_text_width_px(block_width_px: usize) -> usize {
    block_width_px
        .saturating_sub(AI_WORKSPACE_BLOCK_TEXT_SIDE_PADDING_PX * 2)
        .max(120)
}

pub(crate) fn ai_workspace_chars_per_line(
    text_width_px: usize,
    title: bool,
    monospace: bool,
) -> usize {
    let char_width_px = if monospace {
        7.2
    } else if title {
        7.0
    } else {
        6.6
    };
    ((text_width_px as f32) / char_width_px).floor() as usize
}

fn ai_workspace_preview_line_limit(block: &AiWorkspaceBlock) -> usize {
    match block.kind {
        AiWorkspaceBlockKind::Message => 96,
        AiWorkspaceBlockKind::Plan => 32,
        AiWorkspaceBlockKind::DiffSummary => 5,
        AiWorkspaceBlockKind::Group => 4,
        AiWorkspaceBlockKind::Tool | AiWorkspaceBlockKind::Status => {
            if block.expanded {
                48
            } else {
                4
            }
        }
    }
}

fn ai_workspace_width_bucket(width_px: usize) -> usize {
    const AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX: usize = 40;

    let clamped = width_px.max(AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX);
    (clamped / AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX) * AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX
}

fn ai_workspace_message_preview_lines(
    markdown: &str,
    text_width_px: usize,
    block: &AiWorkspaceBlock,
) -> AiWorkspacePreviewProjection {
    let max_lines = ai_workspace_preview_line_limit(block);
    if markdown.trim().is_empty() {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }

    let blocks = hunk_domain::markdown_preview::parse_markdown_preview(markdown);
    if blocks.is_empty() {
        let lines = ai_workspace_wrap_text(
            markdown,
            ai_workspace_chars_per_line(text_width_px, false, false),
            max_lines,
        );
        return (
            lines.clone(),
            vec![AiWorkspacePreviewLineKind::Normal; lines.len()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
    }

    let mut structured_lines = Vec::<AiWorkspaceStructuredPreviewLine>::new();
    for (block_index, markdown_block) in blocks.into_iter().enumerate() {
        if block_index > 0 {
            structured_lines.push((
                String::new(),
                AiWorkspacePreviewLineKind::Normal,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ));
        }
        match markdown_block {
            hunk_domain::markdown_preview::MarkdownPreviewBlock::Heading { spans, .. } => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    text,
                    AiWorkspacePreviewLineKind::Heading,
                    link_ranges,
                    style_spans,
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::Paragraph(spans) => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    text,
                    AiWorkspacePreviewLineKind::Normal,
                    link_ranges,
                    style_spans,
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::UnorderedListItem(spans) => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    format!("- {text}"),
                    AiWorkspacePreviewLineKind::Normal,
                    ai_workspace_offset_link_ranges(link_ranges, 2),
                    ai_workspace_offset_style_spans(style_spans, 2),
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::OrderedListItem {
                number,
                spans,
            } => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                let prefix = format!("{number}. ");
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    format!("{prefix}{text}"),
                    AiWorkspacePreviewLineKind::Normal,
                    ai_workspace_offset_link_ranges(link_ranges, prefix.len()),
                    ai_workspace_offset_style_spans(style_spans, prefix.len()),
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::BlockQuote(spans) => {
                let (text, link_ranges, style_spans) =
                    ai_workspace_markdown_inline_text_and_styles(spans.as_slice());
                ai_workspace_push_markdown_block_line(
                    &mut structured_lines,
                    format!("| {text}"),
                    AiWorkspacePreviewLineKind::Quote,
                    ai_workspace_offset_link_ranges(link_ranges, 2),
                    ai_workspace_offset_style_spans(style_spans, 2),
                    Vec::new(),
                );
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::CodeBlock { language, lines } => {
                if let Some(language) = language.filter(|value| !value.trim().is_empty()) {
                    ai_workspace_push_markdown_block_line(
                        &mut structured_lines,
                        language,
                        AiWorkspacePreviewLineKind::Quote,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                }
                for line in lines {
                    let (text, syntax_spans) =
                        ai_workspace_markdown_code_line_text_and_spans(&line);
                    structured_lines.push((
                        text,
                        AiWorkspacePreviewLineKind::Code,
                        Vec::new(),
                        Vec::new(),
                        syntax_spans,
                    ));
                }
            }
            hunk_domain::markdown_preview::MarkdownPreviewBlock::ThematicBreak => {
                structured_lines.push((
                    "----".to_string(),
                    AiWorkspacePreviewLineKind::Rule,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                ));
            }
        }
    }

    ai_workspace_wrap_structured_preview_lines(structured_lines, text_width_px, max_lines)
}

fn ai_workspace_push_markdown_block_line(
    structured_lines: &mut Vec<AiWorkspaceStructuredPreviewLine>,
    text: String,
    kind: AiWorkspacePreviewLineKind,
    link_ranges: Vec<MarkdownLinkRange>,
    style_spans: Vec<AiWorkspacePreviewStyleSpan>,
    syntax_spans: Vec<AiWorkspacePreviewSyntaxSpan>,
) {
    if !text.trim().is_empty() {
        structured_lines.push((text, kind, link_ranges, style_spans, syntax_spans));
    }
}

fn ai_workspace_wrap_structured_preview_lines(
    structured_lines: Vec<AiWorkspaceStructuredPreviewLine>,
    text_width_px: usize,
    max_lines: usize,
) -> AiWorkspacePreviewProjection {
    if max_lines == 0 {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    }

    let mut wrapped_lines = Vec::new();
    let mut wrapped_kinds = Vec::new();
    let mut wrapped_link_ranges = Vec::new();
    let mut wrapped_style_spans = Vec::new();
    let mut wrapped_syntax_spans = Vec::new();

    let total_structured_lines = structured_lines.len();
    for (line_index, (line, kind, link_ranges, style_spans, syntax_spans)) in
        structured_lines.into_iter().enumerate()
    {
        let has_more_input = line_index + 1 < total_structured_lines;
        let max_chars_per_line = ai_workspace_chars_per_line(
            text_width_px,
            false,
            kind == AiWorkspacePreviewLineKind::Code,
        );
        let wrapped = ai_workspace_wrap_text_ranges(line.as_str(), max_chars_per_line, usize::MAX);
        let wrapped = if wrapped.is_empty() {
            std::iter::once(0..0).collect::<Vec<_>>()
        } else {
            wrapped
        };

        for (wrapped_index, wrapped_range) in wrapped.into_iter().enumerate() {
            wrapped_lines.push(line[wrapped_range.clone()].to_string());
            wrapped_kinds.push(kind);
            wrapped_link_ranges.push(ai_workspace_clip_link_ranges(
                link_ranges.as_slice(),
                wrapped_range.clone(),
            ));
            wrapped_style_spans.push(ai_workspace_clip_style_spans(
                style_spans.as_slice(),
                wrapped_range.clone(),
            ));
            wrapped_syntax_spans.push(ai_workspace_clip_syntax_spans(
                syntax_spans.as_slice(),
                wrapped_range,
            ));
            if wrapped_lines.len() == max_lines {
                if has_more_input || wrapped_index > 0 {
                    ai_workspace_append_ellipsis(wrapped_lines.last_mut());
                }
                return (
                    wrapped_lines,
                    wrapped_kinds,
                    wrapped_link_ranges,
                    wrapped_style_spans,
                    wrapped_syntax_spans,
                );
            }
        }
    }

    (
        wrapped_lines,
        wrapped_kinds,
        wrapped_link_ranges,
        wrapped_style_spans,
        wrapped_syntax_spans,
    )
}

fn ai_workspace_wrap_text(text: &str, max_chars_per_line: usize, max_lines: usize) -> Vec<String> {
    ai_workspace_wrap_text_ranges(text, max_chars_per_line, max_lines)
        .into_iter()
        .map(|range| text[range].to_string())
        .collect()
}

fn ai_workspace_wrap_text_ranges(
    text: &str,
    max_chars_per_line: usize,
    max_lines: usize,
) -> Vec<Range<usize>> {
    if max_lines == 0 {
        return Vec::new();
    }

    let max_chars_per_line = max_chars_per_line.max(8);
    let mut lines = Vec::new();

    let mut raw_lines = text.lines().peekable();
    let mut cursor = 0usize;
    while let Some(raw_line) = raw_lines.next() {
        let has_more_input = raw_lines.peek().is_some();
        let raw_line_start = cursor;
        let raw_line_end = raw_line_start + raw_line.len();
        cursor = raw_line_end.saturating_add(1);
        if raw_line.is_empty() {
            lines.push(raw_line_start..raw_line_start);
            if lines.len() == max_lines {
                if has_more_input {
                    ai_workspace_append_ellipsis_range(lines.last_mut(), text);
                }
                return lines;
            }
            continue;
        }

        let mut remaining_start = raw_line_start;
        let mut remaining = raw_line.trim_end_matches(['\r', ' ']);
        loop {
            if remaining.is_empty() {
                break;
            }

            let remaining_chars = remaining.chars().count();
            if remaining_chars <= max_chars_per_line {
                let trimmed_len = remaining.len();
                lines.push(remaining_start..remaining_start.saturating_add(trimmed_len));
                if lines.len() == max_lines {
                    if has_more_input {
                        ai_workspace_append_ellipsis_range(lines.last_mut(), text);
                    }
                    return lines;
                }
                break;
            }

            let split_index = ai_workspace_wrap_split_index(remaining, max_chars_per_line)
                .unwrap_or(remaining.len());
            let (chunk, rest) = remaining.split_at(split_index);
            let chunk = chunk.trim_end_matches([' ', '\t']);
            lines.push(if chunk.is_empty() {
                remaining_start..remaining_start.saturating_add(split_index)
            } else {
                remaining_start..remaining_start.saturating_add(chunk.len())
            });
            if lines.len() == max_lines {
                ai_workspace_append_ellipsis_range(lines.last_mut(), text);
                return lines;
            }
            remaining_start = remaining_start.saturating_add(split_index).saturating_add(
                rest.len()
                    .saturating_sub(rest.trim_start_matches([' ', '\t']).len()),
            );
            remaining = rest.trim_start_matches([' ', '\t']);
        }
    }

    lines
}

fn ai_workspace_wrap_split_index(text: &str, max_chars_per_line: usize) -> Option<usize> {
    let mut char_count = 0usize;
    let mut last_whitespace_break = None;

    for (byte_index, ch) in text.char_indices() {
        char_count = char_count.saturating_add(1);
        if ch.is_whitespace() {
            last_whitespace_break = Some(byte_index + ch.len_utf8());
        }
        if char_count >= max_chars_per_line {
            return last_whitespace_break.or(Some(byte_index + ch.len_utf8()));
        }
    }

    None
}

fn ai_workspace_append_ellipsis(line: Option<&mut String>) {
    let Some(line) = line else {
        return;
    };
    if !line.ends_with("...") {
        line.push_str("...");
    }
}

fn ai_workspace_append_ellipsis_range(line: Option<&mut Range<usize>>, text: &str) {
    let Some(line) = line else {
        return;
    };
    let mut end = line.end;
    while end > line.start && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    line.end = end;
}

fn ai_workspace_offset_link_ranges(
    link_ranges: Vec<MarkdownLinkRange>,
    offset: usize,
) -> Vec<MarkdownLinkRange> {
    link_ranges
        .into_iter()
        .map(|range| MarkdownLinkRange {
            range: (range.range.start + offset)..(range.range.end + offset),
            raw_target: range.raw_target,
        })
        .collect()
}

fn ai_workspace_offset_style_spans(
    style_spans: Vec<AiWorkspacePreviewStyleSpan>,
    offset: usize,
) -> Vec<AiWorkspacePreviewStyleSpan> {
    style_spans
        .into_iter()
        .map(|span| AiWorkspacePreviewStyleSpan {
            range: (span.range.start + offset)..(span.range.end + offset),
            ..span
        })
        .collect()
}

fn ai_workspace_markdown_inline_text_and_styles(
    spans: &[hunk_domain::markdown_preview::MarkdownInlineSpan],
) -> (
    String,
    Vec<MarkdownLinkRange>,
    Vec<AiWorkspacePreviewStyleSpan>,
) {
    let (text, link_ranges) = markdown_inline_text_and_link_ranges(spans);
    let mut style_spans = Vec::new();
    let mut cursor = 0usize;

    for span in spans {
        if span.style.hard_break {
            if !text[..cursor].ends_with('\n') {
                cursor += 1;
            }
            continue;
        }
        if span.text.is_empty() {
            continue;
        }

        let start = cursor;
        let end = start + span.text.len();
        cursor = end;
        if !(span.style.bold
            || span.style.italic
            || span.style.strikethrough
            || span.style.code
            || span.style.link.is_some())
        {
            continue;
        }

        style_spans.push(AiWorkspacePreviewStyleSpan {
            range: start..end,
            bold: span.style.bold,
            italic: span.style.italic,
            strikethrough: span.style.strikethrough,
            code: span.style.code,
            link: span.style.link.is_some(),
        });
    }

    (text, link_ranges, style_spans)
}

fn ai_workspace_markdown_code_line_text_and_spans(
    spans: &[hunk_domain::markdown_preview::MarkdownCodeSpan],
) -> (String, Vec<AiWorkspacePreviewSyntaxSpan>) {
    let mut text = String::new();
    let mut syntax_spans = Vec::new();
    let mut cursor = 0usize;

    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let start = cursor;
        text.push_str(span.text.as_str());
        cursor += span.text.len();
        syntax_spans.push(AiWorkspacePreviewSyntaxSpan {
            range: start..cursor,
            token: span.token,
        });
    }

    (text, syntax_spans)
}

fn ai_workspace_clip_link_ranges(
    link_ranges: &[MarkdownLinkRange],
    visible_range: Range<usize>,
) -> Vec<MarkdownLinkRange> {
    link_ranges
        .iter()
        .filter_map(|range| {
            let start = range.range.start.max(visible_range.start);
            let end = range.range.end.min(visible_range.end);
            (start < end).then(|| MarkdownLinkRange {
                range: (start - visible_range.start)..(end - visible_range.start),
                raw_target: range.raw_target.clone(),
            })
        })
        .collect()
}

fn ai_workspace_clip_style_spans(
    style_spans: &[AiWorkspacePreviewStyleSpan],
    visible_range: Range<usize>,
) -> Vec<AiWorkspacePreviewStyleSpan> {
    style_spans
        .iter()
        .filter_map(|span| {
            let start = span.range.start.max(visible_range.start);
            let end = span.range.end.min(visible_range.end);
            (start < end).then(|| AiWorkspacePreviewStyleSpan {
                range: (start - visible_range.start)..(end - visible_range.start),
                bold: span.bold,
                italic: span.italic,
                strikethrough: span.strikethrough,
                code: span.code,
                link: span.link,
            })
        })
        .collect()
}

fn ai_workspace_clip_syntax_spans(
    syntax_spans: &[AiWorkspacePreviewSyntaxSpan],
    visible_range: Range<usize>,
) -> Vec<AiWorkspacePreviewSyntaxSpan> {
    syntax_spans
        .iter()
        .filter_map(|span| {
            let start = span.range.start.max(visible_range.start);
            let end = span.range.end.min(visible_range.end);
            (start < end).then(|| AiWorkspacePreviewSyntaxSpan {
                range: (start - visible_range.start)..(end - visible_range.start),
                token: span.token,
            })
        })
        .collect()
}
