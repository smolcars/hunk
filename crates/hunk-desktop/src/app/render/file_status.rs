impl DiffViewer {
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
        let path = path.to_string();
        let is_collapsed = self.collapsed_files.contains(path.as_str());

        let (label, accent) = match status {
            FileStatus::Added | FileStatus::Untracked => ("NEW FILE", cx.theme().success),
            FileStatus::Deleted => ("DELETED FILE", cx.theme().danger),
            FileStatus::Renamed => ("RENAMED", cx.theme().accent),
            FileStatus::Modified => ("MODIFIED", cx.theme().warning),
            FileStatus::TypeChange => ("TYPE CHANGED", cx.theme().warning),
            FileStatus::Conflicted => ("CONFLICTED", cx.theme().danger),
            FileStatus::Unknown => ("MODIFIED", cx.theme().muted_foreground),
        };
        let row_background = cx
            .theme()
            .background
            .blend(accent.opacity(if is_dark { 0.34 } else { 0.16 }));
        let border_color = accent.opacity(if is_dark { 0.78 } else { 0.52 });
        let badge_background = accent.opacity(if is_dark { 0.50 } else { 0.27 });
        let accent_strip = if is_dark {
            accent.lighten(0.18)
        } else {
            accent.darken(0.06)
        };
        let arrow_color = if is_dark {
            accent.lighten(0.34)
        } else {
            accent.darken(0.18)
        };

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
            .border_color(border_color.opacity(if is_dark { 0.92 } else { 0.82 }))
            .bg(row_background)
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
                    .text_color(arrow_color)
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
                    .rounded_sm()
                    .bg(badge_background)
                    .border_1()
                    .border_color(accent.opacity(if is_dark { 0.88 } else { 0.44 }))
                    .text_color(cx.theme().foreground)
                    .child(label),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().foreground)
                    .child(path.clone()),
            )
            .child(self.render_line_stats("file", stats, cx))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.0))
                    .bg(accent_strip),
            );

        if self.reduced_motion_enabled() {
            row.into_any_element()
        } else {
            row.with_animation(
                ("diff-file-header-sticky-bump", stable_row_id),
                Animation::new(self.animation_duration_ms(180))
                    .with_easing(cubic_bezier(0.32, 0.72, 0.0, 1.0)),
                |this, delta| {
                    let entering = 1.0 - delta;
                    let bump = (delta * std::f32::consts::PI).sin() * entering;
                    this.top(px(entering * 8.0 - bump * 2.0))
                        .opacity(0.88 + (0.12 * delta))
                },
            )
            .into_any_element()
        }
    }

    fn render_line_stats(
        &self,
        label: &'static str,
        stats: LineStats,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
                    .text_color(if cx.theme().mode.is_dark() {
                        cx.theme().success.lighten(0.42)
                    } else {
                        cx.theme().success.darken(0.05)
                    })
                    .child(format!("+{}", stats.added)),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(if cx.theme().mode.is_dark() {
                        cx.theme().danger.lighten(0.42)
                    } else {
                        cx.theme().danger.darken(0.05)
                    })
                    .child(format!("-{}", stats.removed)),
            )
            .child(
                div()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
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
