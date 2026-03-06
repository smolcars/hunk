use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, AppContext as _, ClipboardItem, Context, Corner,
    Entity, FocusHandle, Hsla, InteractiveElement as _, IntoElement, IsZero as _, KeyBinding,
    ListAlignment, ListOffset, ListSizingBehavior, ListState, Menu, MenuItem, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, OsAction, ParentElement as _, PathPromptOptions,
    Point, Render, ScrollHandle, ScrollWheelEvent, SharedString, StatefulInteractiveElement as _,
    Styled as _, SystemMenuType, Task, TitlebarOptions, Window, WindowOptions, actions, anchored,
    deferred, div, list, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, GlobalState, Root, StyledExt as _, Theme, ThemeMode, h_flex,
    highlighter::HighlightThemeStyle,
    input::{Enter as InputEnter, InputEvent, InputState},
    menu::AppMenuBar,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    v_flex,
};
use gpui_component_assets::Assets;
use tracing::error;

use hunk_domain::config::{AppConfig, ConfigStore, KeyboardShortcuts, ThemePreference};
use hunk_domain::db::{
    CommentLineSide, CommentRecord, CommentStatus, DatabaseStore, NewComment,
    compute_comment_anchor_hash, format_comment_clipboard_blob, next_status_for_unmatched_anchor,
    now_unix_ms,
};
use hunk_domain::diff::{DiffCell, DiffCellKind, DiffRowKind, SideBySideRow};
use hunk_domain::markdown_preview::MarkdownPreviewBlock;
use hunk_domain::state::{
    AiServiceTierSelection, AppState, AppStateStore, CachedBookmarkRevisionState,
    CachedChangedFileState, CachedLocalBranchState, CachedWorkflowState,
};
use hunk_jj::jj::{
    BookmarkRevision, ChangedFile, FileStatus, LineStats, LocalBranch, RepoSnapshotFingerprint,
};

use ai_runtime::AiApprovalDecision;
use ai_runtime::AiApprovalKind;
use ai_runtime::AiConnectionState;
use ai_runtime::AiPendingApproval;
use ai_runtime::AiPendingUserInputQuestion;
use ai_runtime::AiPendingUserInputRequest;
use ai_runtime::AiSnapshot;
use ai_runtime::AiTurnSessionOverrides;
use ai_runtime::AiWorkerCommand;
use ai_runtime::AiWorkerEvent;
use ai_runtime::AiWorkerStartConfig;
use ai_runtime::spawn_ai_worker;
use data::{
    DiffRowSegmentCache, DiffStreamRowMeta, FileRowRange, RepoTreeNode, RepoTreeNodeKind,
    RepoTreeRow, WorkspaceSwitchAction, WorkspaceViewMode,
};

const FPS_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
const AUTO_REFRESH_SCROLL_DEBOUNCE: Duration = Duration::from_millis(500);
const DIFF_MONO_CHAR_WIDTH: f32 = 8.0;
const DIFF_LINE_NUMBER_MIN_DIGITS: u32 = 3;
const DIFF_LINE_NUMBER_EXTRA_PADDING: f32 = 6.0;
const DIFF_MARKER_GUTTER_WIDTH: f32 = 10.0;
const APP_BOTTOM_SAFE_INSET: f32 = 0.0;
const DIFF_BOTTOM_SAFE_INSET: f32 = APP_BOTTOM_SAFE_INSET;
const DIFF_SCROLLBAR_RIGHT_INSET: f32 = 0.0;
const DIFF_SCROLLBAR_SIZE: f32 = 16.0;
const FILE_EDITOR_MAX_BYTES: usize = 2_400_000;
const MARKDOWN_PREVIEW_DEBOUNCE: Duration = Duration::from_millis(200);
const DIFF_SEGMENT_PREFETCH_RADIUS_ROWS: usize = 120;
const DIFF_SEGMENT_PREFETCH_STEP_ROWS: usize = 24;
const DIFF_SEGMENT_PREFETCH_BATCH_ROWS: usize = 96;
const DIFF_PROGRESSIVE_BATCH_FILES: usize = 8;
const SIDEBAR_REPO_LIST_ESTIMATED_ROW_HEIGHT: f32 = 24.0;
const COMMENT_CONTEXT_RADIUS_ROWS: usize = 2;
const COMMENT_RETENTION_DAYS: i64 = 14;
const COMMENT_PREVIEW_MAX_ITEMS: usize = 64;
const COMMENT_RECONCILE_MISS_THRESHOLD: u8 = 2;
const COMMENT_FUZZY_MATCH_MIN_SCORE: i32 = 6;
const COMMENT_FUZZY_RENAME_MATCH_MIN_SCORE: i32 = 11;
const AI_TIMELINE_DEFAULT_VISIBLE_TURNS: usize = 80;
const AI_TIMELINE_TURN_PAGE_SIZE: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RepoTreePromptAction {
    CreateFile { base_dir: Option<String> },
    CreateFolder { base_dir: Option<String> },
    RenameFile { path: String },
}

#[derive(Clone)]
struct RepoTreeInlineEditState {
    action: RepoTreePromptAction,
    input_state: Entity<InputState>,
}

#[derive(Debug, Clone)]
struct RepoTreeContextMenuState {
    target_path: Option<String>,
    target_kind: RepoTreeNodeKind,
    position: Point<gpui::Pixels>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkingCopyRecoveryCandidate {
    source_revision_id: String,
    source_bookmark: String,
    switched_to_bookmark: String,
    changed_file_count: usize,
    unix_time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingBookmarkSwitch {
    source_bookmark: String,
    target_bookmark: String,
    changed_file_count: usize,
    unix_time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AiComposerDraftKey {
    Thread(String),
    Workspace(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AiComposerDraft {
    prompt: String,
    local_images: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SnapshotRefreshPriority {
    Background,
    UserInitiated,
}

impl SnapshotRefreshPriority {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::UserInitiated => "user",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SnapshotRefreshRequest {
    force: bool,
    priority: SnapshotRefreshPriority,
}

impl SnapshotRefreshRequest {
    const fn user(force: bool) -> Self {
        Self {
            force,
            priority: SnapshotRefreshPriority::UserInitiated,
        }
    }

    const fn background() -> Self {
        Self {
            force: false,
            priority: SnapshotRefreshPriority::Background,
        }
    }

    const fn background_force() -> Self {
        Self {
            force: true,
            priority: SnapshotRefreshPriority::Background,
        }
    }

    fn merge(self, other: Self) -> Self {
        Self {
            force: self.force || other.force,
            priority: if self.priority >= other.priority {
                self.priority
            } else {
                other.priority
            },
        }
    }

    fn is_more_urgent_than(self, other: Self) -> bool {
        self.priority > other.priority
            || (self.priority == other.priority && self.force && !other.force)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LineStatsRefreshScope {
    Full,
    Paths(BTreeSet<String>),
}

impl LineStatsRefreshScope {
    const fn label(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Paths(_) => "paths",
        }
    }

    fn path_count(&self) -> usize {
        match self {
            Self::Full => 0,
            Self::Paths(paths) => paths.len(),
        }
    }

    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::Paths(mut left), Self::Paths(right)) => {
                left.extend(right);
                Self::Paths(left)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct PendingLineStatsRefresh {
    repo_root: PathBuf,
    request: SnapshotRefreshRequest,
    scope: LineStatsRefreshScope,
    snapshot_epoch: usize,
    cold_start: bool,
}

impl PendingLineStatsRefresh {
    fn merge(self, newer: Self) -> Self {
        let scope = if self.repo_root == newer.repo_root {
            self.scope.merge(newer.scope)
        } else {
            newer.scope
        };
        Self {
            repo_root: newer.repo_root,
            request: self.request.merge(newer.request),
            scope,
            snapshot_epoch: newer.snapshot_epoch,
            cold_start: newer.cold_start,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AiTimelineRowSource {
    Item { item_key: String },
    Group { group_id: String },
    TurnDiff { turn_key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTimelineRow {
    id: String,
    thread_id: String,
    turn_id: String,
    last_sequence: u64,
    source: AiTimelineRowSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AiTimelineGroup {
    id: String,
    thread_id: String,
    turn_id: String,
    last_sequence: u64,
    kind: String,
    status: hunk_codex::state::ItemStatus,
    title: String,
    summary: Option<String>,
    child_row_ids: Vec<String>,
}

mod ai_rollout_fallback;
mod ai_runtime;
mod controller;
mod data;
mod data_segments;
mod highlight;
mod render;
mod workspace_view;

actions!(
    diff_viewer,
    [
        SelectNextLine,
        SelectPreviousLine,
        ExtendSelectionNextLine,
        ExtendSelectionPreviousLine,
        CopySelection,
        SelectAllDiffRows,
        NextHunk,
        PreviousHunk,
        NextFile,
        PreviousFile,
        ToggleSidebarTree,
        SwitchToFilesView,
        SwitchToReviewView,
        SwitchToGitView,
        SwitchToAiView,
        AiNewThread,
        AiInterruptSelectedTurn,
        OpenProject,
        SaveCurrentFile,
        OpenSettings,
        QuitApp,
        RepoTreeNewFile,
        RepoTreeNewFolder,
        RepoTreeRenameFile,
        RepoTreeCancelInlineEdit,
    ]
);

fn preferred_ui_font_family() -> &'static str {
    if cfg!(target_os = "macos") {
        ".SystemUIFont"
    } else if cfg!(target_os = "windows") {
        "Segoe UI"
    } else {
        "Inter"
    }
}

fn preferred_mono_font_family() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "DejaVu Sans Mono"
    }
}

fn hsla_hex(hex: &str) -> Option<Hsla> {
    Hsla::parse_hex(hex).ok()
}

fn editor_highlight_style(
    base: Option<HighlightThemeStyle>,
    fallback: HighlightThemeStyle,
    mode: ThemeMode,
) -> HighlightThemeStyle {
    let mut style = base.unwrap_or(fallback);
    if mode.is_dark() {
        style.editor_background = hsla_hex("#20252f");
        style.editor_active_line = hsla_hex("#2a3140");
        style.editor_line_number = hsla_hex("#748094");
        style.editor_active_line_number = hsla_hex("#ced7e6");
    } else {
        style.editor_background = hsla_hex("#f4f6fa");
        style.editor_active_line = hsla_hex("#e7edf7");
        style.editor_line_number = hsla_hex("#8d97a8");
        style.editor_active_line_number = hsla_hex("#4a5363");
    }
    style
}

fn apply_soft_light_theme(cx: &mut App) {
    let mut light_theme = (*Theme::global(cx).light_theme).clone();
    let fallback_highlight = Theme::global(cx).highlight_theme.style.clone();

    // Reduce eye strain in light mode by shifting from pure white to a soft off-white palette.
    light_theme.colors.accent = Some("#4f6ddf".into());
    light_theme.colors.accent_foreground = Some("#f8fbff".into());
    light_theme.colors.background = Some("#f5f6f8".into());
    light_theme.colors.list = Some("#f5f6f8".into());
    light_theme.colors.list_active = Some("#4f6ddf33".into());
    light_theme.colors.list_active_border = Some("#4f6ddf".into());
    light_theme.colors.list_hover = Some("#dce3ee".into());
    light_theme.colors.popover = Some("#f5f6f8".into());
    light_theme.colors.table = Some("#f5f6f8".into());
    light_theme.colors.sidebar = Some("#f5f6f8".into());
    light_theme.colors.title_bar = Some("#f5f6f8".into());
    light_theme.colors.list_even = Some("#f1f2f5".into());
    light_theme.colors.list_head = Some("#eef0f4".into());
    light_theme.colors.secondary = Some("#eceef3".into());
    light_theme.colors.secondary_hover = Some("#dde4ef".into());
    light_theme.colors.secondary_active = Some("#d3dbe8".into());
    light_theme.colors.muted = Some("#e9ecf2".into());
    light_theme.colors.muted_foreground = Some("#616977".into());
    light_theme.colors.border = Some("#d2d8e3".into());
    light_theme.font_family = Some(preferred_ui_font_family().into());
    light_theme.font_size = Some(14.0);
    light_theme.mono_font_family = Some(preferred_mono_font_family().into());
    light_theme.mono_font_size = Some(12.0);
    light_theme.radius = Some(8);
    light_theme.radius_lg = Some(10);
    light_theme.shadow = Some(false);
    light_theme.highlight = Some(editor_highlight_style(
        light_theme.highlight.clone(),
        fallback_highlight,
        ThemeMode::Light,
    ));

    Theme::global_mut(cx).light_theme = Rc::new(light_theme);

    if !Theme::global(cx).mode.is_dark() {
        Theme::change(ThemeMode::Light, None, cx);
    }
}

fn apply_soft_dark_theme(cx: &mut App) {
    let mut dark_theme = (*Theme::global(cx).dark_theme).clone();
    let fallback_highlight = Theme::global(cx).highlight_theme.style.clone();

    // Match a softer charcoal palette so colored diff cues stand out without eye strain.
    dark_theme.colors.accent = Some("#5f81eb".into());
    dark_theme.colors.accent_foreground = Some("#f8fbff".into());
    dark_theme.colors.background = Some("#1f2126".into());
    dark_theme.colors.list = Some("#1f2126".into());
    dark_theme.colors.list_active = Some("#5f81eb33".into());
    dark_theme.colors.list_active_border = Some("#7d9eff".into());
    dark_theme.colors.list_hover = Some("#343e4c".into());
    dark_theme.colors.popover = Some("#242831".into());
    dark_theme.colors.table = Some("#1f2126".into());
    dark_theme.colors.sidebar = Some("#1b1e24".into());
    dark_theme.colors.title_bar = Some("#1a1d22".into());
    dark_theme.colors.list_even = Some("#21242b".into());
    dark_theme.colors.list_head = Some("#292d36".into());
    dark_theme.colors.secondary = Some("#2a2f38".into());
    dark_theme.colors.secondary_hover = Some("#3b4554".into());
    dark_theme.colors.secondary_active = Some("#465163".into());
    dark_theme.colors.muted = Some("#272c35".into());
    dark_theme.colors.muted_foreground = Some("#a3adbb".into());
    dark_theme.colors.border = Some("#3d4554".into());
    dark_theme.font_family = Some(preferred_ui_font_family().into());
    dark_theme.font_size = Some(14.0);
    dark_theme.mono_font_family = Some(preferred_mono_font_family().into());
    dark_theme.mono_font_size = Some(12.0);
    dark_theme.radius = Some(8);
    dark_theme.radius_lg = Some(10);
    dark_theme.shadow = Some(false);
    dark_theme.highlight = Some(editor_highlight_style(
        dark_theme.highlight.clone(),
        fallback_highlight,
        ThemeMode::Dark,
    ));

    Theme::global_mut(cx).dark_theme = Rc::new(dark_theme);

    if Theme::global(cx).mode.is_dark() {
        Theme::change(ThemeMode::Dark, None, cx);
    }
}

fn build_application_menus() -> Vec<Menu> {
    if cfg!(target_os = "macos") {
        vec![
            Menu {
                name: "Hunk".into(),
                items: vec![
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Settings...", OpenSettings),
                    MenuItem::separator(),
                    MenuItem::action("Quit Hunk", QuitApp),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Open Project...", OpenProject),
                    MenuItem::action("Save File", SaveCurrentFile),
                    MenuItem::separator(),
                    MenuItem::action("Settings...", OpenSettings),
                ],
            },
            Menu {
                name: "Edit".into(),
                items: vec![
                    MenuItem::os_action("Copy", CopySelection, OsAction::Copy),
                    MenuItem::os_action("Select All", SelectAllDiffRows, OsAction::SelectAll),
                ],
            },
        ]
    } else {
        vec![
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Open Project...", OpenProject),
                    MenuItem::action("Save File", SaveCurrentFile),
                    MenuItem::action("Settings...", OpenSettings),
                    MenuItem::separator(),
                    MenuItem::action("Quit Hunk", QuitApp),
                ],
            },
            Menu {
                name: "Edit".into(),
                items: vec![
                    MenuItem::action("Copy", CopySelection),
                    MenuItem::action("Select All", SelectAllDiffRows),
                ],
            },
        ]
    }
}

fn install_application_menus(cx: &mut App) {
    cx.set_menus(build_application_menus());
    GlobalState::global_mut(cx).set_app_menus(
        build_application_menus()
            .into_iter()
            .map(|menu| menu.owned())
            .collect(),
    );
}

fn load_keyboard_shortcuts() -> KeyboardShortcuts {
    let store = match ConfigStore::new() {
        Ok(store) => store,
        Err(err) => {
            error!("failed to initialize config path for keyboard shortcuts: {err:#}");
            return KeyboardShortcuts::default();
        }
    };

    match store.load_or_create_default() {
        Ok(config) => config.keyboard_shortcuts,
        Err(err) => {
            error!(
                "failed to load keyboard shortcuts from {}: {err:#}",
                store.path().display()
            );
            KeyboardShortcuts::default()
        }
    }
}

fn bind_keyboard_shortcuts(cx: &mut App, shortcuts: &KeyboardShortcuts) {
    let mut bindings = Vec::new();

    bindings.extend(
        shortcuts
            .select_next_line
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), SelectNextLine, Some("DiffViewer"))),
    );
    bindings.extend(shortcuts.select_previous_line.iter().map(|shortcut| {
        KeyBinding::new(shortcut.as_str(), SelectPreviousLine, Some("DiffViewer"))
    }));
    bindings.extend(shortcuts.extend_selection_next_line.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            ExtendSelectionNextLine,
            Some("DiffViewer"),
        )
    }));
    bindings.extend(
        shortcuts
            .extend_selection_previous_line
            .iter()
            .map(|shortcut| {
                KeyBinding::new(
                    shortcut.as_str(),
                    ExtendSelectionPreviousLine,
                    Some("DiffViewer"),
                )
            }),
    );
    bindings.extend(
        shortcuts
            .copy_selection
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), CopySelection, Some("DiffViewer"))),
    );
    bindings.extend(
        shortcuts.select_all_diff_rows.iter().map(|shortcut| {
            KeyBinding::new(shortcut.as_str(), SelectAllDiffRows, Some("DiffViewer"))
        }),
    );
    bindings.extend(
        shortcuts
            .next_hunk
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), NextHunk, Some("DiffViewer"))),
    );
    bindings.extend(
        shortcuts
            .previous_hunk
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), PreviousHunk, Some("DiffViewer"))),
    );
    bindings.extend(
        shortcuts
            .next_file
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), NextFile, Some("DiffViewer"))),
    );
    bindings.extend(
        shortcuts
            .previous_file
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), PreviousFile, Some("DiffViewer"))),
    );
    bindings.extend(
        shortcuts.toggle_sidebar_tree.iter().map(|shortcut| {
            KeyBinding::new(shortcut.as_str(), ToggleSidebarTree, Some("DiffViewer"))
        }),
    );
    bindings.extend(
        shortcuts
            .switch_to_files_view
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), SwitchToFilesView, None)),
    );
    bindings.extend(
        shortcuts
            .switch_to_review_view
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), SwitchToReviewView, None)),
    );
    bindings.extend(
        shortcuts
            .switch_to_git_view
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), SwitchToGitView, None)),
    );
    bindings.extend(
        shortcuts
            .switch_to_ai_view
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), SwitchToAiView, None)),
    );
    bindings.push(KeyBinding::new("cmd-n", AiNewThread, Some("DiffViewer")));
    bindings.push(KeyBinding::new("ctrl-n", AiNewThread, Some("DiffViewer")));
    bindings.extend(
        shortcuts
            .open_project
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), OpenProject, None)),
    );
    bindings.extend(
        shortcuts
            .save_current_file
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), SaveCurrentFile, None)),
    );
    bindings.extend(
        shortcuts
            .open_settings
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), OpenSettings, None)),
    );
    bindings.extend(
        shortcuts
            .quit_app
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), QuitApp, None)),
    );
    bindings.extend(
        shortcuts
            .repo_tree_new_file
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), RepoTreeNewFile, Some("RepoTree"))),
    );
    bindings.extend(
        shortcuts.repo_tree_new_folder.iter().map(|shortcut| {
            KeyBinding::new(shortcut.as_str(), RepoTreeNewFolder, Some("RepoTree"))
        }),
    );
    bindings.extend(
        shortcuts.repo_tree_rename_file.iter().map(|shortcut| {
            KeyBinding::new(shortcut.as_str(), RepoTreeRenameFile, Some("RepoTree"))
        }),
    );
    bindings.push(KeyBinding::new(
        "escape",
        RepoTreeCancelInlineEdit,
        Some("RepoTree"),
    ));
    bindings.push(KeyBinding::new(
        "escape",
        RepoTreeCancelInlineEdit,
        Some("RepoTreeInlineEdit"),
    ));
    bindings.push(KeyBinding::new(
        "escape",
        AiInterruptSelectedTurn,
        Some("AiWorkspace"),
    ));
    bindings.push(KeyBinding::new(
        "shift-enter",
        InputEnter { secondary: true },
        Some("Input"),
    ));

    cx.bind_keys(bindings);
}

pub fn run() -> Result<()> {
    let app = gpui_platform::application().with_assets(Assets);
    let keyboard_shortcuts = load_keyboard_shortcuts();
    app.on_reopen(|cx: &mut App| {
        if cx.windows().is_empty() {
            open_main_window(cx);
        }
        cx.activate(true);
    });

    app.run(move |cx| {
        gpui_component::init(cx);
        apply_soft_light_theme(cx);
        apply_soft_dark_theme(cx);
        cx.on_action(quit_app);
        bind_keyboard_shortcuts(cx, &keyboard_shortcuts);
        install_application_menus(cx);
        cx.activate(true);
        open_main_window(cx);
    });

    Ok(())
}

fn open_main_window(cx: &mut App) {
    let window_options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Hunk".into()),
            ..Default::default()
        }),
        ..Default::default()
    };

    if let Err(err) = cx.open_window(window_options, |window, cx| {
        let view = cx.new(|cx| DiffViewer::new(window, cx));
        cx.new(|cx| Root::new(view, window, cx))
    }) {
        error!("failed to open window: {err:#}");
    }
}

fn quit_app(_: &QuitApp, cx: &mut App) {
    cx.quit();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsCategory {
    Ui,
    KeyboardShortcuts,
}

impl SettingsCategory {
    const ALL: [Self; 2] = [Self::Ui, Self::KeyboardShortcuts];

    fn title(self) -> &'static str {
        match self {
            Self::Ui => "UI",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
        }
    }
}

#[derive(Clone)]
struct SettingsShortcutRow {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    input_state: Entity<InputState>,
}

#[derive(Clone)]
struct SettingsShortcutInputs {
    select_next_line: Entity<InputState>,
    select_previous_line: Entity<InputState>,
    extend_selection_next_line: Entity<InputState>,
    extend_selection_previous_line: Entity<InputState>,
    copy_selection: Entity<InputState>,
    select_all_diff_rows: Entity<InputState>,
    next_hunk: Entity<InputState>,
    previous_hunk: Entity<InputState>,
    next_file: Entity<InputState>,
    previous_file: Entity<InputState>,
    toggle_sidebar_tree: Entity<InputState>,
    switch_to_files_view: Entity<InputState>,
    switch_to_review_view: Entity<InputState>,
    switch_to_git_view: Entity<InputState>,
    open_project: Entity<InputState>,
    save_current_file: Entity<InputState>,
    open_settings: Entity<InputState>,
    quit_app: Entity<InputState>,
    repo_tree_new_file: Entity<InputState>,
    repo_tree_new_folder: Entity<InputState>,
    repo_tree_rename_file: Entity<InputState>,
}

impl SettingsShortcutInputs {
    fn rows(&self) -> Vec<SettingsShortcutRow> {
        vec![
            SettingsShortcutRow {
                id: "select-next-line",
                label: "Select Next Line",
                hint: "Moves selection down one diff row.",
                input_state: self.select_next_line.clone(),
            },
            SettingsShortcutRow {
                id: "select-previous-line",
                label: "Select Previous Line",
                hint: "Moves selection up one diff row.",
                input_state: self.select_previous_line.clone(),
            },
            SettingsShortcutRow {
                id: "extend-selection-next-line",
                label: "Extend Selection Down",
                hint: "Extends the multi-row selection downward.",
                input_state: self.extend_selection_next_line.clone(),
            },
            SettingsShortcutRow {
                id: "extend-selection-previous-line",
                label: "Extend Selection Up",
                hint: "Extends the multi-row selection upward.",
                input_state: self.extend_selection_previous_line.clone(),
            },
            SettingsShortcutRow {
                id: "copy-selection",
                label: "Copy Selection",
                hint: "Copies the selected diff rows.",
                input_state: self.copy_selection.clone(),
            },
            SettingsShortcutRow {
                id: "select-all-diff-rows",
                label: "Select All Diff Rows",
                hint: "Selects all rows in the current diff.",
                input_state: self.select_all_diff_rows.clone(),
            },
            SettingsShortcutRow {
                id: "next-hunk",
                label: "Next Hunk",
                hint: "Jumps to the next diff hunk.",
                input_state: self.next_hunk.clone(),
            },
            SettingsShortcutRow {
                id: "previous-hunk",
                label: "Previous Hunk",
                hint: "Jumps to the previous diff hunk.",
                input_state: self.previous_hunk.clone(),
            },
            SettingsShortcutRow {
                id: "next-file",
                label: "Next File",
                hint: "Moves to the next changed file.",
                input_state: self.next_file.clone(),
            },
            SettingsShortcutRow {
                id: "previous-file",
                label: "Previous File",
                hint: "Moves to the previous changed file.",
                input_state: self.previous_file.clone(),
            },
            SettingsShortcutRow {
                id: "toggle-sidebar-tree",
                label: "Toggle File Tree",
                hint: "Collapses or expands the left file tree pane.",
                input_state: self.toggle_sidebar_tree.clone(),
            },
            SettingsShortcutRow {
                id: "switch-to-files-view",
                label: "Switch to Files View",
                hint: "Switches the workspace to file editing view.",
                input_state: self.switch_to_files_view.clone(),
            },
            SettingsShortcutRow {
                id: "switch-to-review-view",
                label: "Switch to Review View",
                hint: "Switches the workspace to side-by-side diff review.",
                input_state: self.switch_to_review_view.clone(),
            },
            SettingsShortcutRow {
                id: "switch-to-git-view",
                label: "Switch to Git View",
                hint: "Switches the workspace to the Git workflow view.",
                input_state: self.switch_to_git_view.clone(),
            },
            SettingsShortcutRow {
                id: "open-project",
                label: "Open Project",
                hint: "Opens the system project picker.",
                input_state: self.open_project.clone(),
            },
            SettingsShortcutRow {
                id: "save-current-file",
                label: "Save Current File",
                hint: "Saves the active file editor buffer.",
                input_state: self.save_current_file.clone(),
            },
            SettingsShortcutRow {
                id: "open-settings",
                label: "Open Settings",
                hint: "Opens this settings popup.",
                input_state: self.open_settings.clone(),
            },
            SettingsShortcutRow {
                id: "quit-app",
                label: "Quit App",
                hint: "Quits Hunk.",
                input_state: self.quit_app.clone(),
            },
            SettingsShortcutRow {
                id: "repo-tree-new-file",
                label: "Tree: New File",
                hint: "Creates a file from the focused file tree.",
                input_state: self.repo_tree_new_file.clone(),
            },
            SettingsShortcutRow {
                id: "repo-tree-new-folder",
                label: "Tree: New Folder",
                hint: "Creates a folder from the focused file tree.",
                input_state: self.repo_tree_new_folder.clone(),
            },
            SettingsShortcutRow {
                id: "repo-tree-rename-file",
                label: "Tree: Rename File",
                hint: "Renames the selected file in the focused file tree.",
                input_state: self.repo_tree_rename_file.clone(),
            },
        ]
    }
}

#[derive(Clone)]
struct SettingsDraft {
    category: SettingsCategory,
    theme: ThemePreference,
    show_whitespace: bool,
    show_eol_markers: bool,
    reduce_motion: bool,
    show_fps_counter: bool,
    shortcuts: SettingsShortcutInputs,
    error_message: Option<String>,
}

fn shortcut_lines(values: &[String]) -> String {
    values.join(", ")
}

fn parse_shortcut_lines(value: &str) -> Vec<String> {
    let mut shortcuts = Vec::new();
    let mut token = String::new();
    let mut previous_non_whitespace = None;

    for character in value.chars() {
        let is_separator =
            character == '\n' || (character == ',' && previous_non_whitespace != Some('-'));
        if is_separator {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                shortcuts.push(trimmed.to_owned());
            }
            token.clear();
            previous_non_whitespace = Some(character);
            continue;
        }

        token.push(character);
        if !character.is_whitespace() {
            previous_non_whitespace = Some(character);
        }
    }

    let trimmed = token.trim();
    if !trimmed.is_empty() {
        shortcuts.push(trimmed.to_owned());
    }

    shortcuts
}

struct RepoTreeCacheState {
    nodes: Vec<RepoTreeNode>,
    file_count: usize,
    folder_count: usize,
    expanded_dirs: BTreeSet<String>,
    error: Option<String>,
    scroll_anchor_path: Option<String>,
    fingerprint: Option<RepoSnapshotFingerprint>,
}

struct RepoTreeState {
    list_state: ListState,
    row_count: usize,
    nodes: Vec<RepoTreeNode>,
    rows: Vec<RepoTreeRow>,
    file_count: usize,
    folder_count: usize,
    expanded_dirs: BTreeSet<String>,
    scroll_anchor_path: Option<String>,
    full_cache: Option<RepoTreeCacheState>,
    epoch: usize,
    task: Task<()>,
    loading: bool,
    reload_pending: bool,
    error: Option<String>,
    changed_only: bool,
    last_reload: Instant,
}

impl RepoTreeState {
    fn new() -> Self {
        Self {
            list_state: ListState::new(
                0,
                ListAlignment::Top,
                px(SIDEBAR_REPO_LIST_ESTIMATED_ROW_HEIGHT),
            ),
            row_count: 0,
            nodes: Vec::new(),
            rows: Vec::new(),
            file_count: 0,
            folder_count: 0,
            expanded_dirs: BTreeSet::new(),
            scroll_anchor_path: None,
            full_cache: None,
            epoch: 0,
            task: Task::ready(()),
            loading: false,
            reload_pending: false,
            error: None,
            changed_only: false,
            last_reload: Instant::now(),
        }
    }
}

struct DiffViewer {
    config_store: Option<ConfigStore>,
    config: AppConfig,
    settings_draft: Option<SettingsDraft>,
    state_store: Option<AppStateStore>,
    state: AppState,
    database_store: Option<DatabaseStore>,
    comments_cache: Vec<CommentRecord>,
    comments_preview_open: bool,
    comments_show_non_open: bool,
    comment_miss_streaks: BTreeMap<String, u8>,
    comment_row_matches: BTreeMap<String, usize>,
    comment_open_row_counts: Vec<usize>,
    hovered_comment_row: Option<usize>,
    active_comment_editor_row: Option<usize>,
    comment_input_state: Entity<InputState>,
    comment_status_message: Option<String>,
    project_path: Option<PathBuf>,
    repo_root: Option<PathBuf>,
    branch_name: String,
    branch_has_upstream: bool,
    branch_ahead_count: usize,
    working_copy_commit_id: Option<String>,
    can_undo_operation: bool,
    can_redo_operation: bool,
    branches: Vec<LocalBranch>,
    bookmark_revisions: Vec<BookmarkRevision>,
    jj_workspace_scroll_handle: ScrollHandle,
    pending_bookmark_switch: Option<PendingBookmarkSwitch>,
    show_jj_terms_glossary: bool,
    workspace_view_mode: WorkspaceViewMode,
    ai_connection_state: AiConnectionState,
    ai_bootstrap_loading: bool,
    ai_status_message: Option<String>,
    ai_error_message: Option<String>,
    ai_state_snapshot: hunk_codex::state::AiState,
    ai_selected_thread_id: Option<String>,
    ai_scroll_timeline_to_bottom: bool,
    ai_timeline_follow_output: bool,
    ai_thread_list_scroll_handle: ScrollHandle,
    ai_thread_inline_toast: Option<String>,
    ai_thread_inline_toast_epoch: usize,
    ai_thread_inline_toast_task: Task<()>,
    ai_timeline_list_state: ListState,
    ai_timeline_list_row_count: usize,
    ai_timeline_visible_turn_limit_by_thread: BTreeMap<String, usize>,
    ai_timeline_turn_ids_by_thread: BTreeMap<String, Vec<String>>,
    ai_timeline_row_ids_by_thread: BTreeMap<String, Vec<String>>,
    ai_timeline_rows_by_id: BTreeMap<String, AiTimelineRow>,
    ai_timeline_groups_by_id: BTreeMap<String, AiTimelineGroup>,
    ai_timeline_group_parent_by_child_row_id: BTreeMap<String, String>,
    ai_in_progress_turn_started_at: BTreeMap<String, Instant>,
    ai_composer_activity_elapsed_second: Option<u64>,
    ai_expanded_timeline_row_ids: BTreeSet<String>,
    ai_pending_approvals: Vec<AiPendingApproval>,
    ai_pending_user_inputs: Vec<AiPendingUserInputRequest>,
    ai_pending_user_input_answers: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    ai_account: Option<codex_app_server_protocol::Account>,
    ai_requires_openai_auth: bool,
    ai_pending_chatgpt_login_id: Option<String>,
    ai_pending_chatgpt_auth_url: Option<String>,
    ai_rate_limits: Option<codex_app_server_protocol::RateLimitSnapshot>,
    ai_models: Vec<codex_app_server_protocol::Model>,
    ai_experimental_features: Vec<codex_app_server_protocol::ExperimentalFeature>,
    ai_collaboration_modes: Vec<codex_app_server_protocol::CollaborationModeMask>,
    ai_include_hidden_models: bool,
    ai_selected_model: Option<String>,
    ai_selected_effort: Option<String>,
    ai_selected_collaboration_mode: Option<String>,
    ai_selected_service_tier: AiServiceTierSelection,
    ai_mad_max_mode: bool,
    ai_event_epoch: usize,
    ai_event_task: Task<()>,
    ai_attachment_picker_task: Task<()>,
    ai_worker_thread: Option<JoinHandle<()>>,
    ai_command_tx: Option<mpsc::Sender<AiWorkerCommand>>,
    ai_composer_input_state: Entity<InputState>,
    ai_composer_drafts: BTreeMap<AiComposerDraftKey, AiComposerDraft>,
    files: Vec<ChangedFile>,
    file_status_by_path: BTreeMap<String, FileStatus>,
    revision_stack_collapsed: bool,
    branch_input_state: Entity<InputState>,
    commit_input_state: Entity<InputState>,
    commit_excluded_files: BTreeSet<String>,
    last_commit_subject: Option<String>,
    git_action_epoch: usize,
    git_action_task: Task<()>,
    git_action_loading: bool,
    git_action_label: Option<String>,
    git_status_message: Option<String>,
    working_copy_recovery_candidates: Vec<WorkingCopyRecoveryCandidate>,
    collapsed_files: BTreeSet<String>,
    selected_path: Option<String>,
    selected_status: Option<FileStatus>,
    diff_rows: Vec<SideBySideRow>,
    diff_row_metadata: Vec<DiffStreamRowMeta>,
    diff_row_segment_cache: Vec<Option<DiffRowSegmentCache>>,
    diff_visible_file_header_lookup: Vec<Option<usize>>,
    diff_visible_hunk_header_lookup: Vec<Option<usize>>,
    file_row_ranges: Vec<FileRowRange>,
    file_line_stats: BTreeMap<String, LineStats>,
    diff_list_state: ListState,
    diff_show_whitespace: bool,
    diff_show_eol_markers: bool,
    diff_left_line_number_width: f32,
    diff_right_line_number_width: f32,
    overall_line_stats: LineStats,
    refresh_epoch: usize,
    auto_refresh_unmodified_streak: u32,
    auto_refresh_task: Task<()>,
    repo_watch_task: Task<()>,
    repo_watch_refresh_epoch: usize,
    repo_watch_refresh_force: bool,
    repo_watch_refresh_task: Task<()>,
    snapshot_epoch: usize,
    snapshot_task: Task<()>,
    snapshot_loading: bool,
    snapshot_active_request: Option<SnapshotRefreshRequest>,
    workflow_loading: bool,
    line_stats_epoch: usize,
    line_stats_task: Task<()>,
    line_stats_loading: bool,
    pending_line_stats_refresh: Option<PendingLineStatsRefresh>,
    pending_snapshot_refresh: Option<SnapshotRefreshRequest>,
    pending_dirty_paths: BTreeSet<String>,
    last_snapshot_fingerprint: Option<RepoSnapshotFingerprint>,
    open_project_task: Task<()>,
    patch_epoch: usize,
    patch_task: Task<()>,
    patch_loading: bool,
    in_app_menu_bar: Option<Entity<AppMenuBar>>,
    focus_handle: FocusHandle,
    repo_tree_focus_handle: FocusHandle,
    selection_anchor_row: Option<usize>,
    selection_head_row: Option<usize>,
    drag_selecting_rows: bool,
    scroll_selected_after_reload: bool,
    last_visible_row_start: Option<usize>,
    last_diff_scroll_offset: Option<gpui::Point<gpui::Pixels>>,
    last_scroll_activity_at: Instant,
    segment_prefetch_anchor_row: Option<usize>,
    segment_prefetch_epoch: usize,
    segment_prefetch_task: Task<()>,
    fps: f32,
    frame_sample_count: u32,
    frame_sample_started_at: Instant,
    fps_epoch: usize,
    fps_task: Task<()>,
    repo_discovery_failed: bool,
    error_message: Option<String>,
    sidebar_collapsed: bool,
    repo_tree: RepoTreeState,
    repo_tree_inline_edit: Option<RepoTreeInlineEditState>,
    repo_tree_context_menu: Option<RepoTreeContextMenuState>,
    editor_input_state: Entity<InputState>,
    editor_path: Option<String>,
    editor_loading: bool,
    editor_error: Option<String>,
    editor_dirty: bool,
    editor_last_saved_text: Option<String>,
    editor_epoch: usize,
    editor_task: Task<()>,
    editor_save_loading: bool,
    editor_save_epoch: usize,
    editor_save_task: Task<()>,
    editor_markdown_preview_task: Task<()>,
    editor_markdown_preview_blocks: Vec<MarkdownPreviewBlock>,
    editor_markdown_preview_loading: bool,
    editor_markdown_preview_revision: usize,
    editor_markdown_preview: bool,
}

impl Drop for DiffViewer {
    fn drop(&mut self) {
        self.shutdown_ai_worker_blocking();
    }
}
