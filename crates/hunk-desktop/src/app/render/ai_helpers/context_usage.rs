use gpui_component::tooltip::Tooltip;

fn ai_context_usage_cached_input_tokens(
    usage: &hunk_codex::state::TokenUsageBreakdownSummary,
) -> i64 {
    usage.cached_input_tokens.max(0)
}

fn ai_context_usage_non_cached_input_tokens(
    usage: &hunk_codex::state::TokenUsageBreakdownSummary,
) -> i64 {
    (usage.input_tokens.max(0) - ai_context_usage_cached_input_tokens(usage)).max(0)
}

fn ai_context_usage_display_tokens(usage: &hunk_codex::state::ThreadTokenUsageSummary) -> i64 {
    let last = &usage.last;
    (ai_context_usage_non_cached_input_tokens(last) + last.output_tokens.max(0)).max(0)
}

fn ai_context_usage_percent_used(
    usage: &hunk_codex::state::ThreadTokenUsageSummary,
) -> Option<u16> {
    let window = usage.model_context_window.filter(|value| *value > 0)? as u128;
    let used_tokens = ai_context_usage_display_tokens(usage) as u128;
    Some((used_tokens.saturating_mul(100) / window).min(100) as u16)
}

fn ai_context_usage_percent_left(
    usage: &hunk_codex::state::ThreadTokenUsageSummary,
) -> Option<u16> {
    Some(100u16.saturating_sub(ai_context_usage_percent_used(usage)?.min(100)))
}

fn ai_context_usage_compact_token_count(token_count: i64) -> String {
    let token_count = token_count.max(0);
    if token_count >= 1_000_000 {
        format!("{}m", (token_count + 500_000) / 1_000_000)
    } else if token_count >= 1_000 {
        format!("{}k", (token_count + 500) / 1_000)
    } else {
        token_count.to_string()
    }
}

fn ai_context_usage_exact_token_count(token_count: i64) -> String {
    let digits = token_count.max(0).to_string();
    let mut reversed = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            reversed.push(',');
        }
        reversed.push(ch);
    }
    reversed.chars().rev().collect()
}

fn ai_render_context_usage_detail_row(
    label: &str,
    value: String,
    label_color: Hsla,
    value_color: Hsla,
) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div()
                .text_xs()
                .text_color(label_color)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(value_color)
                .child(value),
        )
        .into_any_element()
}

fn ai_render_context_usage_chip(
    usage: &hunk_codex::state::ThreadTokenUsageSummary,
    is_dark: bool,
    cx: &mut Context<DiffViewer>,
) -> Option<AnyElement> {
    let percent_used = ai_context_usage_percent_used(usage)?;
    let usage_color = if percent_used >= 90 {
        cx.theme().danger
    } else if percent_used >= 75 {
        cx.theme().warning
    } else {
        cx.theme().foreground
    };
    let chip_border = if percent_used >= 90 {
        hunk_opacity(cx.theme().danger, is_dark, 0.44, 0.34)
    } else if percent_used >= 75 {
        hunk_opacity(cx.theme().warning, is_dark, 0.44, 0.34)
    } else {
        hunk_opacity(cx.theme().border, is_dark, 0.76, 0.62)
    };
    let chip_background = if percent_used >= 90 {
        hunk_opacity(cx.theme().danger, is_dark, 0.10, 0.06)
    } else if percent_used >= 75 {
        hunk_opacity(cx.theme().warning, is_dark, 0.10, 0.06)
    } else {
        hunk_blend(cx.theme().background, cx.theme().muted, is_dark, 0.16, 0.20)
    };
    let tooltip_usage = usage.clone();

    Some(
        div()
            .id("ai-context-usage-chip")
            .tooltip(move |window, cx| {
                let tooltip_usage = tooltip_usage.clone();
                Tooltip::element(move |_, cx| {
                    v_flex()
                        .w(px(220.0))
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child("Context window"),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(format!(
                                            "{}% used ({}% left)",
                                            ai_context_usage_percent_used(&tooltip_usage)
                                                .unwrap_or_default(),
                                            ai_context_usage_percent_left(&tooltip_usage)
                                                .unwrap_or_default()
                                        )),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!(
                                            "{} / {} tokens",
                                            ai_context_usage_compact_token_count(
                                                ai_context_usage_display_tokens(&tooltip_usage),
                                            ),
                                            ai_context_usage_compact_token_count(
                                                tooltip_usage
                                                    .model_context_window
                                                    .unwrap_or_default(),
                                            )
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .h(px(1.0))
                                .bg(hunk_opacity(cx.theme().border, is_dark, 0.70, 0.56)),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(ai_render_context_usage_detail_row(
                                    "Input",
                                    ai_context_usage_exact_token_count(
                                        ai_context_usage_non_cached_input_tokens(&tooltip_usage.last),
                                    ),
                                    cx.theme().muted_foreground,
                                    cx.theme().foreground,
                                ))
                                .child(ai_render_context_usage_detail_row(
                                    "Cached input",
                                    ai_context_usage_exact_token_count(
                                        ai_context_usage_cached_input_tokens(&tooltip_usage.last),
                                    ),
                                    cx.theme().muted_foreground,
                                    cx.theme().foreground,
                                ))
                                .child(ai_render_context_usage_detail_row(
                                    "Output",
                                    ai_context_usage_exact_token_count(
                                        tooltip_usage.last.output_tokens,
                                    ),
                                    cx.theme().muted_foreground,
                                    cx.theme().foreground,
                                ))
                                .child(ai_render_context_usage_detail_row(
                                    "Reasoning",
                                    ai_context_usage_exact_token_count(
                                        tooltip_usage.last.reasoning_output_tokens,
                                    ),
                                    cx.theme().muted_foreground,
                                    cx.theme().foreground,
                                ))
                                .child(ai_render_context_usage_detail_row(
                                    "Used now",
                                    ai_context_usage_exact_token_count(
                                        ai_context_usage_display_tokens(&tooltip_usage),
                                    ),
                                    cx.theme().muted_foreground,
                                    cx.theme().foreground,
                                )),
                        )
                })
                .build(window, cx)
            })
            .rounded(px(999.0))
            .border_1()
            .border_color(chip_border)
            .bg(chip_background)
            .px_2()
            .py_0p5()
            .child(
                h_flex()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(usage_color)
                            .child(format!("{percent_used}%")),
                    ),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage_with_window(
        total_tokens: i64,
        model_context_window: i64,
    ) -> hunk_codex::state::ThreadTokenUsageSummary {
        hunk_codex::state::ThreadTokenUsageSummary {
            turn_id: "turn-1".to_string(),
            total: hunk_codex::state::TokenUsageBreakdownSummary {
                total_tokens,
                input_tokens: 48_200,
                cached_input_tokens: 14_400,
                output_tokens: 7_800,
                reasoning_output_tokens: 2_489,
            },
            last: hunk_codex::state::TokenUsageBreakdownSummary {
                total_tokens: 6_400,
                input_tokens: 4_500,
                cached_input_tokens: 900,
                output_tokens: 700,
                reasoning_output_tokens: 300,
            },
            model_context_window: Some(model_context_window),
            last_sequence: 3,
        }
    }

    #[test]
    fn context_usage_helpers_use_raw_context_window_percentage() {
        let usage = usage_with_window(72_889, 258_000);

        assert_eq!(ai_context_usage_non_cached_input_tokens(&usage.last), 3_600);
        assert_eq!(ai_context_usage_display_tokens(&usage), 4_300);
        assert_eq!(ai_context_usage_percent_used(&usage), Some(1));
        assert_eq!(ai_context_usage_percent_left(&usage), Some(99));
        assert_eq!(ai_context_usage_compact_token_count(72_889), "73k");
        assert_eq!(ai_context_usage_exact_token_count(72_889), "72,889");
    }

    #[test]
    fn context_usage_percentage_requires_a_positive_window() {
        let mut usage = usage_with_window(12_000, 128_000);
        usage.model_context_window = None;
        assert_eq!(ai_context_usage_percent_used(&usage), None);

        usage.model_context_window = Some(0);
        assert_eq!(ai_context_usage_percent_used(&usage), None);
    }

    #[test]
    fn context_usage_percentage_clamps_when_used_exceeds_window() {
        let mut usage = usage_with_window(500_000, 128_000);
        usage.last.input_tokens = 300_000;
        usage.last.cached_input_tokens = 10_000;
        usage.last.output_tokens = 40_000;

        assert_eq!(ai_context_usage_display_tokens(&usage), 330_000);
        assert_eq!(ai_context_usage_percent_used(&usage), Some(100));
        assert_eq!(ai_context_usage_percent_left(&usage), Some(0));
    }
}
