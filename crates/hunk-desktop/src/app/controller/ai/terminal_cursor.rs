impl DiffViewer {
    fn ai_terminal_cursor_should_blink(&self) -> bool {
        self.ai_terminal_open
            && self.ai_terminal_surface_focused
            && self
                .ai_terminal_session
                .screen
                .as_ref()
                .is_some_and(|screen| {
                    screen.mode.show_cursor
                        && crate::app::terminal_cursor::ai_terminal_cursor_shape_blinks(
                            screen.cursor.shape,
                        )
                })
    }

    fn ai_stop_terminal_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.ai_terminal_cursor_blink_generation =
            self.ai_terminal_cursor_blink_generation.saturating_add(1);
        self.ai_terminal_cursor_blink_active = false;
        self.ai_terminal_cursor_blink_task = Task::ready(());
        if !self.ai_terminal_cursor_blink_visible {
            self.ai_terminal_cursor_blink_visible = true;
            cx.notify();
        }
    }

    fn ai_start_terminal_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.ai_terminal_cursor_blink_generation =
            self.ai_terminal_cursor_blink_generation.saturating_add(1);
        let generation = self.ai_terminal_cursor_blink_generation;
        self.ai_terminal_cursor_blink_active = true;
        if !self.ai_terminal_cursor_blink_visible {
            self.ai_terminal_cursor_blink_visible = true;
            cx.notify();
        }
        self.ai_terminal_cursor_blink_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(crate::app::terminal_cursor::AI_TERMINAL_CURSOR_BLINK_INTERVAL)
                    .await;

                let Some(this) = this.upgrade() else {
                    return;
                };

                let mut keep_running = true;
                this.update(cx, |this, cx| {
                    if this.ai_terminal_cursor_blink_generation != generation
                        || !this.ai_terminal_cursor_should_blink()
                    {
                        this.ai_terminal_cursor_blink_active = false;
                        if !this.ai_terminal_cursor_blink_visible {
                            this.ai_terminal_cursor_blink_visible = true;
                            cx.notify();
                        }
                        keep_running = false;
                        return;
                    }

                    this.ai_terminal_cursor_blink_visible = !this.ai_terminal_cursor_blink_visible;
                    cx.notify();
                });
                if !keep_running {
                    return;
                }
            }
        });
    }

    pub(super) fn ai_sync_terminal_cursor_blink(&mut self, cx: &mut Context<Self>) {
        if self.ai_terminal_cursor_should_blink() {
            if !self.ai_terminal_cursor_blink_active {
                self.ai_start_terminal_cursor_blink(cx);
            }
            return;
        }

        if self.ai_terminal_cursor_blink_active || !self.ai_terminal_cursor_blink_visible {
            self.ai_stop_terminal_cursor_blink(cx);
        }
    }
}
