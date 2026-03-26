impl DiffViewer {
    fn start_fps_monitor(&mut self, cx: &mut Context<Self>) {
        let epoch = self.next_fps_epoch();
        self.schedule_fps_sample(epoch, cx);
    }

    fn next_fps_epoch(&mut self) -> usize {
        self.fps_epoch = self.fps_epoch.saturating_add(1);
        self.fps_epoch
    }

    fn schedule_fps_sample(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch != self.fps_epoch {
            return;
        }

        self.fps_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FPS_SAMPLE_INTERVAL).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    let elapsed = this.frame_sample_started_at.elapsed().as_secs_f32();
                    if elapsed > 0.0 {
                        this.fps = this.frame_sample_count as f32 / elapsed;
                    } else {
                        this.fps = 0.0;
                    }
                    this.frame_sample_count = 0;
                    this.frame_sample_started_at = Instant::now();

                    if !this.recently_scrolling()
                        && let Some(visible_row) = this.last_visible_row_start
                    {
                        this.request_visible_row_segment_prefetch(visible_row, true, cx);
                    }

                    if this.workspace_view_mode == WorkspaceViewMode::Ai {
                        this.roll_ai_perf_sample_if_due();
                    }
                    let next_epoch = this.next_fps_epoch();
                    this.schedule_fps_sample(next_epoch, cx);
                    this.ignore_next_frame_sample = true;
                    cx.notify();
                });
            }
        });
    }
}
