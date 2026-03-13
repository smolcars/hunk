use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use anyhow::{Context as _, Result, anyhow};
use arc_swap::{ArcSwap, access::Map};
use gpui::{prelude::FluentBuilder, *};
use helix_core::syntax::HighlightEvent;
use helix_term::commands;
use helix_term::compositor::{Compositor, Context as CompositorContext, EventResult};
use helix_term::config::Config as HelixConfig;
use helix_term::job::Jobs;
use helix_term::keymap::Keymaps;
use helix_term::ui::EditorView;
use helix_view::editor::Action;
use helix_view::graphics::{Color, CursorKind, Rect, Style};
use helix_view::handlers::Handlers;
use helix_view::input::KeyEvent;
use helix_view::{Document, DocumentId, Editor, Theme, View, ViewId, theme};

pub(crate) type SharedHelixFilesEditor = Rc<RefCell<HelixFilesEditor>>;

pub(crate) struct HelixFilesEditor {
    runtime: Option<HelixRuntime>,
    active_path: Option<PathBuf>,
    last_error: Option<String>,
}

struct HelixRuntime {
    editor: Editor,
    compositor: Compositor,
    view: EditorView,
    jobs: Jobs,
}

#[derive(Clone)]
pub(crate) struct HelixFilesEditorElement {
    state: SharedHelixFilesEditor,
    focus: FocusHandle,
    is_focused: bool,
    style: TextStyle,
    palette: HelixFilesEditorPalette,
}

#[derive(Clone, Copy)]
pub(crate) struct HelixFilesEditorPalette {
    pub(crate) background: Hsla,
    pub(crate) line_number: Hsla,
    pub(crate) current_line_number: Hsla,
    pub(crate) border: Hsla,
    pub(crate) default_foreground: Hsla,
}

#[derive(Debug)]
struct DocumentLayout {
    rows: usize,
    line_height: Pixels,
    font_size: Pixels,
    cell_width: Pixels,
    gutter_columns: usize,
    hitbox: Option<Hitbox>,
}

struct RopeWrapper<'a>(helix_core::ropey::RopeSlice<'a>);

struct StyleIter<'a, H: Iterator<Item = HighlightEvent>> {
    text_style: Style,
    active_highlights: Vec<helix_core::syntax::Highlight>,
    highlight_iter: H,
    theme: &'a Theme,
}

impl HelixFilesEditor {
    pub(crate) fn new() -> Self {
        Self {
            runtime: None,
            active_path: None,
            last_error: None,
        }
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn is_ready_for_path(&self, path: Option<&str>) -> bool {
        self.runtime.is_some()
            && self
                .active_path
                .as_ref()
                .and_then(|active| active.to_str())
                == path
    }

    pub(crate) fn open_path(&mut self, path: &Path) -> Result<()> {
        let runtime = self.runtime.get_or_insert_with(HelixRuntime::new);
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(err) => {
                let message = err.to_string();
                self.last_error = Some(message.clone());
                return Err(anyhow!(message));
            }
        };

        runtime
            .editor
            .open(path, Action::Replace)
            .with_context(|| format!("failed to open {} in Helix editor", path.display()))?;
        self.active_path = Some(path.to_path_buf());
        self.last_error = None;
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.active_path = None;
        self.last_error = None;
    }

    pub(crate) fn is_dirty(&self) -> bool {
        let Some(runtime) = self.runtime.as_ref().and_then(Result::ok) else {
            return false;
        };
        let Some(doc_id) = runtime.current_doc_id() else {
            return false;
        };
        runtime
            .editor
            .document(doc_id)
            .is_some_and(Document::is_modified)
    }

    pub(crate) fn current_text(&self) -> Option<String> {
        let runtime = self.runtime.as_ref().and_then(Result::ok)?;
        let doc_id = runtime.current_doc_id()?;
        runtime
            .editor
            .document(doc_id)
            .map(|doc| doc.text().slice(..).to_string())
    }

    pub(crate) fn mark_saved(&mut self) {
        let Some(runtime) = self.runtime.as_mut().and_then(Result::ok) else {
            return;
        };
        let Some(doc_id) = runtime.current_doc_id() else {
            return;
        };
        if let Some(doc) = runtime.editor.document_mut(doc_id) {
            doc.reset_modified();
            doc.pickup_last_saved_time();
        }
    }

    pub(crate) fn handle_keystroke(&mut self, keystroke: &Keystroke) -> bool {
        let Some(runtime) = self.runtime.as_mut().and_then(Result::ok) else {
            return false;
        };
        let Some(key) = translate_key(keystroke) else {
            return false;
        };

        let mut comp_ctx = CompositorContext {
            editor: &mut runtime.editor,
            scroll: None,
            jobs: &mut runtime.jobs,
        };
        let mut is_handled = runtime
            .compositor
            .handle_event(&helix_view::input::Event::Key(key), &mut comp_ctx);
        if !is_handled {
            let event = &helix_view::input::Event::Key(key);
            let result = runtime.view.handle_event(event, &mut comp_ctx);
            is_handled = matches!(result, EventResult::Consumed(_));
            if let EventResult::Consumed(Some(callback)) = result {
                callback(&mut runtime.compositor, &mut comp_ctx);
            }
        }

        is_handled
    }

    pub(crate) fn scroll_lines(
        &mut self,
        line_count: usize,
        direction: helix_core::movement::Direction,
    ) {
        let Some(runtime) = self.runtime.as_mut().and_then(Result::ok) else {
            return;
        };
        let mut ctx = commands::Context {
            editor: &mut runtime.editor,
            register: None,
            count: None,
            callback: Vec::new(),
            on_next_key_callback: None,
            jobs: &mut runtime.jobs,
        };
        commands::scroll(&mut ctx, line_count, direction, false);
    }
}

impl HelixRuntime {
    fn new() -> Result<Self> {
        ensure_helix_loader_initialized();

        let mut config = HelixConfig::load_default().unwrap_or_default();
        config.editor.lsp.enable = false;

        let mut theme_parent_dirs = vec![helix_loader::config_dir()];
        theme_parent_dirs.extend(helix_loader::runtime_dirs().iter().cloned());
        let theme_loader = Arc::new(theme::Loader::new(&theme_parent_dirs));
        let true_color = true;
        let theme = config
            .theme
            .as_ref()
            .and_then(|theme_name| {
                theme_loader
                    .load(theme_name)
                    .ok()
                    .filter(|loaded_theme| true_color || loaded_theme.is_16_color())
            })
            .unwrap_or_else(|| theme_loader.default_theme(true_color));

        let lang_loader = helix_core::config::user_lang_loader()
            .unwrap_or_else(|_| helix_core::config::default_lang_loader());
        let syn_loader = Arc::new(ArcSwap::from_pointee(lang_loader));
        let config = Arc::new(ArcSwap::from_pointee(config));

        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 25,
        };
        let (completions, _completions_rx) = tokio::sync::mpsc::channel(1);
        let (signature_hints, _signature_hints_rx) = tokio::sync::mpsc::channel(1);
        let handlers = Handlers {
            completions,
            signature_hints,
        };
        let mut editor = Editor::new(
            area,
            theme_loader.clone(),
            syn_loader,
            Arc::new(Map::new(Arc::clone(&config), |config: &HelixConfig| {
                &config.editor
            })),
            handlers,
        );
        editor.new_file(Action::VerticalSplit);
        editor.set_theme(theme);

        let keys = Box::new(Map::new(Arc::clone(&config), |config: &HelixConfig| {
            &config.keys
        }));
        let compositor = Compositor::new(area);
        let view = EditorView::new(Keymaps::new(keys));
        let jobs = Jobs::new();

        ensure_helix_events_registered();

        Ok(Self {
            editor,
            compositor,
            view,
            jobs,
        })
    }

    fn current_view_id(&self) -> Option<ViewId> {
        self.editor.tree.views().next().map(|(view, _)| view.id)
    }

    fn current_doc_id(&self) -> Option<DocumentId> {
        let view_id = self.current_view_id()?;
        Some(self.editor.tree.get(view_id).doc)
    }
}

impl HelixFilesEditorElement {
    pub(crate) fn new(
        state: SharedHelixFilesEditor,
        focus: &FocusHandle,
        is_focused: bool,
        style: TextStyle,
        palette: HelixFilesEditorPalette,
    ) -> Self {
        Self {
            state,
            focus: focus.clone(),
            is_focused,
            style,
            palette,
        }
    }
}

impl IntoElement for HelixFilesEditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl InteractiveElement for HelixFilesEditorElement {
    fn interactivity(&mut self) -> &mut Interactivity {
        unreachable!("HelixFilesEditorElement does not use GPUI interactivity forwarding")
    }
}

impl StatefulInteractiveElement for HelixFilesEditorElement {}

impl Element for HelixFilesEditorElement {
    type RequestLayoutState = ();
    type PrepaintState = DocumentLayout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        cx: &mut WindowContext,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = StyleRefinement::default();
        style.size.width = Some(relative(1.).into());
        style.size.height = Some(relative(1.).into());
        (cx.request_layout(style.style(), []), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        cx: &mut WindowContext,
    ) -> Self::PrepaintState {
        let font_id = cx.text_system().resolve_font(&self.style.font());
        let font_size = self.style.font_size.to_pixels(cx.rem_size());
        let line_height = self.style.line_height_in_pixels(cx.rem_size());
        let cell_width = cx
            .text_system()
            .advance(font_id, font_size, 'm')
            .unwrap_or_else(|| size(px(8.0), px(0.0)))
            .width;
        let columns = (bounds.size.width / cell_width).floor().max(1.0) as usize;
        let rows = (bounds.size.height / line_height).floor().max(1.0) as usize;

        let gutter_columns = self
            .state
            .borrow()
            .runtime
            .as_ref()
            .and_then(Result::ok)
            .and_then(|runtime| runtime.current_doc_id().and_then(|doc_id| runtime.editor.document(doc_id)))
            .map(|doc| doc.text().len_lines().max(1).to_string().len() + 1)
            .unwrap_or(4);
        let editor_columns = columns.saturating_sub(gutter_columns + 2).max(1);

        if let Some(runtime) = self.state.borrow_mut().runtime.as_mut().and_then(Result::ok) {
            runtime.editor.resize(Rect {
                x: 0,
                y: 0,
                width: editor_columns.min(u16::MAX as usize) as u16,
                height: rows.min(u16::MAX as usize) as u16,
            });
        }

        DocumentLayout {
            rows,
            line_height,
            font_size,
            cell_width,
            gutter_columns,
            hitbox: Some(cx.insert_hitbox(bounds, false)),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        cx: &mut WindowContext,
    ) {
        let focus = self.focus.clone();
        if let Some(hitbox) = layout.hitbox.as_ref() {
            if hitbox.is_hovered(cx) {
                cx.on_mouse_event(move |event: &MouseDownEvent, _, cx| {
                    if event.button == MouseButton::Left {
                        cx.focus(&focus);
                    }
                });
            }
        }

        cx.with_content_mask(Some(ContentMask { bounds }), |cx| {
            cx.paint_quad(fill(bounds, self.palette.background));

            let state_ref = self.state.borrow();
            let Some(runtime) = state_ref.runtime.as_ref().and_then(Result::ok) else {
                return;
            };
            let Some(view_id) = runtime.current_view_id() else {
                return;
            };
            let Some(doc_id) = runtime.current_doc_id() else {
                return;
            };
            let Some(document) = runtime.editor.document(doc_id) else {
                return;
            };
            let view = runtime.editor.tree.get(view_id);
            let text = document.text();
            let total_lines = text.len_lines();
            let anchor = view.offset.anchor;
            let first_row = text.char_to_line(anchor.min(text.len_chars()));
            let last_row = (first_row + layout.rows + 1).min(total_lines);
            let end_char = text.line_to_char(last_row.min(total_lines));
            let text_view = text.slice(anchor..end_char);
            let visible_text: SharedString = RopeWrapper(text_view).into();
            let syntax_runs = syntax_runs(
                &runtime.editor,
                document,
                view,
                anchor,
                layout.rows.min(u16::MAX as usize) as u16,
                end_char,
                self.palette.default_foreground,
                self.style.font(),
            );
            let shaped_lines = cx
                .text_system()
                .shape_text(visible_text, layout.font_size, &syntax_runs, None)
                .unwrap_or_default();

            paint_line_numbers(
                cx,
                bounds.origin,
                layout,
                first_row,
                last_row,
                total_lines,
                self.palette,
                self.style.font(),
            );

            let gutter_x = bounds.origin.x + (layout.cell_width * layout.gutter_columns as f32);
            cx.paint_quad(fill(
                Bounds {
                    origin: point(gutter_x + px(4.0), bounds.origin.y),
                    size: size(px(1.0), bounds.size.height),
                },
                self.palette.border,
            ));

            let mut origin = bounds.origin;
            origin.x += px(10.0) + (layout.cell_width * (layout.gutter_columns as f32 + 1.0));
            origin.y += px(1.0);
            for line in shaped_lines {
                let _ = line.paint(origin, layout.line_height, cx);
                origin.y += layout.line_height;
            }

            if self.is_focused {
                let (_, cursor_kind) = runtime.editor.cursor();
                let primary_idx = document
                    .selection(view_id)
                    .primary()
                    .cursor(text.slice(..));
                if let Some(position) = view.screen_coords_at_pos(document, text.slice(..), primary_idx) {
                    let origin_y = layout.line_height * position.row as f32;
                    let origin_x = layout.cell_width * position.col as f32;
                    let cursor_color = runtime
                        .editor
                        .theme
                        .get("ui.cursor.primary")
                        .bg
                        .and_then(color_to_hsla)
                        .unwrap_or(self.palette.default_foreground);
                    let cursor_bounds = cursor_bounds(
                        point(origin_x, origin_y),
                        cursor_kind,
                        layout.cell_width,
                        layout.line_height,
                        point(
                            bounds.origin.x + px(10.0) + (layout.cell_width * (layout.gutter_columns as f32 + 1.0)),
                            bounds.origin.y + px(1.0),
                        ),
                    );
                    let mut cursor_fill = cursor_color;
                    cursor_fill.a = 0.5;
                    cx.paint_quad(fill(cursor_bounds, cursor_fill));
                }
            }
        });
    }
}

impl<'a> From<RopeWrapper<'a>> for SharedString {
    fn from(value: RopeWrapper<'a>) -> Self {
        let cow: Cow<'_, str> = value.0.into();
        cow.to_string().into()
    }
}

impl<H: Iterator<Item = HighlightEvent>> Iterator for StyleIter<'_, H> {
    type Item = (Style, usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.highlight_iter.next() {
            match event {
                HighlightEvent::HighlightStart(highlight) => self.active_highlights.push(highlight),
                HighlightEvent::HighlightEnd => {
                    self.active_highlights.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if start == end {
                        continue;
                    }
                    let style = self
                        .active_highlights
                        .iter()
                        .fold(self.text_style, |acc, highlight| {
                            acc.patch(self.theme.highlight(highlight.0))
                        });
                    return Some((style, start, end));
                }
            }
        }
        None
    }
}

fn syntax_runs(
    editor: &Editor,
    doc: &Document,
    view: &View,
    anchor: usize,
    lines: u16,
    end_char: usize,
    default_foreground: Hsla,
    font: Font,
) -> Vec<TextRun> {
    let syntax_highlights = EditorView::doc_syntax_highlights(doc, anchor, lines, &editor.theme);
    let mut styles = StyleIter {
        text_style: Style::default(),
        active_highlights: Vec::with_capacity(64),
        highlight_iter: syntax_highlights,
        theme: &editor.theme,
    };
    let mut current_span = styles.next().unwrap_or((Style::default(), 0, usize::MAX));
    let mut position = anchor;
    let mut runs = Vec::new();

    loop {
        let (style, span_start, span_end) = current_span;
        let effective_style = if position < span_start {
            Style::default()
        } else {
            style
        };
        let fg = effective_style
            .fg
            .and_then(color_to_hsla)
            .unwrap_or(default_foreground);
        let bg = effective_style.bg.and_then(color_to_hsla);
        let len = if position < span_start {
            span_start.saturating_sub(position)
        } else {
            span_end.saturating_sub(position)
        };
        let len = len.min(end_char.saturating_sub(position));
        if len == 0 {
            break;
        }
        runs.push(TextRun {
            len,
            font: font.clone(),
            color: fg,
            background_color: bg,
            underline: None,
            strikethrough: None,
        });
        position += len;
        if position >= end_char {
            break;
        }
        if position >= span_end {
            current_span = styles.next().unwrap_or((Style::default(), position, usize::MAX));
        }
    }

    if runs.is_empty() {
        runs.push(TextRun {
            len: end_char.saturating_sub(anchor),
            font,
            color: default_foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    let _ = view;
    runs
}

fn paint_line_numbers(
    cx: &mut WindowContext,
    origin: Point<Pixels>,
    layout: &DocumentLayout,
    first_row: usize,
    last_row: usize,
    total_lines: usize,
    palette: HelixFilesEditorPalette,
    font: Font,
) {
    let width = total_lines.max(1).to_string().len();
    let mut y = origin.y + px(1.0);
    for line_number in first_row..last_row {
        let color = if line_number == first_row {
            palette.current_line_number
        } else {
            palette.line_number
        };
        let text = format!("{:>width$}", line_number + 1, width = width);
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        if let Ok(shaped) = cx
            .text_system()
            .shape_line(text.into(), layout.font_size, &[run])
        {
            let _ = shaped.paint(origin + point(px(0.0), y - origin.y), layout.line_height, cx);
        }
        y += layout.line_height;
    }
}

fn cursor_bounds(
    origin: Point<Pixels>,
    kind: CursorKind,
    cell_width: Pixels,
    line_height: Pixels,
    offset: Point<Pixels>,
) -> Bounds<Pixels> {
    match kind {
        CursorKind::Bar => Bounds {
            origin: origin + offset,
            size: size(px(2.0), line_height),
        },
        CursorKind::Block => Bounds {
            origin: origin + offset,
            size: size(cell_width, line_height),
        },
        CursorKind::Underline => Bounds {
            origin: origin + offset + point(Pixels::ZERO, line_height - px(2.0)),
            size: size(cell_width, px(2.0)),
        },
        CursorKind::Hidden => Bounds {
            origin: origin + offset,
            size: size(Pixels::ZERO, Pixels::ZERO),
        },
    }
}

fn translate_key(keystroke: &Keystroke) -> Option<KeyEvent> {
    use helix_view::keyboard::{KeyCode, KeyModifiers};

    let mut modifiers = KeyModifiers::NONE;
    if keystroke.modifiers.alt {
        modifiers |= KeyModifiers::ALT;
    }
    if keystroke.modifiers.control {
        modifiers |= KeyModifiers::CONTROL;
    }
    if keystroke.modifiers.shift {
        modifiers |= KeyModifiers::SHIFT;
    }
    if keystroke.modifiers.platform {
        modifiers |= KeyModifiers::SUPER;
    }

    let key = keystroke.ime_key.as_ref().unwrap_or(&keystroke.key);
    let code = match key.as_str() {
        "backspace" => KeyCode::Backspace,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "tab" => KeyCode::Tab,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        value => {
            let mut chars = value.chars();
            let first = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(first)
        }
    };

    Some(KeyEvent { code, modifiers })
}

fn color_to_hsla(color: Color) -> Option<Hsla> {
    match color {
        Color::Reset => None,
        Color::Black => Some(black()),
        Color::Red => Some(red()),
        Color::Green => Some(green()),
        Color::Yellow => Some(yellow()),
        Color::Blue => Some(blue()),
        Color::Magenta => Some(hsla(0.82, 0.72, 0.68, 1.0)),
        Color::Cyan => Some(hsla(0.52, 0.70, 0.62, 1.0)),
        Color::Gray => Some(hsla(0.0, 0.0, 0.55, 1.0)),
        Color::LightRed => Some(hsla(0.0, 0.85, 0.68, 1.0)),
        Color::LightGreen => Some(hsla(0.34, 0.80, 0.62, 1.0)),
        Color::LightYellow => Some(hsla(0.15, 0.90, 0.67, 1.0)),
        Color::LightBlue => Some(hsla(0.60, 0.85, 0.70, 1.0)),
        Color::LightMagenta => Some(hsla(0.82, 0.80, 0.75, 1.0)),
        Color::LightCyan => Some(hsla(0.52, 0.82, 0.72, 1.0)),
        Color::LightGray => Some(hsla(0.0, 0.0, 0.78, 1.0)),
        Color::White => Some(white()),
        Color::Rgb(r, g, b) => Some(rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)).into()),
        Color::Indexed(_) => None,
    }
}

fn ensure_helix_loader_initialized() {
    static HELIX_LOADER_INIT: OnceLock<()> = OnceLock::new();
    HELIX_LOADER_INIT.get_or_init(|| {
        helix_loader::initialize_config_file(None);
        helix_loader::initialize_log_file(None);
    });
}

fn ensure_helix_events_registered() {
    static HELIX_EVENTS_INIT: OnceLock<()> = OnceLock::new();
    HELIX_EVENTS_INIT.get_or_init(helix_term::events::register);
}
