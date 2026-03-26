pub(super) struct AiTimelineListView {
    root_view: gpui::WeakEntity<DiffViewer>,
    timeline_visible_row_ids: Arc<[String]>,
    ai_timeline_list_state: ListState,
    ai_timeline_follow_output: bool,
}

impl AiTimelineListView {
    pub(super) fn new(
        root_view: gpui::WeakEntity<DiffViewer>,
        ai_timeline_list_state: ListState,
    ) -> Self {
        Self {
            root_view,
            timeline_visible_row_ids: Arc::from([]),
            ai_timeline_list_state,
            ai_timeline_follow_output: true,
        }
    }

    pub(super) fn sync_state(
        &mut self,
        timeline_visible_row_ids: Arc<[String]>,
        ai_timeline_follow_output: bool,
        cx: &mut Context<Self>,
    ) {
        let row_ids_changed = self.timeline_visible_row_ids.as_ref() != timeline_visible_row_ids.as_ref();
        let follow_output_changed = self.ai_timeline_follow_output != ai_timeline_follow_output;
        if !row_ids_changed && !follow_output_changed {
            return;
        }

        self.timeline_visible_row_ids = timeline_visible_row_ids;
        self.ai_timeline_follow_output = ai_timeline_follow_output;
        cx.notify();
    }
}

impl Render for AiTimelineListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(root_view) = self.root_view.upgrade() else {
            return div().size_full();
        };

        let render_started_at = Instant::now();
        let is_dark = cx.theme().mode.is_dark();
        let theme = cx.theme().clone();
        let row_ids = self.timeline_visible_row_ids.clone();
        let visible_row_count = row_ids.len();
        let list_state = self.ai_timeline_list_state.clone();
        let root_view_for_list: Entity<DiffViewer> = root_view.clone();
        let timeline_list = list(list_state.clone(), {
            cx.processor(move |_this, ix: usize, _window, cx| {
                let Some(row_id) = row_ids.get(ix) else {
                    return div().w_full().h(px(0.0)).into_any_element();
                };
                let this = root_view_for_list.read(cx);
                render_ai_chat_timeline_row_for_view(
                    this,
                    row_id.as_str(),
                    root_view_for_list.clone(),
                    &theme,
                    is_dark,
                )
            })
        })
        .size_full()
        .with_sizing_behavior(ListSizingBehavior::Auto);

        let element = div()
            .size_full()
            .relative()
            .child(div().size_full().child(timeline_list))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(16.0))
                    .child(
                        Scrollbar::vertical(&list_state).scrollbar_show(ScrollbarShow::Always),
                    ),
            )
            .when(!self.ai_timeline_follow_output, |this| {
                let root_view = root_view.clone();
                this.child(
                    div()
                        .absolute()
                        .right(px(16.0))
                        .bottom(px(8.0))
                        .left_0()
                        .flex()
                        .justify_center()
                        .child(
                            Button::new("ai-timeline-scroll-to-bottom")
                                .compact()
                                .primary()
                                .with_size(gpui_component::Size::Small)
                                .icon(Icon::new(IconName::ChevronDown).size(px(14.0)))
                                .tooltip("Scroll to the bottom")
                                .on_click(move |_, _, cx| {
                                    root_view.update(cx, |this: &mut DiffViewer, cx| {
                                        this.ai_scroll_timeline_to_bottom_action(cx);
                                    });
                                }),
                        ),
                )
            });
        root_view.read(cx).record_ai_timeline_list_render_timing(
            render_started_at.elapsed(),
            visible_row_count,
        );
        element
    }
}
