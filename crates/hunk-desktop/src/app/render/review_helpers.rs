impl DiffViewer {
    #[allow(dead_code)]
    fn review_view_file_shortcut_label(&self) -> Option<String> {
        let shortcuts = self.config.keyboard_shortcuts.view_current_review_file.as_slice();
        let preferred = if cfg!(target_os = "macos") {
            shortcuts
                .iter()
                .find(|shortcut| shortcut.to_ascii_lowercase().contains("cmd"))
        } else {
            shortcuts
                .iter()
                .find(|shortcut| shortcut.to_ascii_lowercase().contains("ctrl"))
        }
        .or_else(|| shortcuts.first())?;
        Some(format_shortcut_label(preferred.as_str()))
    }

    #[allow(dead_code)]
    fn render_review_view_file_button(
        &self,
        button_id: (&'static str, u64),
        path: &str,
        status: FileStatus,
        view: Entity<DiffViewer>,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = path.to_string();
        let disabled = !self.can_open_file_in_files_workspace(path.as_str(), status);
        let tooltip = self
            .review_view_file_shortcut_label()
            .map_or_else(|| "View file".to_string(), |shortcut| {
                format!("View file ({shortcut})")
            });

        Button::new(button_id)
            .outline()
            .compact()
            .rounded(px(7.0))
            .label("View File")
            .disabled(disabled)
            .tooltip(tooltip)
            .on_click(move |_, window, cx| {
                view.update(cx, |this, cx| {
                    this.open_file_in_files_workspace(path.clone(), status, window, cx);
                });
            })
            .into_any_element()
    }

    fn render_line_stats(
        &self,
        label: &'static str,
        stats: LineStats,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = hunk_line_stats(cx.theme(), cx.theme().mode.is_dark());
        h_flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.added)
                    .child(format!("+{}", stats.added)),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.removed)
                    .child(format!("-{}", stats.removed)),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(colors.changed)
                    .child(format!("chg {}", stats.changed())),
            )
            .into_any_element()
    }

    #[allow(dead_code)]
    fn diff_row_stable_id(&self, row_ix: usize) -> u64 {
        self.diff_row_metadata
            .get(row_ix)
            .map(|row| row.stable_id)
            .unwrap_or(row_ix as u64)
    }
}

fn relative_time_label(unix_time: Option<i64>) -> String {
    let Some(unix_time) = unix_time else {
        return "unknown".to_string();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(unix_time);

    let elapsed = now.saturating_sub(unix_time).max(0);

    if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 60 * 60 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 60 * 60 * 24 {
        format!("{}h ago", elapsed / (60 * 60))
    } else {
        format!("{}d ago", elapsed / (60 * 60 * 24))
    }
}
