fn ai_group_row_style(
    _kind: &str,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> (Hsla, Hsla, Hsla, Hsla) {
    let colors = hunk_disclosure_row(cx.theme(), is_dark);
    (
        colors.title,
        colors.summary,
        colors.hover_background,
        colors.chevron,
    )
}

fn render_ai_tool_item_row(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row_id: &str,
    item: &hunk_codex::state::ItemSummary,
    is_dark: bool,
    nested: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let content_text = item.content.trim();
    let title = ai_tool_header_title(item);
    let compact_summary = ai_tool_compact_summary(item, content_text);
    let summary_uses_mono = item.kind == "commandExecution";
    let status = ai_item_status_label(item.status);
    let status_color = ai_item_status_color(item.status, cx);
    let show_status = item.status != hunk_codex::state::ItemStatus::Completed;
    let command_details = (item.kind == "commandExecution")
        .then(|| ai_command_execution_display_details(item))
        .flatten();
    let details_text = if item.kind == "commandExecution" {
        content_text
    } else {
        ai_timeline_item_details_json(item).unwrap_or(content_text)
    };
    let has_details = command_details.is_some() || !details_text.is_empty();
    let expanded = has_details && this.ai_expanded_timeline_row_ids.contains(row_id);
    let show_toggle = has_details;
    let row_id_string = row_id.to_string();
    let disclosure_colors = hunk_disclosure_row(cx.theme(), is_dark);
    let hover_bg_color = disclosure_colors.hover_background;
    let chevron_color = disclosure_colors.chevron;

    let header = h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded(px(8.0))
        .when(show_toggle, |this| {
            let row_id = row_id_string.clone();
            let view = view.clone();
            let hover_bg = hover_bg_color;
            this.hover(move |style| style.bg(hover_bg).cursor_pointer())
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.ai_toggle_timeline_row_expansion_action(row_id.clone(), cx);
                    });
                })
        })
        .child({
            let mut title_row = h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .min_w_0()
                        .text_xs()
                        .font_semibold()
                        .whitespace_nowrap()
                        .child(title),
                );
            if let Some(summary) = compact_summary {
                let mut summary_element = div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(disclosure_colors.summary)
                    .truncate()
                    .child(summary);
                if summary_uses_mono {
                    summary_element = summary_element.font_family(cx.theme().mono_font_family.clone());
                }
                title_row = title_row.child(summary_element);
            }
            title_row
        })
        .child(
            h_flex()
                .flex_none()
                .items_center()
                .gap_1p5()
                .when(show_status, |this| {
                    this.child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(status_color)
                            .child(status),
                    )
                })
                .when(show_toggle, |this| {
                    this.child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(12.0))
                        .text_color(chevron_color),
                    )
                }),
        );

    let expanded_body = command_details
        .as_ref()
        .map(|details| {
            render_ai_command_execution_details(
                this,
                view.clone(),
                row_id,
                details,
                content_text,
                is_dark,
                cx,
            )
        })
        .unwrap_or_else(|| {
            ai_tool_detail_section(
                this,
                view.clone(),
                row_id,
                ai_timeline_text_surface_id(row_id, "tool-details", 0),
                "Details",
                details_text.to_string(),
                true,
                Some(px(240.0)),
                false,
                is_dark,
                cx,
            )
        });

    let row_content = v_flex()
        .max_w(px(940.0))
        .w_full()
        .min_w_0()
        .gap_1p5()
        .child(header)
        .when(expanded, |this| {
            this.child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_2()
                    .child(expanded_body),
            )
        });

    let row_element = h_flex()
        .w_full()
        .min_w_0()
        .justify_start()
        .child(
            div()
                .w_full()
                .min_w_0()
                .when(nested, |this| this.pl_4())
                .child(row_content),
        );

    if nested {
        row_element.into_any_element()
    } else {
        ai_timeline_row_with_animation(this, row_id, row_element)
    }
}

fn render_ai_timeline_group_row(
    this: &DiffViewer,
    view: Entity<DiffViewer>,
    row: &AiTimelineRow,
    group: &AiTimelineGroup,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> AnyElement {
    let expanded = this.ai_expanded_timeline_row_ids.contains(row.id.as_str());
    let (title_color, summary_color, hover_bg_color, chevron_color) =
        ai_group_row_style(group.kind.as_str(), is_dark, cx);
    let row_id = row.id.clone();

    let children = group
        .child_row_ids
        .iter()
        .filter_map(|child_row_id| {
            let row = this.ai_timeline_row(child_row_id.as_str())?;
            let AiTimelineRowSource::Item { item_key } = &row.source else {
                return None;
            };
            let item = this.ai_state_snapshot.items.get(item_key.as_str())?;
            Some(render_ai_tool_item_row(
                this,
                view.clone(),
                child_row_id.as_str(),
                item,
                is_dark,
                true,
                cx,
            ))
        })
        .collect::<Vec<_>>();

    let row_element = h_flex()
        .w_full()
        .min_w_0()
        .justify_start()
        .child(
            v_flex()
                .max_w(px(940.0))
                .w_full()
                .min_w_0()
                .gap_1p5()
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .rounded(px(8.0))
                        .hover(move |style| style.bg(hover_bg_color).cursor_pointer())
                        .on_mouse_down(MouseButton::Left, {
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.ai_toggle_timeline_row_expansion_action(row_id.clone(), cx);
                                });
                            }
                        })
                        .child({
                            let mut title_row = h_flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_none()
                                        .min_w_0()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(title_color)
                                        .whitespace_nowrap()
                                        .child(group.title.clone()),
                                );
                            if let Some(summary) = group.summary.as_ref() {
                                title_row = title_row.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_xs()
                                        .text_color(summary_color)
                                        .truncate()
                                        .child(summary.clone()),
                                );
                            }
                            title_row
                        })
                        .child(
                            h_flex()
                                .flex_none()
                                .items_center()
                                .gap_1()
                                .child(
                                    Icon::new(if expanded {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .size(px(12.0))
                                    .text_color(chevron_color),
                                ),
                        ),
                )
                .when(expanded, |this| {
                    this.child(v_flex().w_full().min_w_0().gap_1().children(children))
                }),
        );

    ai_timeline_row_with_animation(this, row.id.as_str(), row_element)
}
