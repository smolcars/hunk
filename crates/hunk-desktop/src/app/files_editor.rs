use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use anyhow::{Context as _, Result};
use arc_swap::{ArcSwap, access::Map};
use gpui::*;
use helix_core::coords_at_pos;
use helix_core::syntax::{HighlightEvent, Highlighter};
use helix_core::textobject::{TextObject, textobject_word};
use helix_core::{Range, Selection, movement::Direction};
use helix_term::commands;
use helix_term::compositor::{Component, Compositor, Context as CompositorContext, EventResult};
use helix_term::config::Config as HelixConfig;
use helix_term::job::Jobs;
use helix_term::keymap::Keymaps;
use helix_term::ui::EditorView;
use helix_view::editor::Action;
use helix_view::graphics::{Color, Rect, Style as HelixStyle};
use helix_view::handlers::completion::{CompletionEvent, CompletionHandler};
use helix_view::handlers::lsp::{
    DocumentColorsEvent, PullAllDocumentsDiagnosticsEvent, PullDiagnosticsEvent,
};
use helix_view::handlers::word_index;
use helix_view::handlers::{AutoSaveEvent, Handlers};
use helix_view::input::{Event as HelixEvent, KeyEvent};
use helix_view::keyboard::{KeyCode, KeyModifiers};
use helix_view::{Document, DocumentId, Editor, Theme, ViewId, theme};
mod paint;

use self::paint::{
    clamp_to_bounds, cursor_bounds, mouse_text_position, paint_current_line_background,
    paint_line_numbers, paint_selection_backgrounds, palette_text_width,
};
pub(crate) type SharedHelixFilesEditor = Rc<RefCell<HelixFilesEditor>>;

pub(crate) struct HelixFilesEditor {
    runtime: Option<HelixRuntime>,
    active_path: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct HelixStatusSnapshot {
    pub(crate) mode: &'static str,
    pub(crate) language: String,
    pub(crate) position: String,
    pub(crate) selection: String,
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
    pub(crate) current_line_background: Hsla,
    pub(crate) selection_background: Hsla,
    pub(crate) cursor: Hsla,
}

#[derive(Debug, Clone)]
pub(crate) struct DocumentLayout {
    rows: usize,
    line_height: Pixels,
    font_size: Pixels,
    cell_width: Pixels,
    gutter_columns: usize,
    hitbox: Hitbox,
}

impl DocumentLayout {
    fn content_origin_x(&self) -> Pixels {
        self.hitbox.bounds.origin.x
            + px(10.0)
            + (self.cell_width * (self.gutter_columns as f32 + 1.0))
    }

    fn is_in_gutter(&self, position: Point<Pixels>) -> bool {
        position.x < self.content_origin_x()
    }
}

struct LineNumberPaintParams {
    origin: Point<Pixels>,
    first_row: usize,
    last_row: usize,
    current_line: usize,
    digits: usize,
    palette: HelixFilesEditorPalette,
    font: Font,
}

struct RopeWrapper<'a>(helix_core::ropey::RopeSlice<'a>);

struct SyntaxStyleIter<'h, 'r, 't> {
    inner: Option<Highlighter<'h>>,
    text: helix_core::ropey::RopeSlice<'r>,
    pos: usize,
    theme: &'t Theme,
    text_style: HelixStyle,
    style: HelixStyle,
}

impl HelixFilesEditor {
    pub(crate) fn new() -> Self {
        Self {
            runtime: None,
            active_path: None,
        }
    }
    pub(crate) fn open_path(&mut self, path: &Path) -> Result<()> {
        if self.runtime.is_none() {
            match HelixRuntime::new() {
                Ok(runtime) => self.runtime = Some(runtime),
                Err(err) => return Err(err),
            }
        }
        let runtime = self
            .runtime
            .as_mut()
            .expect("runtime is initialized before use");
        let open_action = if runtime.current_view_id().is_some() {
            Action::Replace
        } else {
            Action::VerticalSplit
        };
        let open_result = with_tokio_runtime(|| runtime.editor.open(path, open_action));
        open_result
            .with_context(|| format!("failed to open {} in Helix editor", path.display()))?;
        self.active_path = Some(path.to_path_buf());
        Ok(())
    }
    pub(crate) fn clear(&mut self) {
        self.active_path = None;
    }
    pub(crate) fn is_dirty(&self) -> bool {
        if self.active_path.is_none() {
            return false;
        }
        let Some(runtime) = self.runtime.as_ref() else {
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
        self.active_path.as_ref()?;
        let runtime = self.runtime.as_ref()?;
        let doc_id = runtime.current_doc_id()?;
        runtime
            .editor
            .document(doc_id)
            .map(|doc| doc.text().slice(..).to_string())
    }
    pub(crate) fn status_snapshot(&self) -> Option<HelixStatusSnapshot> {
        self.active_path.as_ref()?;
        let runtime = self.runtime.as_ref()?;
        let view_id = runtime.current_view_id()?;
        let doc_id = runtime.current_doc_id()?;
        let doc = runtime.editor.document(doc_id)?;
        let text = doc.text().slice(..);
        let selection = doc.selection(view_id);
        let cursor = selection.primary().cursor(text);
        let coords = coords_at_pos(text, cursor);
        let selection_len = selection.primary().len();
        Some(HelixStatusSnapshot {
            mode: match runtime.editor.mode() {
                helix_view::document::Mode::Insert => "INSERT",
                helix_view::document::Mode::Select => "SELECT",
                helix_view::document::Mode::Normal => "NORMAL",
            },
            language: doc.language_name().unwrap_or("text").to_string(),
            position: format!(
                "Ln {}  Col {}  {} lines",
                coords.row + 1,
                coords.col + 1,
                text.len_lines()
            ),
            selection: if selection_len > 0 {
                format!("{} sel  {} cursors", selection_len, selection.len())
            } else {
                format!("{} cursors", selection.len())
            },
        })
    }
    pub(crate) fn mark_saved(&mut self) {
        if self.active_path.is_none() {
            return;
        }
        let Some(runtime) = self.runtime.as_mut() else {
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
        if self.active_path.is_none() {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(key) = translate_key(keystroke) else {
            return false;
        };
        let event = HelixEvent::Key(key);
        let mut comp_ctx = CompositorContext {
            editor: &mut runtime.editor,
            scroll: None,
            jobs: &mut runtime.jobs,
        };
        if runtime.compositor.handle_event(&event, &mut comp_ctx) {
            return true;
        }
        match runtime.view.handle_event(&event, &mut comp_ctx) {
            EventResult::Consumed(callback) => {
                if let Some(callback) = callback {
                    callback(&mut runtime.compositor, &mut comp_ctx);
                }
                true
            }
            EventResult::Ignored(_) => false,
        }
    }
    pub(crate) fn scroll_lines(
        &mut self,
        line_count: usize,
        direction: helix_core::movement::Direction,
    ) {
        if self.active_path.is_none() {
            return;
        }
        let Some(runtime) = self.runtime.as_mut() else {
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
    pub(crate) fn handle_mouse_down(
        &mut self,
        position: Point<Pixels>,
        layout: &DocumentLayout,
        extend: bool,
        click_count: usize,
    ) -> bool {
        let Some((runtime, view_id, doc_id, pos)) = self.mouse_target(position, layout) else {
            return false;
        };
        runtime.editor.focus(view_id);
        let doc = runtime
            .editor
            .document_mut(doc_id)
            .expect("current doc exists");
        let text = doc.text().slice(..);
        let range = if layout.is_in_gutter(position) || click_count >= 3 {
            line_selection_range(text, &doc.selection(view_id).primary(), pos, extend)
        } else if click_count == 2 {
            word_selection_range(text, pos)
        } else if extend {
            doc.selection(view_id).primary().put_cursor(text, pos, true)
        } else {
            Range::point(pos)
        };
        doc.set_selection(view_id, Selection::single(range.anchor, range.head));
        runtime.editor.mouse_down_range = Some(range);
        runtime.editor.ensure_cursor_in_view(view_id);
        true
    }
    pub(crate) fn handle_mouse_drag(
        &mut self,
        position: Point<Pixels>,
        layout: &DocumentLayout,
    ) -> bool {
        let Some((runtime, view_id, doc_id, pos)) = self.mouse_target(position, layout) else {
            return false;
        };
        if runtime.editor.mouse_down_range.is_none() {
            return false;
        }
        let doc = runtime
            .editor
            .document_mut(doc_id)
            .expect("current doc exists");
        let mut selection = doc.selection(view_id).clone();
        *selection.primary_mut() = selection
            .primary()
            .put_cursor(doc.text().slice(..), pos, true);
        doc.set_selection(view_id, selection);
        runtime.editor.ensure_cursor_in_view(view_id);
        true
    }
    pub(crate) fn handle_mouse_up(&mut self) -> bool {
        self.runtime
            .as_mut()
            .is_some_and(|runtime| runtime.editor.mouse_down_range.take().is_some())
    }
    fn mouse_target(
        &mut self,
        position: Point<Pixels>,
        layout: &DocumentLayout,
    ) -> Option<(&mut HelixRuntime, ViewId, DocumentId, usize)> {
        let runtime = self.runtime.as_mut()?;
        let view_id = runtime.current_view_id()?;
        let doc_id = runtime.current_doc_id()?;
        let doc = runtime.editor.document(doc_id)?;
        let view = runtime.editor.tree.get(view_id);
        let pos = mouse_text_position(view, doc, position, layout)?;
        Some((runtime, view_id, doc_id, pos))
    }
}

fn word_selection_range(text: helix_core::ropey::RopeSlice<'_>, pos: usize) -> Range {
    textobject_word(text, Range::point(pos), TextObject::Inside, 1, false)
}

fn line_selection_range(
    text: helix_core::ropey::RopeSlice<'_>,
    current: &Range,
    pos: usize,
    extend: bool,
) -> Range {
    let target_line = text.char_to_line(pos.min(text.len_chars()));
    let target_start = text.line_to_char(target_line);
    let target_end = line_end_char(text, target_line);
    if !extend {
        return Range::new(target_start, target_end);
    }

    let anchor_line = text.char_to_line(current.anchor.min(text.len_chars()));
    let anchor_start = text.line_to_char(anchor_line);
    let anchor_end = line_end_char(text, anchor_line);
    if target_line >= anchor_line {
        Range::new(anchor_start, target_end)
    } else {
        Range::new(anchor_end, target_start)
    }
}

fn line_end_char(text: helix_core::ropey::RopeSlice<'_>, line: usize) -> usize {
    let next_line = (line + 1).min(text.len_lines());
    text.line_to_char(next_line).min(text.len_chars())
}

impl HelixRuntime {
    fn new() -> Result<Self> {
        ensure_helix_loader_initialized();
        ensure_helix_events_registered();

        let mut config = HelixConfig::load_default().unwrap_or_default();
        config.editor.lsp.enable = false;

        let mut theme_parent_dirs = vec![helix_loader::config_dir()];
        theme_parent_dirs.extend(helix_loader::runtime_dirs().iter().cloned());
        let theme_loader = Arc::new(theme::Loader::new(&theme_parent_dirs));
        let theme = config
            .theme
            .as_ref()
            .and_then(|theme_config| theme_loader.load(theme_config.choose(None)).ok())
            .unwrap_or_else(|| theme_loader.default_theme(true));

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
        let (completions, _completions_rx) = tokio::sync::mpsc::channel::<CompletionEvent>(1);
        let (signature_hints, _signature_hints_rx) = tokio::sync::mpsc::channel(1);
        let (auto_save, _auto_save_rx) = tokio::sync::mpsc::channel::<AutoSaveEvent>(1);
        let (document_colors, _document_colors_rx) =
            tokio::sync::mpsc::channel::<DocumentColorsEvent>(1);
        let (pull_diagnostics, _pull_diagnostics_rx) =
            tokio::sync::mpsc::channel::<PullDiagnosticsEvent>(1);
        let (pull_all_documents_diagnostics, _pull_all_documents_diagnostics_rx) =
            tokio::sync::mpsc::channel::<PullAllDocumentsDiagnosticsEvent>(1);
        let mut editor = with_tokio_runtime(|| {
            let handlers = Handlers {
                completions: CompletionHandler::new(completions),
                signature_hints,
                auto_save,
                document_colors,
                word_index: word_index::Handler::spawn(),
                pull_diagnostics,
                pull_all_documents_diagnostics,
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
            editor
        });
        editor.set_theme(theme);

        let keys = Box::new(Map::new(Arc::clone(&config), |config: &HelixConfig| {
            &config.keys
        }));

        Ok(Self {
            editor,
            compositor: Compositor::new(area),
            view: EditorView::new(Keymaps::new(keys)),
            jobs: Jobs::new(),
        })
    }

    fn current_view_id(&self) -> Option<ViewId> {
        let view_id = self.editor.tree.focus;
        self.editor.tree.contains(view_id).then_some(view_id)
    }

    fn current_doc_id(&self) -> Option<DocumentId> {
        let view_id = self.current_view_id()?;
        Some(self.editor.tree.get(view_id).doc)
    }
}

impl HelixFilesEditorElement {
    pub(crate) fn new(
        state: SharedHelixFilesEditor,
        is_focused: bool,
        style: TextStyle,
        palette: HelixFilesEditorPalette,
    ) -> Self {
        Self {
            state,
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

impl Element for HelixFilesEditorElement {
    type RequestLayoutState = ();
    type PrepaintState = DocumentLayout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = gpui::Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let font_id = window.text_system().resolve_font(&self.style.font());
        let font_size = self.style.font_size.to_pixels(window.rem_size());
        let line_height = self.style.line_height_in_pixels(window.rem_size());
        let cell_width = window
            .text_system()
            .advance(font_id, font_size, 'm')
            .map(|size| size.width)
            .unwrap_or_else(|_| px(8.0));
        let columns = (bounds.size.width / cell_width).floor().max(1.0) as usize;
        let rows = (bounds.size.height / line_height).floor().max(1.0) as usize;

        let gutter_columns = self
            .state
            .borrow()
            .runtime
            .as_ref()
            .and_then(|runtime| {
                runtime
                    .current_doc_id()
                    .and_then(|doc_id| runtime.editor.document(doc_id))
            })
            .map(|doc| doc.text().len_lines().max(1).to_string().len() + 1)
            .unwrap_or(4);
        let editor_columns = columns.saturating_sub(gutter_columns + 2).max(1);

        if let Some(runtime) = self.state.borrow_mut().runtime.as_mut() {
            runtime.editor.resize(Rect {
                x: 0,
                y: 0,
                width: editor_columns.min(u16::MAX as usize) as u16,
                height: rows.min(u16::MAX as usize) as u16,
            });
        }

        let _ = cx;

        DocumentLayout {
            rows,
            line_height,
            font_size,
            cell_width,
            gutter_columns,
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mouse_down_layout = layout.clone();
        let mouse_drag_layout = layout.clone();
        let mouse_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.button == gpui::MouseButton::Left
                && mouse_down_layout.hitbox.is_hovered(window)
                && mouse_state.borrow_mut().handle_mouse_down(
                    event.position,
                    &mouse_down_layout,
                    event.modifiers.shift,
                    event.click_count,
                )
            {
                window.refresh();
            }
        });
        let mouse_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.dragging()
                && mouse_state.borrow_mut().handle_mouse_drag(
                    clamp_to_bounds(event.position, mouse_drag_layout.hitbox.bounds),
                    &mouse_drag_layout,
                )
            {
                window.refresh();
            }
        });
        let mouse_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _cx| {
            if phase == DispatchPhase::Bubble
                && event.button == gpui::MouseButton::Left
                && mouse_state.borrow_mut().handle_mouse_up()
            {
                window.refresh();
            }
        });

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, self.palette.background));

            let state_ref = self.state.borrow();
            let Some(runtime) = state_ref.runtime.as_ref() else {
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
            let view_offset = document.view_offset(view_id);
            let first_row = text.char_to_line(view_offset.anchor.min(text.len_chars()));
            let total_lines = text.len_lines();
            let last_row = (first_row + layout.rows + 1).min(total_lines);
            let end_char = text.line_to_char(last_row.min(total_lines));
            let text_view = text.slice(view_offset.anchor..end_char);
            let visible_text: SharedString = RopeWrapper(text_view).into();
            let syntax_runs = syntax_runs(
                &runtime.editor,
                document,
                view_offset.anchor,
                layout.rows.min(u16::MAX as usize) as u16,
                end_char,
                self.palette.default_foreground,
                self.style.font(),
            );
            let shaped_lines = window
                .text_system()
                .shape_text(visible_text, layout.font_size, &syntax_runs, None, None)
                .unwrap_or_default();

            let current_line = document
                .selection(view_id)
                .primary()
                .cursor(text.slice(..))
                .min(text.len_chars());
            let current_line = text.char_to_line(current_line);

            paint_current_line_background(
                window,
                bounds.origin,
                layout,
                first_row,
                current_line,
                self.palette.current_line_background,
            );
            paint_selection_backgrounds(
                window,
                document,
                view,
                text.slice(..),
                bounds.origin,
                layout,
                self.palette.selection_background,
            );
            paint_line_numbers(
                window,
                cx,
                layout,
                LineNumberPaintParams {
                    origin: bounds.origin,
                    first_row,
                    last_row,
                    current_line,
                    digits: palette_text_width(total_lines),
                    palette: self.palette,
                    font: self.style.font(),
                },
            );

            let gutter_x = bounds.origin.x + (layout.cell_width * layout.gutter_columns as f32);
            window.paint_quad(fill(
                Bounds {
                    origin: point(gutter_x + px(4.0), bounds.origin.y),
                    size: size(px(1.0), bounds.size.height),
                },
                self.palette.border,
            ));

            let mut origin = point(
                bounds.origin.x
                    + px(10.0)
                    + (layout.cell_width * (layout.gutter_columns as f32 + 1.0)),
                bounds.origin.y + px(1.0),
            );
            for line in shaped_lines {
                let _ = line.paint(
                    origin,
                    layout.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                origin.y += layout.line_height;
            }

            if self.is_focused {
                let (_, cursor_kind) = runtime.editor.cursor();
                let primary_idx = document.selection(view_id).primary().cursor(text.slice(..));
                if let Some(position) =
                    view.screen_coords_at_pos(document, text.slice(..), primary_idx)
                {
                    let cursor_bounds = cursor_bounds(
                        point(
                            bounds.origin.x
                                + px(10.0)
                                + (layout.cell_width * (layout.gutter_columns as f32 + 1.0))
                                + (layout.cell_width * position.col as f32),
                            bounds.origin.y + px(1.0) + (layout.line_height * position.row as f32),
                        ),
                        cursor_kind,
                        layout.cell_width,
                        layout.line_height,
                    );
                    let mut cursor_fill = self.palette.cursor;
                    cursor_fill.a = 0.55;
                    window.paint_quad(fill(cursor_bounds, cursor_fill));
                }
            }

            if layout.hitbox.is_hovered(window) {
                window.set_cursor_style(CursorStyle::IBeam, &layout.hitbox);
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

impl<'h, 'r, 't> SyntaxStyleIter<'h, 'r, 't> {
    fn new(
        inner: Option<Highlighter<'h>>,
        text: helix_core::ropey::RopeSlice<'r>,
        theme: &'t Theme,
        text_style: HelixStyle,
    ) -> Self {
        let mut highlighter = Self {
            inner,
            text,
            pos: 0,
            theme,
            text_style,
            style: text_style,
        };
        highlighter.update_pos();
        highlighter
    }

    fn update_pos(&mut self) {
        self.pos = self
            .inner
            .as_ref()
            .map(|highlighter| {
                let next_byte_idx = highlighter.next_event_offset();
                if next_byte_idx == u32::MAX {
                    usize::MAX
                } else {
                    let bounded = (next_byte_idx as usize).min(self.text.len_bytes());
                    let mut char_idx = self.text.byte_to_char(bounded);
                    while char_idx < self.text.len_chars()
                        && self.text.char_to_byte(char_idx) < bounded
                    {
                        char_idx += 1;
                    }
                    char_idx
                }
            })
            .unwrap_or(usize::MAX);
    }

    fn advance(&mut self) {
        let Some(highlighter) = self.inner.as_mut() else {
            return;
        };

        let (event, highlights) = highlighter.advance();
        let base = match event {
            HighlightEvent::Refresh => self.text_style,
            HighlightEvent::Push => self.style,
        };
        self.style = highlights.fold(base, |acc, highlight| {
            acc.patch(self.theme.highlight(highlight))
        });
        self.update_pos();
    }
}

impl Iterator for SyntaxStyleIter<'_, '_, '_> {
    type Item = (HelixStyle, usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == usize::MAX {
            return None;
        }

        let start = self.pos;
        self.advance();
        Some((self.style, start, self.pos))
    }
}

pub(crate) fn scroll_direction_and_count(
    event: &ScrollWheelEvent,
    line_height: Pixels,
) -> Option<(Direction, usize)> {
    let delta = event.delta.pixel_delta(line_height);
    if delta.y.abs() < px(0.5) {
        return None;
    }

    Some((
        if delta.y > Pixels::ZERO {
            Direction::Backward
        } else {
            Direction::Forward
        },
        ((delta.y.abs() / line_height).ceil() as usize).max(1),
    ))
}

fn syntax_runs(
    editor: &Editor,
    doc: &Document,
    anchor: usize,
    lines: u16,
    end_char: usize,
    default_foreground: Hsla,
    font: Font,
) -> Vec<TextRun> {
    let loader = editor.syn_loader.load();
    let highlighter = EditorView::doc_syntax_highlighter(doc, anchor, lines, &loader);
    let base_text_style = editor.theme.get("ui.text");
    let default_foreground = base_text_style
        .fg
        .and_then(color_to_hsla)
        .unwrap_or(default_foreground);
    let mut styles = SyntaxStyleIter::new(
        highlighter,
        doc.text().slice(..),
        &editor.theme,
        base_text_style,
    );
    let mut current_span = styles.next().unwrap_or((base_text_style, 0, usize::MAX));
    let mut position = anchor;
    let mut runs = Vec::new();

    loop {
        let (style, span_start, span_end) = current_span;
        let effective_style = if position < span_start {
            HelixStyle::default()
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
        }
        .min(end_char.saturating_sub(position));

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
            current_span = styles
                .next()
                .unwrap_or((HelixStyle::default(), position, usize::MAX));
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

    runs
}

fn translate_key(keystroke: &Keystroke) -> Option<KeyEvent> {
    let mut modifiers = KeyModifiers::NONE;
    if keystroke.modifiers.shift {
        modifiers |= KeyModifiers::SHIFT;
    }

    let key = keystroke.key_char.as_ref().unwrap_or(&keystroke.key);
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
        Color::Rgb(r, g, b) => {
            Some(rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)).into())
        }
        Color::Indexed(index) => indexed_color_to_hsla(index),
    }
}

fn indexed_color_to_hsla(index: u8) -> Option<Hsla> {
    let (r, g, b) = match index {
        0 => (0, 0, 0),
        1 => (205, 49, 49),
        2 => (13, 188, 121),
        3 => (229, 229, 16),
        4 => (36, 114, 200),
        5 => (188, 63, 188),
        6 => (17, 168, 205),
        7 => (229, 229, 229),
        8 => (102, 102, 102),
        9 => (241, 76, 76),
        10 => (35, 209, 139),
        11 => (245, 245, 67),
        12 => (59, 142, 234),
        13 => (214, 112, 214),
        14 => (41, 184, 219),
        15 => (255, 255, 255),
        16..=231 => {
            let cube = index - 16;
            let red = cube / 36;
            let green = (cube % 36) / 6;
            let blue = cube % 6;
            (
                xterm_cube_component(red),
                xterm_cube_component(green),
                xterm_cube_component(blue),
            )
        }
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            (gray, gray, gray)
        }
    };
    Some(rgb(((r as u32) << 16) | ((g as u32) << 8) | (b as u32)).into())
}

fn xterm_cube_component(value: u8) -> u8 {
    match value {
        0 => 0,
        1 => 95,
        2 => 135,
        3 => 175,
        4 => 215,
        _ => 255,
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

fn with_tokio_runtime<T>(f: impl FnOnce() -> T) -> T {
    static HELIX_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = HELIX_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("helix helper runtime must build")
    });
    let guard = runtime.enter();
    let result = f();
    drop(guard);
    result
}
