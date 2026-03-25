use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, EventEmitter,
    InteractiveElement as _, IntoElement, Keystroke, MouseButton, ParentElement as _, Pixels,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    anchored, deferred, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, ElementExt as _, Icon, IconName, Sizable as _, Size, StyleSized as _,
    input::{Input, InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarShow},
    v_flex,
};

use super::theme::hunk_completion_menu;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HunkPickerAction {
    SelectNext,
    SelectPrevious,
    Accept,
    Dismiss,
}

pub(crate) fn hunk_picker_action_for_keystroke(keystroke: &Keystroke) -> Option<HunkPickerAction> {
    let modifiers = &keystroke.modifiers;
    if modifiers.modified() {
        return None;
    }

    match keystroke.key.as_str() {
        "down" => Some(HunkPickerAction::SelectNext),
        "up" => Some(HunkPickerAction::SelectPrevious),
        "enter" => Some(HunkPickerAction::Accept),
        "escape" => Some(HunkPickerAction::Dismiss),
        _ => None,
    }
}

pub(crate) trait HunkPickerItem: Clone {
    type Value: Clone + PartialEq;

    fn title(&self) -> SharedString;

    fn value(&self) -> &Self::Value;

    fn display_title(&self) -> Option<AnyElement> {
        None
    }

    fn render(&self, cx: &mut App) -> AnyElement;
}

pub(crate) trait HunkPickerDelegate: Clone + Default + 'static {
    type Item: HunkPickerItem;

    fn items_count(&self) -> usize;

    fn item(&self, ix: usize) -> Option<&Self::Item>;

    fn position<V>(&self, value: &V) -> Option<usize>
    where
        Self::Item: HunkPickerItem<Value = V>,
        V: PartialEq;

    fn perform_search(&mut self, query: &str);
}

pub(crate) enum HunkPickerEvent<D>
where
    D: HunkPickerDelegate,
{
    Confirm(Option<<<D as HunkPickerDelegate>::Item as HunkPickerItem>::Value>),
}

pub(crate) struct HunkPickerState<D>
where
    D: HunkPickerDelegate,
{
    search_input_state: Entity<InputState>,
    scroll_handle: ScrollHandle,
    delegate: D,
    selected_index: Option<usize>,
    confirmed_index: Option<usize>,
    selected_value: Option<<<D as HunkPickerDelegate>::Item as HunkPickerItem>::Value>,
    open: bool,
    bounds: Bounds<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl<D> HunkPickerState<D>
where
    D: HunkPickerDelegate,
{
    pub(crate) fn new(
        delegate: D,
        selected_index: Option<usize>,
        search_placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_input_state =
            cx.new(|cx| InputState::new(window, cx).placeholder(search_placeholder));

        let _subscriptions = vec![cx.subscribe(&search_input_state, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.refresh_matches_from_input(cx);
            }
        })];

        let mut this = Self {
            search_input_state,
            scroll_handle: ScrollHandle::default(),
            delegate,
            selected_index: None,
            confirmed_index: None,
            selected_value: None,
            open: false,
            bounds: Bounds::default(),
            _subscriptions,
        };
        this.set_selected_index(selected_index, window, cx);
        this
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn items_count(&self) -> usize {
        self.delegate.items_count()
    }

    pub(crate) fn item(&self, ix: usize) -> Option<&D::Item> {
        self.delegate.item(ix)
    }

    pub(crate) fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub(crate) fn selected_value(
        &self,
    ) -> Option<&<<D as HunkPickerDelegate>::Item as HunkPickerItem>::Value> {
        self.selected_value.as_ref()
    }

    pub(crate) fn search_input_state(&self) -> Entity<InputState> {
        self.search_input_state.clone()
    }

    pub(crate) fn scroll_handle(&self) -> ScrollHandle {
        self.scroll_handle.clone()
    }

    pub(crate) fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    pub(crate) fn set_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.bounds = bounds;
    }

    pub(crate) fn set_items(&mut self, delegate: D, _: &mut Window, cx: &mut Context<Self>) {
        self.delegate = delegate;
        self.refresh_matches_for_query(self.search_query(cx).as_str(), cx);
    }

    pub(crate) fn set_selected_index(
        &mut self,
        selected_index: Option<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirmed_index = selected_index;
        self.selected_index = selected_index;
        self.selected_value = selected_index
            .and_then(|ix| self.delegate.item(ix))
            .map(|item| item.value().clone());
        self.scroll_to_selected();
        cx.notify();
    }

    pub(crate) fn set_selected_value(
        &mut self,
        selected_value: &<<D as HunkPickerDelegate>::Item as HunkPickerItem>::Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_index = self.delegate.position(selected_value);
        self.set_selected_index(selected_index, window, cx);
    }

    pub(crate) fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.dismiss(window, cx);
        } else {
            self.open(window, cx);
        }
    }

    pub(crate) fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }

        self.open = true;
        self.search_input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        self.refresh_matches_for_query("", cx);
        cx.notify();
    }

    pub(crate) fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }

        self.open = false;
        self.search_input_state
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.refresh_matches_for_query("", cx);
        self.selected_index = self.confirmed_index;
        self.scroll_to_selected();
        cx.notify();
    }

    pub(crate) fn highlight_index(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.selected_index == Some(ix) {
            return;
        }
        self.selected_index = Some(ix);
        cx.notify();
    }

    pub(crate) fn apply_action(
        &mut self,
        action: HunkPickerAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.open {
            return false;
        }

        match action {
            HunkPickerAction::SelectNext => {
                let next_index = match (self.selected_index, self.items_count()) {
                    (_, 0) => None,
                    (Some(selected_index), count) => Some((selected_index + 1).min(count - 1)),
                    (None, _) => Some(0),
                };
                self.selected_index = next_index;
                self.scroll_to_selected();
                cx.notify();
                true
            }
            HunkPickerAction::SelectPrevious => {
                self.selected_index = match (self.selected_index, self.items_count()) {
                    (_, 0) => None,
                    (Some(selected_index), _) => Some(selected_index.saturating_sub(1)),
                    (None, count) => Some(count.saturating_sub(1)),
                };
                self.scroll_to_selected();
                cx.notify();
                true
            }
            HunkPickerAction::Accept => {
                self.confirm_selection(window, cx);
                true
            }
            HunkPickerAction::Dismiss => {
                self.dismiss(window, cx);
                true
            }
        }
    }

    pub(crate) fn confirm_index(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = Some(ix);
        self.confirm_selection(window, cx);
    }

    fn confirm_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.confirmed_index = self.selected_index;
        self.selected_value = self
            .confirmed_index
            .and_then(|ix| self.delegate.item(ix))
            .map(|item| item.value().clone());
        let selected_value = self.selected_value.clone();
        self.open = false;
        self.search_input_state
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.refresh_matches_for_query("", cx);
        cx.emit(HunkPickerEvent::<D>::Confirm(selected_value));
        cx.notify();
    }

    fn refresh_matches_from_input(&mut self, cx: &mut Context<Self>) {
        let query = self.search_query(cx);
        self.refresh_matches_for_query(query.as_str(), cx);
    }

    fn refresh_matches_for_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.delegate.perform_search(query);
        self.confirmed_index = self
            .selected_value
            .as_ref()
            .and_then(|selected_value| self.delegate.position(selected_value));
        self.selected_index = if query.trim().is_empty() {
            self.confirmed_index
                .or_else(|| self.first_selectable_index())
        } else {
            self.confirmed_index
                .or_else(|| self.first_selectable_index())
        };
        self.scroll_to_selected();
        cx.notify();
    }

    fn first_selectable_index(&self) -> Option<usize> {
        (self.items_count() > 0).then_some(0)
    }

    fn search_query(&self, cx: &App) -> String {
        self.search_input_state.read(cx).value().to_string()
    }

    fn scroll_to_selected(&self) {
        if let Some(selected_index) = self.selected_index {
            self.scroll_handle.scroll_to_item(selected_index);
        }
    }
}

impl<D> EventEmitter<HunkPickerEvent<D>> for HunkPickerState<D> where D: HunkPickerDelegate {}

pub(crate) struct HunkPickerConfig {
    id: SharedString,
    placeholder: SharedString,
    disabled: bool,
    empty: Option<AnyElement>,
    size: Size,
    width: Option<Pixels>,
    min_width: Option<Pixels>,
    max_width: Option<Pixels>,
    fill_width: bool,
    menu_width: Option<Pixels>,
    menu_max_width: Option<Pixels>,
    menu_max_height: Pixels,
    rounded: Pixels,
    background: Option<gpui::Hsla>,
    border_color: Option<gpui::Hsla>,
}

impl HunkPickerConfig {
    pub(crate) fn new(id: impl Into<SharedString>, placeholder: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            placeholder: placeholder.into(),
            disabled: false,
            empty: None,
            size: Size::Medium,
            width: None,
            min_width: None,
            max_width: None,
            fill_width: false,
            menu_width: None,
            menu_max_width: None,
            menu_max_height: px(320.0),
            rounded: px(8.0),
            background: None,
            border_color: None,
        }
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn empty(mut self, empty: impl IntoElement) -> Self {
        self.empty = Some(empty.into_any_element());
        self
    }

    pub(crate) fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub(crate) fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    pub(crate) fn min_width(mut self, min_width: Pixels) -> Self {
        self.min_width = Some(min_width);
        self
    }

    pub(crate) fn max_width(mut self, max_width: Pixels) -> Self {
        self.max_width = Some(max_width);
        self
    }

    pub(crate) fn fill_width(mut self) -> Self {
        self.fill_width = true;
        self
    }

    pub(crate) fn rounded(mut self, rounded: Pixels) -> Self {
        self.rounded = rounded;
        self
    }

    pub(crate) fn background(mut self, background: gpui::Hsla) -> Self {
        self.background = Some(background);
        self
    }

    pub(crate) fn border_color(mut self, border_color: gpui::Hsla) -> Self {
        self.border_color = Some(border_color);
        self
    }
}

pub(crate) fn render_hunk_picker<D>(
    state: &Entity<HunkPickerState<D>>,
    config: HunkPickerConfig,
    cx: &mut App,
) -> AnyElement
where
    D: HunkPickerDelegate,
{
    let is_dark = cx.theme().mode.is_dark();
    let colors = hunk_completion_menu(cx.theme(), is_dark);
    let (open, selected_index, selected_value, items, scroll_handle, search_input_state, bounds) = {
        let picker_state = state.read(cx);
        (
            picker_state.is_open(),
            picker_state.selected_index(),
            picker_state.selected_value().cloned(),
            (0..picker_state.items_count())
                .filter_map(|ix| picker_state.item(ix).cloned().map(|item| (ix, item)))
                .collect::<Vec<_>>(),
            picker_state.scroll_handle(),
            picker_state.search_input_state(),
            picker_state.bounds(),
        )
    };
    let placeholder = config.placeholder.clone();
    let trigger_background = config.background.unwrap_or(colors.panel.background);
    let trigger_border = config.border_color.unwrap_or(colors.panel.border);
    let title = selected_value
        .as_ref()
        .and_then(|value| items.iter().find(|(_, item)| item.value() == value))
        .map(|(_, item)| item.clone())
        .map(|item| {
            item.display_title()
                .unwrap_or_else(|| item.title().into_any_element())
        })
        .unwrap_or_else(|| {
            div()
                .text_color(cx.theme().muted_foreground)
                .child(placeholder)
                .into_any_element()
        });

    let state_for_bounds = state.clone();
    let state_for_toggle = state.clone();
    let mut trigger = div()
        .relative()
        .flex()
        .items_center()
        .justify_between()
        .gap_1()
        .border_1()
        .border_color(trigger_border)
        .bg(trigger_background)
        .rounded(config.rounded)
        .overflow_hidden()
        .input_size(config.size)
        .input_text_size(config.size)
        .on_prepaint(move |bounds, _, cx| {
            state_for_bounds.update(cx, |this, _| this.set_bounds(bounds));
        });

    if config.disabled {
        trigger = trigger.opacity(0.5);
    } else {
        trigger = trigger.on_mouse_down(MouseButton::Left, move |_, window, cx| {
            state_for_toggle.update(cx, |this, cx| this.toggle(window, cx));
            cx.stop_propagation();
        });
    }

    let root = div()
        .id(config.id.to_string())
        .relative()
        .map(|this| {
            let this = if config.fill_width {
                this.w_full()
            } else {
                this
            };
            let this = if let Some(width) = config.width {
                this.w(width)
            } else {
                this
            };
            let this = if let Some(min_width) = config.min_width {
                this.min_w(min_width)
            } else {
                this
            };
            if let Some(max_width) = config.max_width {
                this.max_w(max_width)
            } else {
                this
            }
        })
        .child(
            trigger
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .truncate()
                        .text_color(cx.theme().foreground)
                        .child(title),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                ),
        );

    if !open {
        return root.into_any_element();
    }

    root.child(
        deferred(
            anchored().snap_to_window_with_margin(px(8.0)).child(
                div()
                    .occlude()
                    .map(|this| {
                        let default_width = bounds.size.width + px(2.0);
                        let this = this.w(config.menu_width.unwrap_or(default_width));
                        if let Some(max_width) = config.menu_max_width {
                            this.max_w(max_width)
                        } else {
                            this
                        }
                    })
                    .child(
                        v_flex()
                            .occlude()
                            .mt_1p5()
                            .bg(colors.panel.background)
                            .border_1()
                            .border_color(colors.panel.border)
                            .rounded(px(12.0))
                            .shadow_md()
                            .overflow_hidden()
                            .child(
                                div().px_2().pt_2().pb_1p5().child(
                                    Input::new(&search_input_state)
                                        .with_size(config.size)
                                        .rounded(px(10.0))
                                        .border_color(colors.panel.border)
                                        .bg(colors.row_hover),
                                ),
                            )
                            .child(
                                div()
                                    .id(format!("{}-scroll-area", config.id))
                                    .relative()
                                    .max_h(config.menu_max_height)
                                    .track_scroll(&scroll_handle)
                                    .overflow_y_scroll()
                                    .children(if items.is_empty() {
                                        vec![
                                            config
                                                .empty
                                                .unwrap_or_else(|| {
                                                    div()
                                                        .h(px(72.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child("No matches.")
                                                        .into_any_element()
                                                })
                                                .into_any_element(),
                                        ]
                                    } else {
                                        items
                                            .into_iter()
                                            .map(|(ix, item)| {
                                                let row_state = state.clone();
                                                let hover_state = row_state.clone();
                                                let is_selected = selected_index == Some(ix);

                                                div()
                                                    .w_full()
                                                    .px_1()
                                                    .pr_3()
                                                    .child(
                                                        div()
                                                            .w_full()
                                                            .min_w_0()
                                                            .rounded(px(10.0))
                                                            .px_2()
                                                            .py_1p5()
                                                            .when(!is_selected, |this| {
                                                                this.hover(|style| {
                                                                    style.bg(colors.row_hover)
                                                                })
                                                            })
                                                            .when(is_selected, |this| {
                                                                this.bg(colors.row_selected)
                                                                    .border_1()
                                                                    .border_color(
                                                                        colors.row_selected_border,
                                                                    )
                                                            })
                                                            .on_mouse_move(move |_, _, cx| {
                                                                hover_state.update(
                                                                    cx,
                                                                    |this, cx| {
                                                                        this.highlight_index(
                                                                            ix, cx,
                                                                        );
                                                                    },
                                                                );
                                                            })
                                                            .on_mouse_down(
                                                                MouseButton::Left,
                                                                move |_, window, cx| {
                                                                    row_state.update(
                                                                        cx,
                                                                        |this, cx| {
                                                                            this.confirm_index(
                                                                                ix, window, cx,
                                                                            );
                                                                        },
                                                                    );
                                                                    cx.stop_propagation();
                                                                },
                                                            )
                                                            .child(item.render(cx)),
                                                    )
                                                    .into_any_element()
                                            })
                                            .collect()
                                    }),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(46.0))
                                    .right_0()
                                    .bottom_0()
                                    .w(px(12.0))
                                    .child(
                                        Scrollbar::vertical(&scroll_handle)
                                            .scrollbar_show(ScrollbarShow::Scrolling),
                                    ),
                            ),
                    )
                    .on_mouse_down_out({
                        let state = state.clone();
                        move |_, window, cx| {
                            state.update(cx, |this, cx| this.dismiss(window, cx));
                        }
                    }),
            ),
        )
        .with_priority(1),
    )
    .into_any_element()
}
