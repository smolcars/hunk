impl DiffViewer {
    fn diff_row_stable_id(&self, row_ix: usize) -> u64 {
        self.active_diff_row_metadata(row_ix)
            .map(|row| row.stable_id)
            .unwrap_or(row_ix as u64)
    }

    fn render_sticky_file_status_banner_row(
        &self,
        row_ix: usize,
        path: &str,
        status: FileStatus,
        stats: LineStats,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let stable_row_id = self.diff_row_stable_id(row_ix);
        let is_dark = cx.theme().mode.is_dark();
        let chrome = hunk_diff_chrome(cx.theme(), is_dark);
        let path = path.to_string();
        let is_collapsed = self.collapsed_files.contains(path.as_str());
        let colors = hunk_file_status_banner(cx.theme(), status, is_dark, false);

        let row = h_flex()
            .id(("diff-file-header-sticky-row", stable_row_id))
            .relative()
            .overflow_x_hidden()
            .w_full()
            .items_center()
            .gap_2()
            .px_3()
            .py_0p5()
            .border_b_1()
            .border_color(chrome.row_divider)
            .bg(colors.row_background)
            .w_full()
            .child({
                let view = view.clone();
                let path = path.clone();
                Button::new(("toggle-file-collapse-sticky", stable_row_id))
                    .ghost()
                    .compact()
                    .icon(
                        Icon::new(if is_collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .size(px(14.0)),
                    )
                    .min_w(px(22.0))
                    .h(px(22.0))
                    .text_color(colors.arrow)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        view.update(cx, |this, cx| {
                            this.toggle_file_collapsed(path.clone(), cx);
                        });
                    })
            })
            .child(
                div()
                    .px_1p5()
                    .py_0p5()
                    .text_xs()
                    .font_semibold()
                    .bg(colors.badge_background)
                    .border_1()
                    .border_color(colors.badge_border)
                    .text_color(cx.theme().foreground)
                    .child(colors.label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .truncate()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().foreground)
                    .child(path.clone()),
            )
            .child(self.render_line_stats("file", stats, cx))
            .child(self.render_review_view_file_button(
                ("diff-file-view-sticky", stable_row_id),
                path.as_str(),
                status,
                view.clone(),
                cx,
            ))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.0))
                    .bg(colors.accent_strip),
            );

        if self.reduced_motion_enabled() {
            return row.into_any_element();
        }

        div()
            .w_full()
            .child(
                div()
                    .w_full()
                    .child(row)
                    .with_animation(
                        ("diff-file-header-sticky-bump", stable_row_id),
                        Animation::new(self.animation_duration_ms(160))
                            .with_easing(cubic_bezier(0.32, 0.72, 0.0, 1.0)),
                        |this, delta| {
                            let entering = 1.0 - delta;
                            let bump = (delta * std::f32::consts::PI).sin() * entering;
                            this.w_full()
                                .top(px(entering * 6.0 - bump * 1.5))
                                .opacity(0.9 + (0.1 * delta))
                        },
                    ),
            )
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
