fn ai_perf_top_invalidation_reason(
    window: &AiPerfWindow,
) -> Option<(&'static str, u32)> {
    window
        .visible_frame_invalidation_reasons
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(reason, count)| (*reason, *count))
}

fn ai_perf_format_average_ms(stats: AiPerfDurationStats) -> String {
    format!("{:.1}", stats.average_us() as f64 / 1_000.0)
}

fn ai_perf_format_count_avg_ms(stats: AiPerfDurationStats) -> String {
    format!("{}x{}ms", stats.count, ai_perf_format_average_ms(stats))
}

fn ai_perf_average_total(total: u64, count: u32) -> u64 {
    if count == 0 {
        0
    } else {
        total / u64::from(count)
    }
}

fn ai_perf_report_line(report: &AiPerfReport, fps: f32) -> Option<String> {
    if report.elapsed_ms == 0 || !report.window.has_data() {
        return None;
    }

    let invalidation_label = ai_perf_top_invalidation_reason(&report.window)
        .map(|(reason, count)| format!("{count}({reason})"))
        .unwrap_or_else(|| report.window.visible_frame_invalidations.to_string());

    Some(format!(
        concat!(
            "ai_perf fps={fps:.0} elapsed={elapsed}ms ",
            "app={app} tb={tb} tbp={tbp} tbl={tbl} tbr={tbr} foot={foot} root={root} vf={vf} cache={cache} inv={inv} ",
            "vf_rows={vf_rows} cmp={cmp} side={side} side_r={side_r} side_row={side_row} idx={idx} ",
            "sync={sync} sync_rows={sync_rows} sync_chg={sync_chg} sync_follow={sync_follow} ",
            "list={list} list_rows={list_rows} ",
            "row={row} skip={skip} msg={msg} tool={tool} grp={grp} diff={diff} plan={plan} ",
            "md={md_hits}/{md_miss} md_parse={md_parse} md_doc={md_doc} md_xform={md_xform} md_code={md_code} md_surf={md_surf} ",
            "md_build={md_build} md_blocks={md_blocks} md_chars={md_chars}"
        ),
        fps = fps,
        elapsed = report.elapsed_ms,
        app = ai_perf_format_count_avg_ms(report.window.app_render),
        tb = ai_perf_format_count_avg_ms(report.window.toolbar_render),
        tbp = ai_perf_format_count_avg_ms(report.window.toolbar_prep),
        tbl = ai_perf_format_count_avg_ms(report.window.toolbar_left_render),
        tbr = ai_perf_format_count_avg_ms(report.window.toolbar_right_render),
        foot = ai_perf_format_count_avg_ms(report.window.footer_render),
        root = ai_perf_format_count_avg_ms(report.window.root_render),
        vf = ai_perf_format_count_avg_ms(report.window.visible_frame_build),
        cache = report.window.visible_frame_cache_hits,
        inv = invalidation_label,
        vf_rows = ai_perf_format_count_avg_ms(report.window.visible_frame_timeline_rows),
        cmp = ai_perf_format_count_avg_ms(report.window.visible_frame_composer_feedback),
        side = ai_perf_format_count_avg_ms(report.window.thread_sidebar_rebuild),
        side_r = ai_perf_format_count_avg_ms(report.window.thread_sidebar_render),
        side_row = ai_perf_format_count_avg_ms(report.window.thread_sidebar_row_render),
        idx = ai_perf_format_count_avg_ms(report.window.timeline_index_rebuild),
        sync = report.window.timeline_list_sync_count,
        sync_rows = ai_perf_average_total(
            report.window.timeline_list_sync_visible_rows_total,
            report.window.timeline_list_sync_count,
        ),
        sync_chg = report.window.timeline_list_sync_row_ids_changed,
        sync_follow = report.window.timeline_list_sync_follow_output_changed,
        list = ai_perf_format_count_avg_ms(report.window.timeline_list_render),
        list_rows = ai_perf_average_total(
            report.window.timeline_list_render_visible_rows_total,
            report.window.timeline_list_render.count,
        ),
        row = ai_perf_format_count_avg_ms(report.window.timeline_row_render),
        skip = report.window.timeline_row_skipped,
        msg = ai_perf_format_count_avg_ms(report.window.message_row_render),
        tool = ai_perf_format_count_avg_ms(report.window.tool_row_render),
        grp = ai_perf_format_count_avg_ms(report.window.group_row_render),
        diff = ai_perf_format_count_avg_ms(report.window.diff_row_render),
        plan = ai_perf_format_count_avg_ms(report.window.plan_row_render),
        md_hits = report.window.markdown_cache_hits,
        md_miss = report.window.markdown_cache_misses,
        md_parse = ai_perf_format_count_avg_ms(report.window.markdown_parse),
        md_doc = ai_perf_format_count_avg_ms(report.window.markdown_comrak_parse),
        md_xform = ai_perf_format_count_avg_ms(report.window.markdown_transform),
        md_code = ai_perf_format_count_avg_ms(report.window.markdown_code_highlight),
        md_surf = ai_perf_format_count_avg_ms(report.window.markdown_selection_surfaces),
        md_build = ai_perf_format_count_avg_ms(report.window.markdown_render_build),
        md_blocks = ai_perf_average_total(
            report.window.markdown_render_block_count_total,
            report.window.markdown_render_build.count,
        ),
        md_chars = ai_perf_average_total(
            report.window.markdown_render_char_count_total,
            report.window.markdown_render_build.count,
        ),
    ))
}

impl AiPerfWindow {
    fn has_data(&self) -> bool {
        self.app_render.count > 0
            || self.toolbar_render.count > 0
            || self.toolbar_prep.count > 0
            || self.toolbar_left_render.count > 0
            || self.toolbar_right_render.count > 0
            || self.footer_render.count > 0
            || self.root_render.count > 0
            || self.visible_frame_build.count > 0
            || self.visible_frame_cache_hits > 0
            || self.visible_frame_invalidations > 0
            || self.thread_sidebar_rebuild.count > 0
            || self.thread_sidebar_render.count > 0
            || self.thread_sidebar_row_render.count > 0
            || self.timeline_index_rebuild.count > 0
            || self.timeline_list_sync_count > 0
            || self.timeline_list_render.count > 0
            || self.timeline_row_render.count > 0
            || self.markdown_cache_hits > 0
            || self.markdown_cache_misses > 0
            || self.markdown_render_build.count > 0
    }

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
    fn roll_sample_if_due(&mut self) -> bool {
        if self.window_started_at.elapsed() < AI_PERF_SAMPLE_INTERVAL {
            return false;
        }

        self.last_report = AiPerfReport {
            elapsed_ms: self.window_started_at.elapsed().as_millis() as u64,
            window: std::mem::take(&mut self.window),
        };
        self.window_started_at = Instant::now();
        true
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

    pub(super) fn roll_ai_perf_sample_if_due(&mut self) -> bool {
        self.ai_perf_metrics.borrow_mut().roll_sample_if_due()
    }

    pub(super) fn ai_perf_log_line(&self, fps: f32) -> Option<String> {
        let metrics = self.ai_perf_metrics.borrow();
        ai_perf_report_line(&metrics.last_report, fps)
    }

    pub(super) fn ai_perf_toolbar_label(&self) -> Option<String> {
        let metrics = self.ai_perf_metrics.borrow();
        let report = &metrics.last_report;
        if report.elapsed_ms == 0 || !report.window.has_data() {
            return None;
        }

        let invalidation_label = ai_perf_top_invalidation_reason(&report.window)
            .map(|(reason, count)| format!("{count}({reason})"))
            .unwrap_or_else(|| report.window.visible_frame_invalidations.to_string());

        Some(format!(
            "ai vf {}x{}ms idx {}x{}ms row {}x{}ms md {}/{} inv {}",
            report.window.visible_frame_build.count,
            ai_perf_format_average_ms(report.window.visible_frame_build),
            report.window.timeline_index_rebuild.count,
            ai_perf_format_average_ms(report.window.timeline_index_rebuild),
            report.window.timeline_row_render.count,
            ai_perf_format_average_ms(report.window.timeline_row_render),
            report.window.markdown_cache_hits,
            report.window.markdown_cache_misses,
            invalidation_label,
        ))
    }
}
