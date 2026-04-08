impl AiPerfWindow {
    fn record_thread_sidebar_row_render(&mut self, kind: AiPerfSidebarRowKind, duration: Duration) {
        self.thread_sidebar_row_render.record(duration);
        match kind {
            AiPerfSidebarRowKind::ProjectHeader => {
                self.thread_sidebar_project_header_row_render
                    .record(duration);
            }
            AiPerfSidebarRowKind::Thread => {
                self.thread_sidebar_thread_row_render.record(duration);
            }
            AiPerfSidebarRowKind::EmptyProject => {
                self.thread_sidebar_empty_project_row_render
                    .record(duration);
            }
            AiPerfSidebarRowKind::ProjectFooter => {
                self.thread_sidebar_project_footer_row_render
                    .record(duration);
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

    pub(super) fn record_ai_visible_frame_timeline_rows_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| {
            window.visible_frame_timeline_rows.record(duration);
        });
    }

    pub(super) fn record_ai_visible_frame_composer_feedback_timing(&self, duration: Duration) {
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

    pub(super) fn record_ai_workspace_session_rebuild_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| {
            window.workspace_session_rebuild.record(duration);
        });
    }

    pub(super) fn record_ai_workspace_session_refresh_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| {
            window.workspace_session_refresh.record(duration);
        });
    }

    pub(super) fn record_ai_workspace_session_cache_hit(&self) {
        self.update_ai_perf_window(|window| {
            window.workspace_session_cache_hits =
                window.workspace_session_cache_hits.saturating_add(1);
        });
    }

    pub(super) fn record_ai_workspace_surface_geometry_rebuild_timing(&self, duration: Duration) {
        self.update_ai_perf_window(|window| {
            window.workspace_surface_geometry_rebuild.record(duration);
        });
    }

    pub(super) fn record_ai_workspace_surface_text_layout_stats(
        &self,
        duration: Duration,
        build_count: u32,
        cache_hits: u32,
    ) {
        self.update_ai_perf_window(|window| {
            if build_count > 0 {
                window.workspace_surface_text_layout_build.record(duration);
                window.workspace_surface_text_layout_builds = window
                    .workspace_surface_text_layout_builds
                    .saturating_add(build_count as u64);
            }
            if cache_hits > 0 {
                window.workspace_surface_text_layout_cache_hits = window
                    .workspace_surface_text_layout_cache_hits
                    .saturating_add(cache_hits as u64);
            }
        });
    }

    pub(super) fn record_ai_workspace_surface_paint_timing(
        &self,
        duration: Duration,
        visible_block_count: usize,
    ) {
        self.update_ai_perf_window(|window| {
            window.workspace_surface_paint.record(duration);
            window.workspace_surface_visible_blocks_total = window
                .workspace_surface_visible_blocks_total
                .saturating_add(visible_block_count as u64);
        });
    }

    pub(super) fn record_ai_workspace_surface_hit_test(&self) {
        self.update_ai_perf_window(|window| {
            window.workspace_surface_hit_tests =
                window.workspace_surface_hit_tests.saturating_add(1);
        });
    }

    pub(super) fn roll_ai_perf_sample_if_due(&mut self) {
        self.ai_perf_metrics.borrow_mut().roll_sample_if_due();
    }
}
