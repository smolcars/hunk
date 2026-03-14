use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use arc_swap::{ArcSwap, access::Map};
use gpui::*;
use helix_core::coords_at_pos;
use helix_core::{Range, Selection, Transaction, movement::Direction};
use helix_term::commands;
use helix_term::compositor::{Component, Compositor, Context as CompositorContext, EventResult};
use helix_term::config::Config as HelixConfig;
use helix_term::job::Jobs;
use helix_term::keymap::Keymaps;
use helix_term::ui::EditorView;
use helix_view::editor::{Action, CursorShapeConfig};
use helix_view::graphics::Rect;
use helix_view::handlers::completion::{CompletionEvent, CompletionHandler};
use helix_view::handlers::word_index;
use helix_view::handlers::{AutoSaveEvent, Handlers};
use helix_view::input::{Event as HelixEvent, KeyEvent};
use helix_view::keyboard::{KeyCode, KeyModifiers};
use helix_view::view::ViewPosition;
use helix_view::{Document, DocumentId, Editor, ViewId, theme as helix_theme};
use tracing::warn;
mod highlight;
mod paint;
mod runtime_env;
mod selection;
mod theme;

use self::highlight::syntax_runs;
use self::paint::{
    CursorPaintParams, animated_cursor_kind, clamp_to_bounds, mouse_text_position, paint_cursors,
    paint_line_numbers, paint_selection_backgrounds, palette_text_width, visible_row_char_range,
};
use self::runtime_env::{ensure_helix_loader_initialized, with_tokio_runtime};
use self::selection::{line_selection_range, word_selection_range};
use self::theme::load_hunk_helix_theme;

pub(crate) type SharedHelixFilesEditor = Rc<RefCell<HelixFilesEditor>>;

pub(crate) fn initialize_helix_runtime_environment() {
    runtime_env::initialize_helix_runtime_environment();
}

pub(crate) struct HelixFilesEditor {
    runtime: Option<HelixRuntime>,
    active_path: Option<PathBuf>,
    view_state_by_path: BTreeMap<PathBuf, HelixDocumentViewState>,
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
    theme_loader: Arc<helix_theme::Loader>,
    is_dark_theme: bool,
}

#[derive(Clone)]
struct HelixDocumentViewState {
    selection: Selection,
    view_offset: ViewPosition,
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

impl HelixFilesEditor {
    pub(crate) fn new() -> Self {
        Self {
            runtime: None,
            active_path: None,
            view_state_by_path: BTreeMap::new(),
        }
    }
    pub(crate) fn open_document(&mut self, path: &Path, contents: &str) -> Result<()> {
        if self.runtime.is_none() {
            match HelixRuntime::new() {
                Ok(runtime) => self.runtime = Some(runtime),
                Err(err) => return Err(err),
            }
        }
        self.capture_active_view_state();
        let Some(runtime) = self.runtime.as_mut() else {
            anyhow::bail!("helix runtime was not initialized")
        };
        let open_action = if runtime.current_view_id().is_some() {
            Action::Replace
        } else {
            Action::VerticalSplit
        };
        runtime
            .replace_document(path, contents, open_action)
            .with_context(|| format!("failed to open {} in Helix editor", path.display()))?;
        self.active_path = Some(path.to_path_buf());
        self.restore_view_state(path);
        Ok(())
    }
    pub(crate) fn clear(&mut self) {
        self.capture_active_view_state();
        self.active_path = None;
        self.view_state_by_path.clear();
    }
    pub(crate) fn shutdown(&mut self) {
        self.clear();
        self.runtime = None;
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
    pub(crate) fn copy_selection_text(&self) -> Option<String> {
        self.active_path.as_ref()?;
        let runtime = self.runtime.as_ref()?;
        let view_id = runtime.current_view_id()?;
        let doc_id = runtime.current_doc_id()?;
        let document = runtime.editor.document(doc_id)?;
        let text = document.text().slice(..);
        let fragments: Vec<Cow<'_, str>> = document.selection(view_id).fragments(text).collect();
        if fragments.iter().all(|fragment| fragment.is_empty()) {
            return None;
        }
        Some(
            fragments
                .into_iter()
                .map(Cow::into_owned)
                .collect::<Vec<_>>()
                .join(document.line_ending.as_str()),
        )
    }
    pub(crate) fn cut_selection_text(&mut self) -> Option<String> {
        let copied = self.copy_selection_text()?;
        let runtime = self.runtime.as_mut()?;
        let scrolloff = runtime.editor.config().scrolloff;
        let (view, doc) = helix_view::current!(runtime.editor);
        let selection = doc.selection(view.id).clone();
        let transaction = Transaction::delete_by_selection(doc.text(), &selection, |range| {
            (range.from(), range.to())
        });
        doc.apply(&transaction, view.id);
        doc.append_changes_to_history(view);
        view.ensure_cursor_in_view(doc, scrolloff);
        if matches!(runtime.editor.mode(), helix_view::document::Mode::Select) {
            runtime.editor.enter_normal_mode();
        }
        Some(copied)
    }
    pub(crate) fn paste_text(&mut self, text: &str) -> bool {
        if text.is_empty() || self.active_path.is_none() {
            return false;
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let scrolloff = runtime.editor.config().scrolloff;
        let pasted: Arc<str> = Arc::from(text);
        let (view, doc) = helix_view::current!(runtime.editor);
        let selection = doc.selection(view.id).clone();
        let transaction = Transaction::change_by_selection(doc.text(), &selection, |range| {
            (range.from(), range.to(), Some(pasted.as_ref().into()))
        });
        doc.apply(&transaction, view.id);
        doc.append_changes_to_history(view);
        view.ensure_cursor_in_view(doc, scrolloff);
        true
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
        with_tokio_runtime(|| {
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
        })
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
    pub(crate) fn sync_theme(&mut self, is_dark: bool) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.sync_theme(is_dark);
    }
    pub(crate) fn handle_mouse_down(
        &mut self,
        position: Point<Pixels>,
        layout: &DocumentLayout,
        extend: bool,
        add_cursor: bool,
        click_count: usize,
    ) -> bool {
        let Some((runtime, view_id, doc_id, pos)) = self.mouse_target(position, layout) else {
            return false;
        };
        runtime.editor.focus(view_id);
        let Some(doc) = runtime.editor.document_mut(doc_id) else {
            return false;
        };
        let text = doc.text().slice(..);
        if add_cursor {
            let selection = doc.selection(view_id).clone();
            doc.set_selection(view_id, selection.push(Range::point(pos)));
            runtime.editor.ensure_cursor_in_view(view_id);
            return true;
        }
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
        let Some(doc) = runtime.editor.document_mut(doc_id) else {
            return false;
        };
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

    fn capture_active_view_state(&mut self) {
        let Some(path) = self.active_path.clone() else {
            return;
        };
        let Some(runtime) = self.runtime.as_ref() else {
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
        self.view_state_by_path.insert(
            path,
            HelixDocumentViewState {
                selection: document.selection(view_id).clone(),
                view_offset: document.view_offset(view_id),
            },
        );
    }

    fn restore_view_state(&mut self, path: &Path) {
        let Some(saved_state) = self.view_state_by_path.get(path).cloned() else {
            return;
        };
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let Some(view_id) = runtime.current_view_id() else {
            return;
        };
        let Some(doc_id) = runtime.current_doc_id() else {
            return;
        };
        let Some(document) = runtime.editor.document_mut(doc_id) else {
            return;
        };
        let text = document.text().slice(..);
        let selection = saved_state.selection.ensure_invariants(text);
        let mut view_offset = saved_state.view_offset;
        view_offset.anchor = view_offset.anchor.min(text.len_chars());
        view_offset.vertical_offset = view_offset
            .vertical_offset
            .min(text.char_to_line(view_offset.anchor));
        document.set_selection(view_id, selection);
        document.set_view_offset(view_id, view_offset);
    }
}

impl Drop for HelixFilesEditor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl HelixRuntime {
    fn new() -> Result<Self> {
        ensure_helix_loader_initialized();
        ensure_helix_events_registered();

        let mut config = HelixConfig::load_default().unwrap_or_default();
        config.editor.lsp.enable = false;
        config.editor.cursor_shape = default_hunk_cursor_shape();

        let mut theme_parent_dirs = vec![helix_loader::config_dir()];
        theme_parent_dirs.extend(helix_loader::runtime_dirs().iter().cloned());
        let theme_loader = Arc::new(helix_theme::Loader::new(&theme_parent_dirs));
        let is_dark_theme = true;
        let theme = load_hunk_helix_theme(&theme_loader, is_dark_theme);

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
        let (document_colors, _document_colors_rx) = tokio::sync::mpsc::channel(1);
        let (pull_diagnostics, _pull_diagnostics_rx) = tokio::sync::mpsc::channel(1);
        let (pull_all_documents_diagnostics, _pull_all_documents_diagnostics_rx) =
            tokio::sync::mpsc::channel(1);
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
            theme_loader,
            is_dark_theme,
        })
    }

    fn sync_theme(&mut self, is_dark: bool) {
        if self.is_dark_theme == is_dark {
            return;
        }
        self.editor
            .set_theme(load_hunk_helix_theme(&self.theme_loader, is_dark));
        self.is_dark_theme = is_dark;
    }

    fn current_view_id(&self) -> Option<ViewId> {
        let view_id = self.editor.tree.focus;
        self.editor.tree.contains(view_id).then_some(view_id)
    }

    fn current_doc_id(&self) -> Option<DocumentId> {
        let view_id = self.current_view_id()?;
        Some(self.editor.tree.get(view_id).doc)
    }

    fn replace_document(&mut self, path: &Path, contents: &str, action: Action) -> Result<()> {
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(doc_id) = self.editor.document_id_by_path(&canonical_path) {
            self.editor.close_document(doc_id, true).map_err(|error| {
                let _ = error;
                anyhow::anyhow!("failed to close stale Helix document {}", path.display())
            })?;
        }

        let doc_id = self.editor.new_file(action);
        let (view, doc) = helix_view::current!(self.editor);
        let transaction = Transaction::change(
            doc.text(),
            std::iter::once((0, doc.text().len_chars(), Some(contents.into()))),
        );
        doc.apply(&transaction, view.id);
        doc.set_selection(view.id, Selection::point(0));
        doc.append_changes_to_history(view);

        self.editor.set_doc_path(doc_id, &canonical_path);
        if let Some(document) = self.editor.document_mut(doc_id) {
            document.reset_modified();
            document.pickup_last_saved_time();
        }
        Ok(())
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
                    event.modifiers.alt,
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
            let content_origin = point(
                bounds.origin.x
                    + px(10.0)
                    + (layout.cell_width * (layout.gutter_columns as f32 + 1.0)),
                bounds.origin.y + px(1.0),
            );
            let content_width = (layout.hitbox.bounds.right() - content_origin.x).max(Pixels::ZERO);
            let visible_columns = ((content_width / layout.cell_width).ceil() as usize).max(1);

            let current_line = document
                .selection(view_id)
                .primary()
                .cursor(text.slice(..))
                .min(text.len_chars());
            let current_line = text.char_to_line(current_line);

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

            let mut row_origin = content_origin;
            for row in 0..layout.rows {
                if let Some((row_start, row_end)) = visible_row_char_range(
                    document,
                    view,
                    text.slice(..),
                    row,
                    layout.rows,
                    visible_columns,
                ) {
                    let visible_text: SharedString =
                        RopeWrapper(text.slice(row_start..row_end)).into();
                    let syntax_runs = syntax_runs(
                        &runtime.editor,
                        document,
                        row_start,
                        1,
                        row_end,
                        self.palette.default_foreground,
                        self.style.font(),
                    );
                    let line = window.text_system().shape_line(
                        visible_text,
                        layout.font_size,
                        &syntax_runs,
                        None,
                    );
                    let _ = line.paint(
                        row_origin,
                        layout.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                row_origin.y += layout.line_height;
            }

            if self.is_focused {
                let (_, cursor_kind) = runtime.editor.cursor();
                if matches!(cursor_kind, helix_view::graphics::CursorKind::Bar) {
                    window.request_animation_frame();
                }
                let cursor_kind = animated_cursor_kind(cursor_kind);
                paint_cursors(
                    window,
                    CursorPaintParams {
                        document,
                        view,
                        text: text.slice(..),
                        content_origin,
                        cell_width: layout.cell_width,
                        line_height: layout.line_height,
                        kind: cursor_kind,
                        color: self.palette.cursor,
                    },
                );
            }

            if layout.hitbox.is_hovered(window) {
                window.set_cursor_style(CursorStyle::IBeam, &layout.hitbox);
            }
        });
    }
}

fn default_hunk_cursor_shape() -> CursorShapeConfig {
    toml::from_str(
        r#"
normal = "block"
insert = "bar"
select = "block"
"#,
    )
    .unwrap_or_else(|error| {
        warn!("failed to load default Helix cursor shape config: {error:#}");
        CursorShapeConfig::default()
    })
}

impl<'a> From<RopeWrapper<'a>> for SharedString {
    fn from(value: RopeWrapper<'a>) -> Self {
        let cow: Cow<'_, str> = value.0.into();
        cow.to_string().into()
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

fn translate_key(keystroke: &Keystroke) -> Option<KeyEvent> {
    let mut modifiers = KeyModifiers::NONE;
    let has_non_shift_modifier = keystroke.modifiers.control
        || keystroke.modifiers.alt
        || keystroke.modifiers.platform
        || keystroke.modifiers.function;
    let printable_key_char = (!has_non_shift_modifier)
        .then_some(keystroke.key_char.as_ref())
        .flatten()
        .filter(|value| value.chars().count() == 1);
    if printable_key_char.is_none() && keystroke.modifiers.shift {
        modifiers |= KeyModifiers::SHIFT;
    }
    if keystroke.modifiers.control {
        modifiers |= KeyModifiers::CONTROL;
    }
    if keystroke.modifiers.alt {
        modifiers |= KeyModifiers::ALT;
    }
    if keystroke.modifiers.platform {
        modifiers |= KeyModifiers::SUPER;
    }

    let key = printable_key_char.unwrap_or(&keystroke.key);
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
fn ensure_helix_events_registered() {
    static HELIX_EVENTS_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    HELIX_EVENTS_INIT.get_or_init(helix_term::events::register);
}
