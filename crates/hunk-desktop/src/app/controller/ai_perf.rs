impl AiPerfWindow {
    fn record_timeline_row_render(
        &mut self,
        kind: AiPerfTimelineRowKind,
        duration: Duration,
    ) {
        self.timeline_row_render.record(duration);
        match kind {
            AiPerfTimelineRowKind::Message => self.message_row_render.record(duration),
            AiPerfTimelineRowKind::Tool => self.tool_row_render.record(duration),
            AiPerfTimelineRowKind::Group => self.group_row_render.record(duration),
            AiPerfTimelineRowKind::Diff => self.diff_row_render.record(duration),
            AiPerfTimelineRowKind::Plan => self.plan_row_render.record(duration),
        }
    }

    fn record_thread_sidebar_row_render(
        &mut self,
        kind: AiPerfSidebarRowKind,
        duration: Duration,
    ) {
        self.thread_sidebar_row_render.record(duration);
        match kind {
            AiPerfSidebarRowKind::ProjectHeader => {
                self.thread_sidebar_project_header_row_render.record(duration);
            }
            AiPerfSidebarRowKind::Thread => {
                self.thread_sidebar_thread_row_render.record(duration);
            }
            AiPerfSidebarRowKind::EmptyProject => {
                self.thread_sidebar_empty_project_row_render.record(duration);
            }
            AiPerfSidebarRowKind::ProjectFooter => {
                self.thread_sidebar_project_footer_row_render.record(duration);
            }
        }
    }
}

impl AiPerfMetrics {
    fn roll_sample_if_due(&mut self) {
        if self.window_started_at.elapsed() < AI_PERF_SAMPLE_INTERVAL {
            return;
        }

        self.window = AiPerfWindow::default();
        self.window_started_at = Instant::now();
    }
}

impl DiffViewer {
    fn update_ai_perf_window(&self, update: impl FnOnce(&mut AiPerfWindow)) {
        let mut metrics = self.ai_perf_metrics.borrow_mut();
        update(&mut metrics.window);
    }

    pub(super) fn record_ai_app_render_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| window.app_render.record(duration));
    }

    pub(super) fn record_ai_footer_render_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| window.footer_render.record(duration));
    }

    pub(super) fn record_ai_visible_frame_cache_hit(&self) {
        self.update_ai_perf_window(|window| {
            window.visible_frame_cache_hits = window.visible_frame_cache_hits.saturating_add(1);
        });
    }

    pub(super) fn record_ai_visible_frame_build_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| window.visible_frame_build.record(duration));
    }

    pub(super) fn record_ai_visible_frame_invalidation(&self, reason: &'static str) {
        self.update_ai_perf_window(|window| {
            window.visible_frame_invalidations =
                window.visible_frame_invalidations.saturating_add(1);
            let entry = window
                .visible_frame_invalidation_reasons
                .entry(reason)
                .or_insert(0);
            *entry = entry.saturating_add(1);
        });
    }

    pub(super) fn record_ai_visible_frame_timeline_rows_timing(
        &self,
        duration: Duration,
    ) {
        self.update_ai_perf_window(|window| {
            window.visible_frame_timeline_rows.record(duration);
        });
    }

    pub(super) fn record_ai_visible_frame_composer_feedback_timing(
        &self,
        duration: Duration,
    ) {
        self.update_ai_perf_window(|window| {
            window.visible_frame_composer_feedback.record(duration);
        });
    }

    pub(super) fn record_ai_thread_sidebar_rebuild_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| {
            window.thread_sidebar_rebuild.record(duration);
        });
    }

    pub(super) fn record_ai_thread_sidebar_render_timing(
        &self,
        duration: Duration,
        visible_row_count: usize,
    ) {
        self.update_ai_perf_window(|window| {
            window.thread_sidebar_render.record(duration);
            window.thread_sidebar_visible_rows_total = window
                .thread_sidebar_visible_rows_total
                .saturating_add(visible_row_count as u64);
        });
    }

    pub(super) fn record_ai_thread_sidebar_row_render_timing(
        &self,
        kind: AiPerfSidebarRowKind,
        duration: Duration,
    ) {
        self.update_ai_perf_window(|window| {
            window.record_thread_sidebar_row_render(kind, duration);
        });
    }

    pub(super) fn record_ai_timeline_index_rebuild_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| {
            window.timeline_index_rebuild.record(duration);
        });
    }

    pub(super) fn record_ai_timeline_list_sync(
        &self,
        row_ids_changed: bool,
        follow_output_changed: bool,
        visible_row_count: usize,
    ) {
        self.update_ai_perf_window(|window| {
            window.timeline_list_sync_count =
                window.timeline_list_sync_count.saturating_add(1);
            if row_ids_changed {
                window.timeline_list_sync_row_ids_changed =
                    window.timeline_list_sync_row_ids_changed.saturating_add(1);
            }
            if follow_output_changed {
                window.timeline_list_sync_follow_output_changed = window
                    .timeline_list_sync_follow_output_changed
                    .saturating_add(1);
            }
            window.timeline_list_sync_visible_rows_total = window
                .timeline_list_sync_visible_rows_total
                .saturating_add(visible_row_count as u64);
        });
    }

    pub(super) fn record_ai_timeline_list_render_timing(
        &self,
        duration: Duration,
        visible_row_count: usize,
    ) {
        self.update_ai_perf_window(|window| {
            window.timeline_list_render.record(duration);
            window.timeline_list_render_visible_rows_total = window
                .timeline_list_render_visible_rows_total
                .saturating_add(visible_row_count as u64);
        });
    }

    pub(super) fn record_ai_timeline_row_render_timing(
        &self,
        kind: AiPerfTimelineRowKind,
        duration: Duration,
    ) {
        self.update_ai_perf_window(|window| {
            window.record_timeline_row_render(kind, duration);
        });
    }

    pub(super) fn record_ai_timeline_row_skipped(&self) {
        self.update_ai_perf_window(|window| {
            window.timeline_row_skipped = window.timeline_row_skipped.saturating_add(1);
        });
    }

    pub(super) fn record_ai_markdown_cache_hit(&self) {
        self.update_ai_perf_window(|window| {
            window.markdown_cache_hits = window.markdown_cache_hits.saturating_add(1);
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_ai_markdown_cache_miss(
        &self,
        parse_duration: Duration,
        comrak_parse_duration: Duration,
        transform_duration: Duration,
        code_highlight_duration: Duration,
        code_block_count: usize,
        code_char_count: usize,
        selection_surface_duration: Duration,
    ) {
        self.update_ai_perf_window(|window| {
            window.markdown_cache_misses = window.markdown_cache_misses.saturating_add(1);
            window.markdown_parse.record(parse_duration);
            window.markdown_comrak_parse.record(comrak_parse_duration);
            window.markdown_transform.record(transform_duration);
            window.markdown_code_highlight.record(code_highlight_duration);
            window.markdown_code_block_count_total = window
                .markdown_code_block_count_total
                .saturating_add(code_block_count as u64);
            window.markdown_code_char_count_total = window
                .markdown_code_char_count_total
                .saturating_add(code_char_count as u64);
            window
                .markdown_selection_surfaces
                .record(selection_surface_duration);
        });
    }

    pub(super) fn record_ai_markdown_render_build(
        &self,
        duration: Duration,
        block_count: usize,
        char_count: usize,
    ) {
        self.update_ai_perf_window(|window| {
            window.markdown_render_build.record(duration);
            window.markdown_render_block_count_total = window
                .markdown_render_block_count_total
                .saturating_add(block_count as u64);
            window.markdown_render_char_count_total = window
                .markdown_render_char_count_total
                .saturating_add(char_count as u64);
        });
    }

    pub(super) fn roll_ai_perf_sample_if_due(&mut self) {
        self.ai_perf_metrics.borrow_mut().roll_sample_if_due();
    }
}
