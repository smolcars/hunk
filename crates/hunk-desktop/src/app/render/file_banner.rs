#[derive(Clone)]
struct ReviewWorkspaceFileHeaderPaint {
    row_background: gpui::Hsla,
    row_divider: gpui::Hsla,
    accent_strip: gpui::Hsla,
    badge_background: gpui::Hsla,
    badge_border: gpui::Hsla,
    badge_label: SharedString,
    badge_text_color: gpui::Hsla,
    path: SharedString,
    path_text_color: gpui::Hsla,
    stats_label: SharedString,
    stats_label_color: gpui::Hsla,
    stats_added: SharedString,
    stats_added_color: gpui::Hsla,
    stats_removed: SharedString,
    stats_removed_color: gpui::Hsla,
    stats_changed: SharedString,
    stats_changed_color: gpui::Hsla,
}

impl DiffViewer {
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

    fn build_review_workspace_file_header_paint(
        &self,
        path: &str,
        status: FileStatus,
        stats: LineStats,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> ReviewWorkspaceFileHeaderPaint {
        let is_dark = cx.theme().mode.is_dark();
        let chrome = hunk_diff_chrome(cx.theme(), is_dark);
        let colors = hunk_file_status_banner(cx.theme(), status, is_dark, is_selected);
        let line_stats = hunk_line_stats(cx.theme(), is_dark);

        ReviewWorkspaceFileHeaderPaint {
            row_background: colors.row_background,
            row_divider: chrome.row_divider,
            accent_strip: colors.accent_strip,
            badge_background: colors.badge_background,
            badge_border: colors.badge_border,
            badge_label: SharedString::from(colors.label),
            badge_text_color: cx.theme().foreground,
            path: SharedString::from(path.to_string()),
            path_text_color: cx.theme().foreground,
            stats_label: SharedString::from("file"),
            stats_label_color: cx.theme().muted_foreground,
            stats_added: SharedString::from(format!("+{}", stats.added)),
            stats_added_color: line_stats.added,
            stats_removed: SharedString::from(format!("-{}", stats.removed)),
            stats_removed_color: line_stats.removed,
            stats_changed: SharedString::from(format!("chg {}", stats.changed())),
            stats_changed_color: line_stats.changed,
        }
    }

    fn render_review_workspace_file_header_controls_overlay(
        &self,
        row_ix: usize,
        path: &str,
        status: FileStatus,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let stable_row_id = self.diff_row_stable_id(row_ix);
        let is_dark = cx.theme().mode.is_dark();
        let path = path.to_string();
        let is_collapsed = self.collapsed_files.contains(path.as_str());
        let colors = hunk_file_status_banner(cx.theme(), status, is_dark, is_selected);

        h_flex()
            .size_full()
            .items_center()
            .justify_between()
            .px_3()
            .child({
                let view = view.clone();
                let path = path.clone();
                Button::new(("toggle-file-collapse-surface", stable_row_id))
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
            .child(self.render_review_view_file_button(
                ("diff-file-view-surface", stable_row_id),
                path.as_str(),
                status,
                view,
                cx,
            ))
            .into_any_element()
    }
}

fn paint_review_workspace_file_header_row(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    paint: &ReviewWorkspaceFileHeaderPaint,
    mono_font_family: SharedString,
) {
    let left_padding = px(12.0);
    let collapse_button_reserve = px(30.0);
    let badge_gap = px(8.0);
    let right_padding = px(12.0);
    let stats_gap = px(8.0);
    let view_button_reserve = px(88.0);
    let badge_height = px(18.0);

    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_quad(gpui::fill(bounds, paint.row_background));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(bounds.origin.x, bounds.origin.y + bounds.size.height - px(1.0)),
                size: gpui::size(bounds.size.width, px(1.0)),
            },
            paint.row_divider,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: bounds.origin,
                size: gpui::size(px(2.0), bounds.size.height),
            },
            paint.accent_strip,
        ));

        let text_style = gpui::TextStyle {
            color: paint.path_text_color,
            font_family: mono_font_family.clone(),
            font_size: px(12.0).into(),
            line_height: gpui::relative(1.45),
            ..Default::default()
        };
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = text_style.line_height_in_pixels(window.rem_size());
        let text_y = bounds.origin.y + ((bounds.size.height - line_height) / 2.).max(Pixels::ZERO);

        let badge_runs = vec![TextRun {
            len: paint.badge_label.len(),
            color: paint.badge_text_color,
            font: font.clone(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let badge_shape =
            window
                .text_system()
                .shape_line(paint.badge_label.clone(), font_size, &badge_runs, None);
        let badge_width = badge_shape.width() + px(12.0);
        let badge_x = bounds.origin.x + left_padding + collapse_button_reserve;
        let badge_y = bounds.origin.y + ((bounds.size.height - badge_height) / 2.).max(Pixels::ZERO);
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y),
                size: gpui::size(badge_width, badge_height),
            },
            paint.badge_background,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y),
                size: gpui::size(badge_width, px(1.0)),
            },
            paint.badge_border,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y + badge_height - px(1.0)),
                size: gpui::size(badge_width, px(1.0)),
            },
            paint.badge_border,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x, badge_y),
                size: gpui::size(px(1.0), badge_height),
            },
            paint.badge_border,
        ));
        window.paint_quad(gpui::fill(
            Bounds {
                origin: point(badge_x + badge_width - px(1.0), badge_y),
                size: gpui::size(px(1.0), badge_height),
            },
            paint.badge_border,
        ));
        let _ = badge_shape.paint(
            point(
                badge_x + ((badge_width - badge_shape.width()) / 2.).max(Pixels::ZERO),
                text_y,
            ),
            line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        );

        let stats_items = [
            (&paint.stats_changed, paint.stats_changed_color),
            (&paint.stats_removed, paint.stats_removed_color),
            (&paint.stats_added, paint.stats_added_color),
            (&paint.stats_label, paint.stats_label_color),
        ];
        let mut cursor_x = bounds.origin.x + bounds.size.width - right_padding - view_button_reserve;
        for (text, color) in stats_items {
            let runs = vec![TextRun {
                len: text.len(),
                color,
                font: font.clone(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            let shape = window
                .text_system()
                .shape_line(text.clone(), font_size, &runs, None);
            cursor_x -= shape.width();
            let _ = shape.paint(
                point(cursor_x, text_y),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
            cursor_x -= stats_gap;
        }

        let path_x = badge_x + badge_width + badge_gap;
        let path_right = (cursor_x - stats_gap).max(path_x);
        let path_bounds = Bounds {
            origin: point(path_x, bounds.origin.y),
            size: gpui::size((path_right - path_x).max(Pixels::ZERO), bounds.size.height),
        };
        let path_runs = vec![TextRun {
            len: paint.path.len(),
            color: paint.path_text_color,
            font,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let path_shape = window
            .text_system()
            .shape_line(paint.path.clone(), font_size, &path_runs, None);
        window.with_content_mask(Some(ContentMask { bounds: path_bounds }), |window| {
            let _ = path_shape.paint(
                point(path_x, text_y),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        });
    });
}
