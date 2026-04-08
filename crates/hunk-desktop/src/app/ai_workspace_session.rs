use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::AiTextSelectionSurfaceSpec;
use crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffOptions;
use crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffPresentationPolicy;
use crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffProjection;
use crate::app::ai_workspace_inline_diff::ai_workspace_inline_diff_presentation_policy;
use crate::app::ai_workspace_inline_diff::ai_workspace_project_inline_diff;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AiWorkspaceBlockRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AiWorkspaceBlockKind {
    Message,
    Group,
    DiffSummary,
    Plan,
    Tool,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AiWorkspaceBlockActionArea {
    Header,
    Preview,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AiWorkspaceBlock {
    pub(crate) id: String,
    pub(crate) source_row_id: String,
    pub(crate) role: AiWorkspaceBlockRole,
    pub(crate) kind: AiWorkspaceBlockKind,
    pub(crate) nested: bool,
    pub(crate) mono_preview: bool,
    pub(crate) open_review_tab: bool,
    pub(crate) expandable: bool,
    pub(crate) expanded: bool,
    pub(crate) title: String,
    pub(crate) preview: String,
    pub(crate) action_area: AiWorkspaceBlockActionArea,
    pub(crate) copy_text: Option<String>,
    pub(crate) copy_tooltip: Option<&'static str>,
    pub(crate) copy_success_message: Option<&'static str>,
    pub(crate) run_in_terminal_command: Option<String>,
    pub(crate) run_in_terminal_cwd: Option<PathBuf>,
    pub(crate) status_label: Option<String>,
    pub(crate) status_color_role: Option<AiWorkspacePreviewColorRole>,
    pub(crate) inline_diff_source: Option<Arc<str>>,
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
    pub(crate) block_index: usize,
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
    pub(crate) selection_scope_id: String,
    pub(crate) selection_surfaces: Arc<[AiTextSelectionSurfaceSpec]>,
    pub(crate) scroll_top_px: usize,
    pub(crate) viewport_height_px: usize,
    pub(crate) viewport: AiWorkspaceViewportSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceSurfaceSnapshotResult {
    pub(crate) snapshot: AiWorkspaceSurfaceSnapshot,
    pub(crate) geometry_rebuild_duration: Option<Duration>,
    pub(crate) text_layout_build_duration: Option<Duration>,
    pub(crate) text_layout_build_count: u32,
    pub(crate) text_layout_cache_hits: u32,
    pub(crate) inline_diff_projection_build_duration: Option<Duration>,
    pub(crate) inline_diff_projection_build_count: u32,
    pub(crate) inline_diff_projection_cache_hits: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AiWorkspaceBuildStats {
    geometry_rebuild_duration: Option<Duration>,
    text_layout_build_duration: Duration,
    text_layout_build_count: u32,
    text_layout_cache_hits: u32,
    inline_diff_projection_build_duration: Duration,
    inline_diff_projection_build_count: u32,
    inline_diff_projection_cache_hits: u32,
}

impl AiWorkspaceBuildStats {
    fn record_text_layout_build(&mut self, duration: Duration) {
        self.text_layout_build_duration = self.text_layout_build_duration.saturating_add(duration);
        self.text_layout_build_count = self.text_layout_build_count.saturating_add(1);
    }

    fn record_text_layout_cache_hit(&mut self) {
        self.text_layout_cache_hits = self.text_layout_cache_hits.saturating_add(1);
    }

    fn record_inline_diff_projection_build(&mut self, duration: Duration) {
        self.inline_diff_projection_build_duration = self
            .inline_diff_projection_build_duration
            .saturating_add(duration);
        self.inline_diff_projection_build_count =
            self.inline_diff_projection_build_count.saturating_add(1);
    }

    fn record_inline_diff_projection_cache_hit(&mut self) {
        self.inline_diff_projection_cache_hits =
            self.inline_diff_projection_cache_hits.saturating_add(1);
    }

    fn into_snapshot_result(
        self,
        snapshot: AiWorkspaceSurfaceSnapshot,
    ) -> AiWorkspaceSurfaceSnapshotResult {
        AiWorkspaceSurfaceSnapshotResult {
            snapshot,
            geometry_rebuild_duration: self.geometry_rebuild_duration,
            text_layout_build_duration: (self.text_layout_build_count > 0)
                .then_some(self.text_layout_build_duration),
            text_layout_build_count: self.text_layout_build_count,
            text_layout_cache_hits: self.text_layout_cache_hits,
            inline_diff_projection_build_duration: (self.inline_diff_projection_build_count > 0)
                .then_some(self.inline_diff_projection_build_duration),
            inline_diff_projection_build_count: self.inline_diff_projection_build_count,
            inline_diff_projection_cache_hits: self.inline_diff_projection_cache_hits,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceInlineDiffState {
    pub(crate) projection: Arc<AiWorkspaceInlineDiffProjection>,
    pub(crate) presentation_policy: AiWorkspaceInlineDiffPresentationPolicy,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceBlockTextLayout {
    pub(crate) block_width_px: usize,
    pub(crate) title_lines: Vec<String>,
    pub(crate) title_line_style_spans: Vec<Vec<AiWorkspacePreviewStyleSpan>>,
    pub(crate) preview_lines: Vec<String>,
    pub(crate) preview_line_kinds: Vec<AiWorkspacePreviewLineKind>,
    pub(crate) preview_line_hit_targets: Vec<Option<AiWorkspacePreviewHitTarget>>,
    pub(crate) preview_line_link_ranges: Vec<Vec<MarkdownLinkRange>>,
    pub(crate) preview_line_style_spans: Vec<Vec<AiWorkspacePreviewStyleSpan>>,
    pub(crate) preview_line_syntax_spans: Vec<Vec<AiWorkspacePreviewSyntaxSpan>>,
    pub(crate) preview_copy_regions: Vec<AiWorkspaceCopyRegion>,
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
    DiffFileHeader,
    DiffHunkHeader,
    DiffContext,
    DiffAdded,
    DiffRemoved,
    DiffMeta,
}

impl AiWorkspacePreviewLineKind {
    pub(crate) fn is_monospace(self) -> bool {
        matches!(
            self,
            Self::Code
                | Self::DiffHunkHeader
                | Self::DiffContext
                | Self::DiffAdded
                | Self::DiffRemoved
                | Self::DiffMeta
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AiWorkspacePreviewColorRole {
    Accent,
    Added,
    Removed,
    Foreground,
    Muted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspacePreviewStyleSpan {
    pub(crate) range: Range<usize>,
    pub(crate) color_role: Option<AiWorkspacePreviewColorRole>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strikethrough: bool,
    pub(crate) code: bool,
    pub(crate) link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceCopyRegion {
    pub(crate) line_range: Range<usize>,
    pub(crate) text: String,
    pub(crate) tooltip: &'static str,
    pub(crate) success_message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspacePreviewSyntaxSpan {
    pub(crate) range: Range<usize>,
    pub(crate) token: hunk_domain::markdown_preview::MarkdownCodeTokenKind,
}
type AiWorkspacePreviewProjection = (
    Vec<String>,
    Vec<AiWorkspacePreviewLineKind>,
    Vec<Option<AiWorkspacePreviewHitTarget>>,
    Vec<Vec<MarkdownLinkRange>>,
    Vec<Vec<AiWorkspacePreviewStyleSpan>>,
    Vec<Vec<AiWorkspacePreviewSyntaxSpan>>,
    Vec<AiWorkspaceCopyRegion>,
);
type AiWorkspaceStructuredPreviewLine = (
    String,
    AiWorkspacePreviewLineKind,
    Option<AiWorkspacePreviewHitTarget>,
    Vec<MarkdownLinkRange>,
    Vec<AiWorkspacePreviewStyleSpan>,
    Vec<AiWorkspacePreviewSyntaxSpan>,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiWorkspacePreviewHitTarget {
    InlineDiff(AiWorkspaceInlineDiffHitTarget),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiWorkspaceInlineDiffHitTarget {
    FileHeader {
        file_index: usize,
    },
    HunkHeader {
        file_index: usize,
        hunk_index: usize,
    },
    Line {
        file_index: usize,
        hunk_index: usize,
        line_index: usize,
        kind: crate::app::ai_workspace_inline_diff::AiWorkspaceInlineDiffLineKind,
    },
    Meta {
        file_index: usize,
        hunk_index: Option<usize>,
        meta_index: usize,
    },
    TruncationNotice,
}

#[derive(Debug, Clone)]
pub(crate) struct AiWorkspaceSession {
    thread_id: String,
    source_rows: Arc<[AiWorkspaceSourceRow]>,
    blocks: Vec<AiWorkspaceBlock>,
    selection_scope_id: String,
    geometry_by_width_bucket: BTreeMap<usize, AiWorkspaceDisplayGeometry>,
    selection_surfaces_by_width_bucket: BTreeMap<usize, Arc<[AiTextSelectionSurfaceSpec]>>,
    text_layouts_by_width_bucket:
        BTreeMap<usize, BTreeMap<(String, u64), AiWorkspaceBlockTextLayout>>,
    inline_diffs_by_width_bucket:
        BTreeMap<usize, BTreeMap<(String, u64), Arc<AiWorkspaceInlineDiffState>>>,
}

impl AiWorkspaceSession {
    pub(crate) fn new(
        thread_id: impl Into<String>,
        source_rows: Arc<[AiWorkspaceSourceRow]>,
        blocks: Vec<AiWorkspaceBlock>,
    ) -> Self {
        let thread_id = thread_id.into();
        let selection_scope_id = format!("ai-workspace-thread:{thread_id}");
        Self {
            thread_id,
            source_rows,
            blocks,
            selection_scope_id,
            geometry_by_width_bucket: BTreeMap::new(),
            selection_surfaces_by_width_bucket: BTreeMap::new(),
            text_layouts_by_width_bucket: BTreeMap::new(),
            inline_diffs_by_width_bucket: BTreeMap::new(),
        }
    }

    pub(crate) fn matches_source(
        &self,
        thread_id: &str,
        source_rows: &[AiWorkspaceSourceRow],
    ) -> bool {
        self.thread_id == thread_id && self.source_rows.as_ref() == source_rows
    }

    #[allow(dead_code)]
    pub(crate) fn belongs_to_thread(&self, thread_id: &str) -> bool {
        self.thread_id == thread_id
    }

    pub(crate) fn update_source(
        &mut self,
        thread_id: impl Into<String>,
        source_rows: Arc<[AiWorkspaceSourceRow]>,
        blocks: Vec<AiWorkspaceBlock>,
    ) {
        let thread_id = thread_id.into();
        if self.thread_id != thread_id {
            *self = Self::new(thread_id, source_rows, blocks);
            return;
        }

        let retained_layout_keys = blocks
            .iter()
            .map(ai_workspace_block_layout_cache_key)
            .collect::<std::collections::BTreeSet<_>>();
        let retained_inline_diff_keys = blocks
            .iter()
            .filter_map(|block| {
                block
                    .inline_diff_source
                    .as_ref()
                    .map(|_| (block.source_row_id.clone(), block.last_sequence))
            })
            .collect::<std::collections::BTreeSet<_>>();

        self.source_rows = source_rows;
        self.blocks = blocks;
        self.geometry_by_width_bucket.clear();
        self.selection_surfaces_by_width_bucket.clear();
        self.text_layouts_by_width_bucket.retain(|_, layouts| {
            layouts.retain(|key, _| retained_layout_keys.contains(key));
            !layouts.is_empty()
        });
        self.inline_diffs_by_width_bucket.retain(|_, inline_diffs| {
            inline_diffs.retain(|key, _| retained_inline_diff_keys.contains(key));
            !inline_diffs.is_empty()
        });
    }

    pub(crate) fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub(crate) fn selection_scope_id(&self) -> &str {
        self.selection_scope_id.as_str()
    }

    pub(crate) fn selection_surfaces_for_width(
        &mut self,
        width_px: usize,
    ) -> Arc<[AiTextSelectionSurfaceSpec]> {
        let mut stats = AiWorkspaceBuildStats::default();
        self.selection_surfaces_for_width_with_stats(width_px, &mut stats)
    }

    fn selection_surfaces_for_width_with_stats(
        &mut self,
        width_px: usize,
        stats: &mut AiWorkspaceBuildStats,
    ) -> Arc<[AiTextSelectionSurfaceSpec]> {
        let width_bucket = ai_workspace_width_bucket(width_px);
        if let Some(selection_surfaces) = self.selection_surfaces_by_width_bucket.get(&width_bucket)
        {
            return selection_surfaces.clone();
        }

        let selection_surfaces = self.build_selection_surfaces_for_width(width_bucket, stats);
        self.selection_surfaces_by_width_bucket
            .insert(width_bucket, selection_surfaces.clone());
        selection_surfaces
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
        let block_index = self.block_index(block_id)?;
        let width_bucket = ai_workspace_width_bucket(width_px);
        if !self.geometry_by_width_bucket.contains_key(&width_bucket) {
            let geometry =
                self.build_geometry_for_width(width_bucket, &mut AiWorkspaceBuildStats::default());
            self.geometry_by_width_bucket.insert(width_bucket, geometry);
        }
        let geometry = self.geometry_by_width_bucket.get(&width_bucket)?;
        geometry.blocks.get(block_index).cloned()
    }

    #[allow(dead_code)]
    pub(crate) fn inline_diff_for_block(
        &mut self,
        block_id: &str,
        width_px: usize,
    ) -> Option<Arc<AiWorkspaceInlineDiffState>> {
        let mut stats = AiWorkspaceBuildStats::default();
        self.inline_diff_for_block_with_stats(block_id, width_px, &mut stats)
    }

    fn inline_diff_for_block_with_stats(
        &mut self,
        block_id: &str,
        width_px: usize,
        stats: &mut AiWorkspaceBuildStats,
    ) -> Option<Arc<AiWorkspaceInlineDiffState>> {
        let block = self.block(block_id)?.clone();
        let inline_diff_source = block.inline_diff_source.clone()?;
        let width_bucket = ai_workspace_width_bucket(width_px);
        let cache_key = (block.source_row_id.clone(), block.last_sequence);
        if let Some(cached) = self
            .inline_diffs_by_width_bucket
            .get(&width_bucket)
            .and_then(|entries| entries.get(&cache_key))
        {
            stats.record_inline_diff_projection_cache_hit();
            return Some(cached.clone());
        }

        let options = AiWorkspaceInlineDiffOptions::default();
        let build_started_at = Instant::now();
        let projection = Arc::new(ai_workspace_project_inline_diff(
            inline_diff_source.as_ref(),
            options,
        ));
        let inline_diff_state = Arc::new(AiWorkspaceInlineDiffState {
            presentation_policy: ai_workspace_inline_diff_presentation_policy(
                projection.as_ref(),
                options,
            ),
            projection,
        });
        self.inline_diffs_by_width_bucket
            .entry(width_bucket)
            .or_default()
            .insert(cache_key, inline_diff_state.clone());
        stats.record_inline_diff_projection_build(build_started_at.elapsed());
        Some(inline_diff_state)
    }

    pub(crate) fn surface_snapshot_with_stats(
        &mut self,
        scroll_top_px: usize,
        viewport_height_px: usize,
        width_px: usize,
    ) -> AiWorkspaceSurfaceSnapshotResult {
        let width_bucket = ai_workspace_width_bucket(width_px);
        let mut stats = AiWorkspaceBuildStats::default();
        let geometry_rebuild_started_at =
            (!self.geometry_by_width_bucket.contains_key(&width_bucket)).then(Instant::now);
        let selection_surfaces =
            self.selection_surfaces_for_width_with_stats(width_bucket, &mut stats);
        if !self.geometry_by_width_bucket.contains_key(&width_bucket) {
            let geometry = self.build_geometry_for_width(width_bucket, &mut stats);
            self.geometry_by_width_bucket.insert(width_bucket, geometry);
        }
        let geometry = self
            .geometry_by_width_bucket
            .get(&width_bucket)
            .expect("geometry should exist for width bucket");
        stats.geometry_rebuild_duration =
            geometry_rebuild_started_at.map(|started_at| started_at.elapsed());
        let viewport_end_px = scroll_top_px.saturating_add(viewport_height_px);
        let visible_entries = geometry
            .blocks
            .iter()
            .filter(|entry| {
                !(entry.bottom_px() <= scroll_top_px || entry.top_px >= viewport_end_px)
            })
            .map(|entry| (entry.block_index, entry.top_px, entry.height_px))
            .collect::<Vec<_>>();
        let total_surface_height_px = geometry.total_surface_height_px;
        let visible_blocks = visible_entries
            .into_iter()
            .filter_map(|(block_index, top_px, height_px)| {
                self.blocks.get(block_index).cloned().map(|block| {
                    let text_layout = self.text_layout_for_block(&block, width_bucket, &mut stats);
                    debug_assert_eq!(text_layout.height_px, height_px);
                    AiWorkspaceViewportBlock {
                        block,
                        top_px,
                        height_px,
                        text_layout,
                    }
                })
            })
            .collect::<Vec<_>>();

        stats.into_snapshot_result(AiWorkspaceSurfaceSnapshot {
            selection_scope_id: self.selection_scope_id.clone(),
            selection_surfaces,
            scroll_top_px,
            viewport_height_px,
            viewport: AiWorkspaceViewportSnapshot {
                total_surface_height_px,
                visible_pixel_range: (!visible_blocks.is_empty())
                    .then_some(scroll_top_px..viewport_end_px.min(total_surface_height_px)),
                visible_blocks,
            },
        })
    }

    fn build_selection_surfaces_for_width(
        &mut self,
        surface_width_px: usize,
        stats: &mut AiWorkspaceBuildStats,
    ) -> Arc<[AiTextSelectionSurfaceSpec]> {
        let mut surfaces = Vec::new();
        for block_index in 0..self.blocks.len() {
            let block_separator = (!surfaces.is_empty()).then_some("\n\n");
            let block = self.blocks[block_index].clone();
            let text_layout = self.text_layout_for_block(&block, surface_width_px, stats);
            let title_text = text_layout.title_lines.join("\n");
            let preview_text = text_layout.preview_lines.join("\n");
            let has_title = !title_text.is_empty();

            if has_title {
                let mut title_surface = AiTextSelectionSurfaceSpec::new(
                    format!("ai-workspace:{}:title", block.id),
                    title_text,
                )
                .with_row_id(block.source_row_id.clone());
                if let Some(separator) = block_separator {
                    title_surface = title_surface.with_separator_before(separator);
                }
                surfaces.push(title_surface);
            }

            if !preview_text.is_empty() {
                let mut preview_surface = AiTextSelectionSurfaceSpec::new(
                    format!("ai-workspace:{}:preview", block.id),
                    preview_text,
                )
                .with_row_id(block.source_row_id.clone());
                preview_surface = if !has_title {
                    if let Some(separator) = block_separator {
                        preview_surface.with_separator_before(separator)
                    } else {
                        preview_surface
                    }
                } else {
                    preview_surface.with_separator_before("\n")
                };
                surfaces.push(preview_surface);
            }
        }

        Arc::<[AiTextSelectionSurfaceSpec]>::from(surfaces)
    }

    fn build_geometry_for_width(
        &mut self,
        surface_width_px: usize,
        stats: &mut AiWorkspaceBuildStats,
    ) -> AiWorkspaceDisplayGeometry {
        let mut top_px = AI_WORKSPACE_SURFACE_BLOCK_TOP_PADDING_PX;
        let mut geometry_blocks = Vec::with_capacity(self.blocks.len());

        for block_index in 0..self.blocks.len() {
            let block = self.blocks[block_index].clone();
            let height_px = self
                .text_layout_for_block(&block, surface_width_px, stats)
                .height_px;
            geometry_blocks.push(AiWorkspaceBlockGeometry {
                block_index,
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

    fn text_layout_for_block(
        &mut self,
        block: &AiWorkspaceBlock,
        surface_width_px: usize,
        stats: &mut AiWorkspaceBuildStats,
    ) -> AiWorkspaceBlockTextLayout {
        let width_bucket = ai_workspace_width_bucket(surface_width_px);
        let cache_key = ai_workspace_block_layout_cache_key(block);
        if let Some(cached) = self
            .text_layouts_by_width_bucket
            .get(&width_bucket)
            .and_then(|layouts| layouts.get(&cache_key))
        {
            if block.kind == AiWorkspaceBlockKind::DiffSummary
                && block.expanded
                && block.inline_diff_source.is_some()
            {
                stats.record_inline_diff_projection_cache_hit();
            }
            stats.record_text_layout_cache_hit();
            return cached.clone();
        }

        let build_started_at = Instant::now();
        let inline_diff_state = if block.kind == AiWorkspaceBlockKind::DiffSummary && block.expanded
        {
            self.inline_diff_for_block_with_stats(block.id.as_str(), surface_width_px, stats)
        } else {
            None
        };
        let layout = ai_workspace_text_layout_for_block_with_inline_diff(
            block,
            surface_width_px,
            inline_diff_state.as_deref(),
        );
        self.text_layouts_by_width_bucket
            .entry(width_bucket)
            .or_default()
            .insert(cache_key, layout.clone());
        stats.record_text_layout_build(build_started_at.elapsed());
        layout
    }
}

#[allow(dead_code)]
pub(crate) fn ai_workspace_text_layout_for_block(
    block: &AiWorkspaceBlock,
    surface_width_px: usize,
) -> AiWorkspaceBlockTextLayout {
    ai_workspace_text_layout_for_block_with_inline_diff(block, surface_width_px, None)
}

fn ai_workspace_block_layout_cache_key(block: &AiWorkspaceBlock) -> (String, u64) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    block.hash(&mut hasher);
    (block.id.clone(), hasher.finish())
}

fn ai_workspace_text_layout_for_block_with_inline_diff(
    block: &AiWorkspaceBlock,
    surface_width_px: usize,
    inline_diff_state: Option<&AiWorkspaceInlineDiffState>,
) -> AiWorkspaceBlockTextLayout {
    let block_width_px = ai_workspace_block_width_px(surface_width_px, block.kind, block.role);
    let text_width_px = ai_workspace_block_text_width_px(block_width_px);
    let title_lines = ai_workspace_wrap_text(
        block.title.as_str(),
        ai_workspace_chars_per_line(text_width_px, true, false),
        2,
    );
    let title_line_style_spans =
        ai_workspace_title_style_spans(block, title_lines.as_slice(), text_width_px);
    let (
        preview_lines,
        preview_line_kinds,
        preview_line_hit_targets,
        preview_line_link_ranges,
        preview_line_style_spans,
        preview_line_syntax_spans,
        preview_copy_regions,
    ) = if let Some(inline_diff_state) = inline_diff_state {
        ai_workspace_inline_diff_preview_lines(block, inline_diff_state, text_width_px)
    } else if block.kind == AiWorkspaceBlockKind::Message {
        ai_workspace_message_preview_lines(block.preview.as_str(), text_width_px, block)
    } else {
        let preview_lines = ai_workspace_wrap_text(
            block.preview.as_str(),
            ai_workspace_chars_per_line(text_width_px, false, block.mono_preview),
            ai_workspace_preview_line_limit(block),
        );
        let preview_line_style_spans = if block.kind == AiWorkspaceBlockKind::Plan {
            preview_lines
                .iter()
                .map(|line| match line.as_bytes().get(..4) {
                    Some(b"[x] ") => vec![
                        AiWorkspacePreviewStyleSpan {
                            range: 0..3,
                            color_role: Some(AiWorkspacePreviewColorRole::Added),
                            bold: false,
                            italic: false,
                            strikethrough: false,
                            code: false,
                            link: false,
                        },
                        AiWorkspacePreviewStyleSpan {
                            range: 4..line.len(),
                            color_role: Some(AiWorkspacePreviewColorRole::Muted),
                            bold: false,
                            italic: false,
                            strikethrough: true,
                            code: false,
                            link: false,
                        },
                    ],
                    Some(b"[>] ") => vec![
                        AiWorkspacePreviewStyleSpan {
                            range: 0..3,
                            color_role: Some(AiWorkspacePreviewColorRole::Accent),
                            bold: false,
                            italic: false,
                            strikethrough: false,
                            code: false,
                            link: false,
                        },
                        AiWorkspacePreviewStyleSpan {
                            range: 4..line.len(),
                            color_role: Some(AiWorkspacePreviewColorRole::Foreground),
                            bold: true,
                            italic: false,
                            strikethrough: false,
                            code: false,
                            link: false,
                        },
                    ],
                    Some(b"[ ] ") => vec![
                        AiWorkspacePreviewStyleSpan {
                            range: 0..3,
                            color_role: Some(AiWorkspacePreviewColorRole::Muted),
                            bold: false,
                            italic: false,
                            strikethrough: false,
                            code: false,
                            link: false,
                        },
                        AiWorkspacePreviewStyleSpan {
                            range: 4..line.len(),
                            color_role: Some(AiWorkspacePreviewColorRole::Muted),
                            bold: false,
                            italic: false,
                            strikethrough: false,
                            code: false,
                            link: false,
                        },
                    ],
                    _ => vec![AiWorkspacePreviewStyleSpan {
                        range: 0..line.len(),
                        color_role: Some(AiWorkspacePreviewColorRole::Muted),
                        bold: false,
                        italic: true,
                        strikethrough: false,
                        code: false,
                        link: false,
                    }],
                })
                .collect()
        } else if block.kind == AiWorkspaceBlockKind::DiffSummary {
            preview_lines
                .iter()
                .map(|line| ai_workspace_diff_summary_line_style_spans(line, text_width_px))
                .collect()
        } else {
            Vec::new()
        };
        (
            preview_lines.clone(),
            vec![
                if block.mono_preview {
                    AiWorkspacePreviewLineKind::Code
                } else {
                    AiWorkspacePreviewLineKind::Normal
                };
                preview_lines.len()
            ],
            vec![None; preview_lines.len()],
            Vec::new(),
            preview_line_style_spans,
            Vec::new(),
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
        title_line_style_spans,
        preview_lines,
        preview_line_kinds,
        preview_line_hit_targets,
        preview_line_link_ranges,
        preview_line_style_spans,
        preview_line_syntax_spans,
        preview_copy_regions,
        height_px: AI_WORKSPACE_BLOCK_CONTENT_TOP_PADDING_PX
            + title_height_px
            + preview_gap_px
            + preview_height_px
            + AI_WORKSPACE_BLOCK_CONTENT_BOTTOM_PADDING_PX,
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
        AiWorkspaceBlockKind::DiffSummary => {
            if block.expanded {
                240
            } else {
                5
            }
        }
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

fn ai_workspace_title_style_spans(
    block: &AiWorkspaceBlock,
    title_lines: &[String],
    text_width_px: usize,
) -> Vec<Vec<AiWorkspacePreviewStyleSpan>> {
    if block.kind == AiWorkspaceBlockKind::DiffSummary {
        return title_lines
            .iter()
            .map(|line| ai_workspace_diff_summary_line_style_spans(line, text_width_px))
            .collect();
    }

    title_lines
        .iter()
        .map(|line| {
            let mut spans = Vec::new();
            if let Some(status) = block.status_label.as_deref()
                && let Some(start) = line.rfind(status)
            {
                spans.push(AiWorkspacePreviewStyleSpan {
                    range: start..start.saturating_add(status.len()),
                    color_role: block.status_color_role,
                    bold: true,
                    italic: false,
                    strikethrough: false,
                    code: false,
                    link: false,
                });
            }
            spans
        })
        .collect()
}

fn ai_workspace_width_bucket(width_px: usize) -> usize {
    const AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX: usize = 40;

    let clamped = width_px.max(AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX);
    (clamped / AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX) * AI_WORKSPACE_WIDTH_BUCKET_SIZE_PX
}

include!("ai_workspace_session_preview.rs");
include!("ai_workspace_session_projection.rs");
