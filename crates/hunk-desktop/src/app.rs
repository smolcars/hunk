use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::Result;
use codex_app_server_protocol::SkillMetadata;
use gpui::{
    AnchoredPositionMode, Animation, AnimationExt as _, AnyWindowHandle, App, AppContext as _,
    Bounds, ClipboardItem, Context, Corner, Decorations, DragMoveEvent, Empty, Entity, EntityId,
    EntityInputHandler, FocusHandle, InteractiveElement as _, IsZero as _, KeyBinding,
    ListAlignment, ListOffset, ListSizingBehavior, ListState, Menu, MenuItem, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, OsAction, ParentElement as _, PathPromptOptions,
    Pixels, Point, Render, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement as _, Styled as _, SystemMenuType, Task, TitlebarOptions, Window,
    WindowOptions, actions, anchored, canvas, deferred, div, list, point,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, GlobalState, Root, RopeExt, StyledExt as _, Theme, ThemeMode,
    TitleBar, h_flex,
    input::{Enter as InputEnter, InputEvent, InputState},
    menu::AppMenuBar,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    v_flex,
};
use tracing::error;

mod hunk_assets;
mod hunk_picker;

use hunk_assets::HunkAssets;
pub(crate) use hunk_assets::HunkIconName;

use hunk_domain::config::{
    AppConfig, ConfigStore, KeyboardShortcuts, TerminalConfig, TerminalShell, ThemePreference,
};
use hunk_domain::db::{
    CommentLineSide, CommentRecord, CommentStatus, DatabaseStore, NewComment,
    compute_comment_anchor_hash, format_comment_clipboard_blob, next_status_for_unmatched_anchor,
    now_unix_ms,
};
use hunk_domain::diff::{DiffCell, DiffCellKind, DiffRowKind, SideBySideRow};
use hunk_domain::markdown_preview::MarkdownPreviewBlock;
use hunk_domain::state::{
    AiCollaborationModeSelection, AiServiceTierSelection, AppState, AppStateStore,
    CachedChangedFileState, CachedLocalBranchState, CachedRecentCommitState,
    CachedRecentCommitsState, CachedWorkflowState, ReviewCompareSelectionState,
};
use hunk_git::git::{ChangedFile, FileStatus, LineStats, LocalBranch, RepoSnapshotFingerprint};
use hunk_git::history::{
    DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT, RecentCommitSummary, RecentCommitsFingerprint,
};
use hunk_git::worktree::WorkspaceTargetSummary;
use hunk_terminal::{
    TerminalEvent, TerminalScreenSnapshot, TerminalScroll, TerminalSessionHandle,
    TerminalSpawnRequest, spawn_terminal_session,
};

const AI_TERMINAL_TEXT_SELECTION_ROW_ID: &str = "ai-terminal";
const FILES_TERMINAL_TEXT_SELECTION_ROW_ID: &str = "files-terminal";

use ai_composer_commands::AiComposerSlashCommandMenuState;
use ai_composer_completion::{
    ActivePrefixedToken, AiComposerFileCompletionMenuState, AiComposerFileCompletionProvider,
    AiComposerSkillCompletionMenuState, ai_composer_inserted_path_text,
    skill_completion_menu_state,
};
use ai_git_progress::{
    AiGitProgressAction, AiGitProgressState, AiGitProgressStep, ai_commit_and_push_progress_steps,
    ai_open_pr_progress_steps,
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
use ai_runtime::AiWorkerEventPayload;
use ai_runtime::AiWorkerStartConfig;
use ai_runtime::spawn_ai_worker;
use branch_picker::{
    BranchPickerDelegate, branch_picker_selected_index, build_branch_picker_delegate,
};
use data::{
    DiffRowSegmentCache, DiffStreamRowMeta, FileRowRange, RepoTreeNode, RepoTreeNodeKind,
    RepoTreeRow, WorkspaceSwitchAction, WorkspaceViewMode,
};
use hunk_picker::{
    HunkPickerAction, HunkPickerConfig, HunkPickerEvent, HunkPickerState,
    hunk_picker_action_for_keystroke, render_hunk_picker,
};
use project_picker::{
    ProjectPickerDelegate, build_project_picker_delegate, project_picker_selected_index,
};
use refresh_policy::{
    GitWorkspaceRefreshRequest, SnapshotRefreshBehavior, SnapshotRefreshPriority,
    SnapshotRefreshRequest, diff_state_changed, line_stats_paths_from_dirty_paths,
    missing_line_stat_paths, repo_watch_refresh_request, retained_selection_path,
    should_bootstrap_empty_files_workspace_editor, should_refresh_line_stats_after_snapshot,
    should_reload_diff_after_snapshot, should_reload_empty_files_workspace_tree,
    should_reload_repo_tree_after_snapshot, should_request_startup_git_workspace_refresh,
    should_run_cold_start_reconcile, should_scroll_selected_after_reload,
};
use repo_file_search::RepoFileSearchProvider;
use review_compare_picker::{
    ReviewComparePickerDelegate, ReviewCompareSourceOption, build_review_compare_picker_delegate,
};
use workspace_target_picker::{
    WorkspaceTargetPickerDelegate, build_workspace_target_picker_delegate,
    workspace_target_picker_selected_index,
};
use workspace_view::{SHORTCUT_CONTEXT_SELECTABLE_WORKSPACE, SHORTCUT_CONTEXT_TREE_WORKSPACE};

const FPS_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
const AI_PERF_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const AUTO_REFRESH_SCROLL_DEBOUNCE: Duration = Duration::from_millis(500);
const DIFF_MONO_CHAR_WIDTH: f32 = 8.0;
const DIFF_LINE_NUMBER_MIN_DIGITS: u32 = 3;
const DIFF_LINE_NUMBER_EXTRA_PADDING: f32 = 6.0;
const DIFF_MARKER_GUTTER_WIDTH: f32 = 10.0;
const APP_BOTTOM_SAFE_INSET: f32 = 0.0;
const DIFF_BOTTOM_SAFE_INSET: f32 = APP_BOTTOM_SAFE_INSET;
const DIFF_SCROLLBAR_RIGHT_INSET: f32 = 0.0;
const DIFF_SCROLLBAR_SIZE: f32 = 16.0;
const DIFF_SPLIT_MIN_CODE_WIDTH: f32 = 120.0;
const DIFF_SPLIT_HANDLE_WIDTH: f32 = 1.0;
const DIFF_SPLIT_HANDLE_HIT_WIDTH: f32 = 10.0;
const FILE_EDITOR_MAX_BYTES: usize = 2_400_000;
const FILE_EDITOR_TAB_LIMIT: usize = 8;
pub(crate) const FILES_WORKSPACE_RAIL_HEIGHT: f32 = 32.0;
const ABOUT_HUNK_VERSION_LABEL: &str = concat!("Version ", env!("CARGO_PKG_VERSION"));
const ABOUT_HUNK_DESCRIPTION_LINE_ONE: &str = "A fast diff viewer and Codex orchestrator.";
const ABOUT_HUNK_DESCRIPTION_LINE_TWO: &str = "Hunk is built in GPUI and aims to be very fast.";
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
const AI_THREAD_TITLE_REFRESH_MAX_ATTEMPTS: u8 = 20;
const AI_THREAD_TITLE_REFRESH_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const AI_COMPOSER_STATUS_AUTO_DISMISS_DELAY: Duration = Duration::from_secs(5);

mod ai_bookmarks;
mod ai_composer_commands;
mod ai_composer_completion;
mod ai_paths;
mod ai_thread_catalog_scheduler;
mod ai_thread_flow;
mod branch_activation;
mod branch_picker;
mod comment_overlay;
mod fuzzy_match;
mod project_open;
mod project_picker;
mod refresh_policy;
mod review_compare_picker;
mod workspace_target_picker;

include!("app/types.rs");

mod ai_git_progress;
mod ai_rollout_fallback;
mod ai_runtime;
mod controller;
mod data;
mod data_segments;
mod highlight;
mod markdown_links;
mod native_files_editor;
mod notifications;
mod render;
mod repo_file_search;
mod review_workspace_session;
mod terminal_cursor;
mod theme;
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
        ViewCurrentReviewFile,
        ToggleSidebarTree,
        SwitchToFilesView,
        SwitchToReviewView,
        SwitchToGitView,
        SwitchToAiView,
        AiToggleTerminalDrawer,
        AiTerminalSendCtrlC,
        AiTerminalSendCtrlA,
        AiTerminalSendTab,
        AiTerminalSendBackTab,
        AiTerminalSendUp,
        AiTerminalSendDown,
        AiTerminalSendLeft,
        AiTerminalSendRight,
        AiTerminalSendHome,
        AiTerminalSendEnd,
        AiNewThread,
        AiNewWorktreeThread,
        AiQueuePrompt,
        AiEditLastQueuedPrompt,
        AiInterruptSelectedTurn,
        OpenProject,
        QuickOpenFile,
        FilesEditorCopy,
        FilesEditorCut,
        FilesEditorPaste,
        FilesEditorMoveUp,
        FilesEditorMoveDown,
        FilesEditorMoveLeft,
        FilesEditorMoveRight,
        FilesEditorMoveToBeginningOfLine,
        FilesEditorMoveToEndOfLine,
        FilesEditorMoveToBeginningOfDocument,
        FilesEditorMoveToEndOfDocument,
        FilesEditorMoveToPreviousWordStart,
        FilesEditorMoveToNextWordEnd,
        FilesEditorSelectUp,
        FilesEditorSelectDown,
        FilesEditorSelectLeft,
        FilesEditorSelectRight,
        FilesEditorSelectToBeginningOfLine,
        FilesEditorSelectToEndOfLine,
        FilesEditorSelectToBeginningOfDocument,
        FilesEditorSelectToEndOfDocument,
        FilesEditorSelectToPreviousWordStart,
        FilesEditorSelectToNextWordEnd,
        FilesEditorPageUp,
        FilesEditorPageDown,
        NextEditorTab,
        PreviousEditorTab,
        CloseEditorTab,
        SaveCurrentFile,
        AboutHunk,
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

fn build_application_menus() -> Vec<Menu> {
    if cfg!(target_os = "macos") {
        vec![
            Menu {
                name: "Hunk".into(),
                items: vec![
                    MenuItem::action("About Hunk", AboutHunk),
                    MenuItem::separator(),
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
                    MenuItem::action("Quick Open...", QuickOpenFile),
                    MenuItem::action("Save File", SaveCurrentFile),
                    MenuItem::separator(),
                    MenuItem::action("About Hunk", AboutHunk),
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
                    MenuItem::action("Quick Open...", QuickOpenFile),
                    MenuItem::action("Save File", SaveCurrentFile),
                    MenuItem::separator(),
                    MenuItem::action("About Hunk", AboutHunk),
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

    bindings.extend(shortcuts.select_next_line.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            SelectNextLine,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.select_previous_line.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            SelectPreviousLine,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.extend_selection_next_line.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            ExtendSelectionNextLine,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
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
                    Some(WorkspaceViewMode::Diff.shortcut_context()),
                )
            }),
    );
    bindings.extend(shortcuts.copy_selection.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            CopySelection,
            Some(SHORTCUT_CONTEXT_SELECTABLE_WORKSPACE),
        )
    }));
    bindings.extend(shortcuts.select_all_diff_rows.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            SelectAllDiffRows,
            Some(SHORTCUT_CONTEXT_SELECTABLE_WORKSPACE),
        )
    }));
    bindings.extend(shortcuts.next_hunk.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            NextHunk,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.previous_hunk.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            PreviousHunk,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.next_file.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            NextFile,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.previous_file.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            PreviousFile,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.view_current_review_file.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            ViewCurrentReviewFile,
            Some(WorkspaceViewMode::Diff.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.toggle_sidebar_tree.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            ToggleSidebarTree,
            Some(SHORTCUT_CONTEXT_TREE_WORKSPACE),
        )
    }));
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
    bindings.extend(shortcuts.toggle_ai_terminal_drawer.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            AiToggleTerminalDrawer,
            Some(WorkspaceViewMode::Ai.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.toggle_ai_terminal_drawer.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            AiToggleTerminalDrawer,
            Some(WorkspaceViewMode::Files.shortcut_context()),
        )
    }));
    bindings.push(KeyBinding::new(
        "ctrl-c",
        AiTerminalSendCtrlC,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-a",
        AiTerminalSendCtrlA,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "tab",
        AiTerminalSendTab,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "shift-tab",
        AiTerminalSendBackTab,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new("up", AiTerminalSendUp, Some("AiTerminal")));
    bindings.push(KeyBinding::new(
        "down",
        AiTerminalSendDown,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "left",
        AiTerminalSendLeft,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "right",
        AiTerminalSendRight,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "home",
        AiTerminalSendHome,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "end",
        AiTerminalSendEnd,
        Some("AiTerminal"),
    ));
    bindings.push(KeyBinding::new(
        "cmd-n",
        AiNewThread,
        Some(WorkspaceViewMode::Ai.shortcut_context()),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-n",
        AiNewThread,
        Some(WorkspaceViewMode::Ai.shortcut_context()),
    ));
    bindings.push(KeyBinding::new(
        "cmd-shift-n",
        AiNewWorktreeThread,
        Some(WorkspaceViewMode::Ai.shortcut_context()),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-shift-n",
        AiNewWorktreeThread,
        Some(WorkspaceViewMode::Ai.shortcut_context()),
    ));
    bindings.extend(
        shortcuts
            .open_project
            .iter()
            .map(|shortcut| KeyBinding::new(shortcut.as_str(), OpenProject, None)),
    );
    bindings.push(KeyBinding::new(
        "cmd-p",
        QuickOpenFile,
        Some(WorkspaceViewMode::Files.shortcut_context()),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-p",
        QuickOpenFile,
        Some(WorkspaceViewMode::Files.shortcut_context()),
    ));
    bindings.push(KeyBinding::new(
        "cmd-c",
        FilesEditorCopy,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-c",
        FilesEditorCopy,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "cmd-x",
        FilesEditorCut,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-x",
        FilesEditorCut,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "cmd-v",
        FilesEditorPaste,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-v",
        FilesEditorPaste,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "up",
        FilesEditorMoveUp,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "down",
        FilesEditorMoveDown,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "left",
        FilesEditorMoveLeft,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "right",
        FilesEditorMoveRight,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "shift-up",
        FilesEditorSelectUp,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "shift-down",
        FilesEditorSelectDown,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "shift-left",
        FilesEditorSelectLeft,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "shift-right",
        FilesEditorSelectRight,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "home",
        FilesEditorMoveToBeginningOfLine,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "end",
        FilesEditorMoveToEndOfLine,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "shift-home",
        FilesEditorSelectToBeginningOfLine,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "shift-end",
        FilesEditorSelectToEndOfLine,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "pageup",
        FilesEditorPageUp,
        Some("FilesEditor"),
    ));
    bindings.push(KeyBinding::new(
        "pagedown",
        FilesEditorPageDown,
        Some("FilesEditor"),
    ));
    if cfg!(target_os = "macos") {
        bindings.push(KeyBinding::new(
            "cmd-left",
            FilesEditorMoveToBeginningOfLine,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-right",
            FilesEditorMoveToEndOfLine,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-up",
            FilesEditorMoveToBeginningOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-down",
            FilesEditorMoveToEndOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-shift-left",
            FilesEditorSelectToBeginningOfLine,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-shift-right",
            FilesEditorSelectToEndOfLine,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-shift-up",
            FilesEditorSelectToBeginningOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-shift-down",
            FilesEditorSelectToEndOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-home",
            FilesEditorMoveToBeginningOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "cmd-end",
            FilesEditorMoveToEndOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "alt-left",
            FilesEditorMoveToPreviousWordStart,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "alt-right",
            FilesEditorMoveToNextWordEnd,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "alt-shift-left",
            FilesEditorSelectToPreviousWordStart,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "alt-shift-right",
            FilesEditorSelectToNextWordEnd,
            Some("FilesEditor"),
        ));
    } else {
        bindings.push(KeyBinding::new(
            "ctrl-left",
            FilesEditorMoveToPreviousWordStart,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "ctrl-right",
            FilesEditorMoveToNextWordEnd,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "ctrl-shift-left",
            FilesEditorSelectToPreviousWordStart,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "ctrl-shift-right",
            FilesEditorSelectToNextWordEnd,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "ctrl-home",
            FilesEditorMoveToBeginningOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "ctrl-end",
            FilesEditorMoveToEndOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "ctrl-shift-home",
            FilesEditorSelectToBeginningOfDocument,
            Some("FilesEditor"),
        ));
        bindings.push(KeyBinding::new(
            "ctrl-shift-end",
            FilesEditorSelectToEndOfDocument,
            Some("FilesEditor"),
        ));
    }
    bindings.extend(shortcuts.save_current_file.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            SaveCurrentFile,
            Some(WorkspaceViewMode::Files.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.next_editor_tab.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            NextEditorTab,
            Some(WorkspaceViewMode::Files.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.previous_editor_tab.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            PreviousEditorTab,
            Some(WorkspaceViewMode::Files.shortcut_context()),
        )
    }));
    bindings.extend(shortcuts.close_editor_tab.iter().map(|shortcut| {
        KeyBinding::new(
            shortcut.as_str(),
            CloseEditorTab,
            Some(WorkspaceViewMode::Files.shortcut_context()),
        )
    }));
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
        Some(WorkspaceViewMode::Ai.shortcut_context()),
    ));
    bindings.push(KeyBinding::new("tab", AiQueuePrompt, Some("AiComposer")));
    bindings.push(KeyBinding::new(
        "ctrl-shift-up",
        AiEditLastQueuedPrompt,
        Some("AiComposer"),
    ));
    bindings.push(KeyBinding::new(
        "shift-enter",
        InputEnter { secondary: true },
        Some("Input"),
    ));

    cx.bind_keys(bindings);
}

pub fn run() -> Result<()> {
    let app = gpui_platform::application().with_assets(HunkAssets);
    let keyboard_shortcuts = load_keyboard_shortcuts();
    app.on_reopen(|cx: &mut App| {
        if cx.windows().is_empty() {
            open_main_window(cx);
        }
        cx.activate(true);
    });

    app.run(move |cx| {
        gpui_component::init(cx);
        theme::install_hunk_themes(cx);
        // Keep a global quit hook alive so tracked Codex hosts are cleaned up even if a
        // particular view/runtime teardown path is bypassed during shutdown.
        std::mem::forget(cx.on_app_quit(|_| async move {
            hunk_codex::host::begin_host_shutdown();
            hunk_codex::host::cleanup_tracked_hosts_for_shutdown();
        }));
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
        app_id: Some("hunk_desktop".into()),
        titlebar: Some(TitlebarOptions {
            title: Some("Hunk".into()),
            ..Default::default()
        }),
        ..Default::default()
    };

    if let Err(err) = cx.open_window(window_options, |window, cx| {
        let view = cx.new(|cx| DiffViewer::new(window, cx));
        view.update(cx, |this, cx| this.defer_root_focus(cx));
        cx.new(|cx| Root::new(view, window, cx))
    }) {
        error!("failed to open window: {err:#}");
    }
}

fn quit_app(_: &QuitApp, cx: &mut App) {
    cx.quit();
}

include!("app/settings.rs");

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

struct WorkspaceProjectState {
    repo_root: Option<PathBuf>,
    workspace_targets: Vec<WorkspaceTargetSummary>,
    active_workspace_target_id: Option<String>,
    git_workspace: GitWorkspaceState,
    review_compare_sources: Vec<ReviewCompareSourceOption>,
    review_default_left_source_id: Option<String>,
    review_default_right_source_id: Option<String>,
    review_left_source_id: Option<String>,
    review_right_source_id: Option<String>,
    review_loaded_left_source_id: Option<String>,
    review_loaded_right_source_id: Option<String>,
    review_loaded_collapsed_files: BTreeSet<String>,
    branch_name: String,
    branch_has_upstream: bool,
    branch_ahead_count: usize,
    branch_behind_count: usize,
    working_copy_commit_id: Option<String>,
    branches: Vec<LocalBranch>,
    git_working_tree_scroll_handle: ScrollHandle,
    recent_commits_scroll_handle: ScrollHandle,
    files: Vec<ChangedFile>,
    file_status_by_path: BTreeMap<String, FileStatus>,
    last_commit_subject: Option<String>,
    recent_commits: Vec<RecentCommitSummary>,
    recent_commits_error: Option<String>,
    collapsed_files: BTreeSet<String>,
    selected_path: Option<String>,
    selected_status: Option<FileStatus>,
    diff_rows: Vec<SideBySideRow>,
    diff_row_metadata: Vec<DiffStreamRowMeta>,
    diff_row_segment_cache: Vec<Option<DiffRowSegmentCache>>,
    file_row_ranges: Vec<FileRowRange>,
    file_line_stats: BTreeMap<String, LineStats>,
    review_surface: ReviewWorkspaceSurfaceState,
    review_files: Vec<ChangedFile>,
    review_file_status_by_path: BTreeMap<String, FileStatus>,
    review_file_line_stats: BTreeMap<String, LineStats>,
    review_overall_line_stats: LineStats,
    review_compare_loading: bool,
    review_compare_error: Option<String>,
    review_workspace_session: Option<review_workspace_session::ReviewWorkspaceSession>,
    review_loaded_snapshot_fingerprint: Option<RepoSnapshotFingerprint>,
    overall_line_stats: LineStats,
    last_git_workspace_fingerprint: Option<RepoSnapshotFingerprint>,
    recent_commits_loading: bool,
    last_recent_commits_fingerprint: Option<RecentCommitsFingerprint>,
    last_snapshot_fingerprint: Option<RepoSnapshotFingerprint>,
    repo_tree: RepoTreeState,
    file_editor_tabs: Vec<FileEditorTab>,
    active_file_editor_tab_id: Option<usize>,
    next_file_editor_tab_id: usize,
    file_editor_tab_scroll_handle: ScrollHandle,
    files_editor: native_files_editor::SharedFilesEditor,
    file_quick_open_visible: bool,
    file_quick_open_matches: Vec<String>,
    file_quick_open_selected_ix: usize,
    editor_path: Option<String>,
    editor_error: Option<String>,
    editor_dirty: bool,
    editor_last_saved_text: Option<String>,
    editor_markdown_preview_blocks: Vec<MarkdownPreviewBlock>,
    editor_markdown_preview_revision: usize,
    editor_markdown_preview: bool,
    editor_search_visible: bool,
}

struct ReviewWorkspaceSurfaceState {
    status_message: Option<String>,
    selected_path: Option<String>,
    workspace_editor_session: Option<native_files_editor::WorkspaceEditorSession>,
    workspace_search_matches: Vec<review_workspace_session::ReviewWorkspaceSearchTarget>,
    selection_anchor_row: Option<usize>,
    selection_head_row: Option<usize>,
    diff_visible_file_header_lookup: Vec<Option<usize>>,
    diff_visible_hunk_header_lookup: Vec<Option<usize>>,
    diff_scroll_handle: ScrollHandle,
    diff_split_ratio: f32,
    diff_split_bounds: Option<Bounds<Pixels>>,
    diff_left_line_number_width: f32,
    diff_right_line_number_width: f32,
    last_surface_snapshot: Option<review_workspace_session::ReviewWorkspaceSurfaceSnapshot>,
    last_prefetched_visible_row_range: Option<std::ops::Range<usize>>,
    last_diff_scroll_offset: Option<gpui::Point<gpui::Pixels>>,
}

impl ReviewWorkspaceSurfaceState {
    fn new() -> Self {
        Self {
            status_message: None,
            selected_path: None,
            workspace_editor_session: None,
            workspace_search_matches: Vec::new(),
            selection_anchor_row: None,
            selection_head_row: None,
            diff_visible_file_header_lookup: Vec::new(),
            diff_visible_hunk_header_lookup: Vec::new(),
            diff_scroll_handle: ScrollHandle::default(),
            diff_split_ratio: 0.5,
            diff_split_bounds: None,
            diff_left_line_number_width: crate::app::data::line_number_column_width(
                DIFF_LINE_NUMBER_MIN_DIGITS,
            ),
            diff_right_line_number_width: crate::app::data::line_number_column_width(
                DIFF_LINE_NUMBER_MIN_DIGITS,
            ),
            last_surface_snapshot: None,
            last_prefetched_visible_row_range: None,
            last_diff_scroll_offset: None,
        }
    }

    fn clear_legacy_diff_row_lookups(&mut self) {
        self.diff_visible_file_header_lookup.clear();
        self.diff_visible_hunk_header_lookup.clear();
    }

    fn clear_workspace_surface_snapshot(&mut self) {
        self.last_surface_snapshot = None;
    }

    fn clear_workspace_editor_session(&mut self) {
        self.workspace_editor_session = None;
    }

    fn clear_workspace_search_matches(&mut self) {
        self.workspace_search_matches.clear();
    }

    fn clear_row_selection(&mut self) {
        self.selection_anchor_row = None;
        self.selection_head_row = None;
    }
}

struct DiffViewer {
    config_store: Option<ConfigStore>,
    config: AppConfig,
    settings_draft: Option<SettingsDraft>,
    state_store: Option<AppStateStore>,
    state: AppState,
    database_store: Option<DatabaseStore>,
    window_handle: AnyWindowHandle,
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
    workspace_targets: Vec<WorkspaceTargetSummary>,
    active_workspace_target_id: Option<String>,
    git_workspace: GitWorkspaceState,
    review_compare_sources: Vec<ReviewCompareSourceOption>,
    review_default_left_source_id: Option<String>,
    review_default_right_source_id: Option<String>,
    review_left_source_id: Option<String>,
    review_right_source_id: Option<String>,
    review_loaded_left_source_id: Option<String>,
    review_loaded_right_source_id: Option<String>,
    review_loaded_collapsed_files: BTreeSet<String>,
    branch_name: String,
    branch_has_upstream: bool,
    branch_ahead_count: usize,
    branch_behind_count: usize,
    working_copy_commit_id: Option<String>,
    branches: Vec<LocalBranch>,
    git_working_tree_scroll_handle: ScrollHandle,
    recent_commits_scroll_handle: ScrollHandle,
    workspace_view_mode: WorkspaceViewMode,
    ai_connection_state: AiConnectionState,
    ai_bootstrap_loading: bool,
    ai_status_message: Option<String>,
    ai_error_message: Option<String>,
    ai_state_snapshot: hunk_codex::state::AiState,
    ai_selected_thread_id: Option<String>,
    ai_new_thread_draft_active: bool,
    ai_new_thread_start_mode: AiNewThreadStartMode,
    ai_worktree_base_branch_name: Option<String>,
    ai_pending_new_thread_selection: bool,
    ai_pending_thread_start: Option<AiPendingThreadStart>,
    ai_pending_steers: Vec<AiPendingSteer>,
    ai_queued_messages: Vec<AiQueuedUserMessage>,
    ai_interrupt_restore_queued_thread_ids: BTreeSet<String>,
    ai_scroll_timeline_to_bottom: bool,
    ai_timeline_follow_output: bool,
    ai_git_progress: Option<AiGitProgressState>,
    ai_thread_title_refresh_state_by_thread: BTreeMap<String, AiThreadTitleRefreshState>,
    ai_expanded_thread_sidebar_project_roots: BTreeSet<String>,
    ai_visible_frame_state: Option<AiVisibleFrameState>,
    ai_thread_sidebar_sections: Vec<AiVisibleThreadProjectSection>,
    ai_thread_sidebar_rows: Vec<AiThreadSidebarRow>,
    ai_thread_sidebar_list_state: ListState,
    ai_thread_sidebar_row_count: usize,
    ai_timeline_list_view: Option<Entity<render::AiTimelineListView>>,
    ai_timeline_list_state: ListState,
    ai_timeline_list_row_count: usize,
    ai_timeline_visible_turn_limit_by_thread: BTreeMap<String, usize>,
    ai_timeline_turn_ids_by_thread: BTreeMap<String, Vec<String>>,
    ai_timeline_row_ids_by_thread: BTreeMap<String, Vec<String>>,
    ai_timeline_rows_by_id: BTreeMap<String, AiTimelineRow>,
    ai_timeline_groups_by_id: BTreeMap<String, AiTimelineGroup>,
    ai_timeline_group_parent_by_child_row_id: BTreeMap<String, String>,
    ai_markdown_row_cache: Mutex<BTreeMap<String, AiMarkdownRowCacheEntry>>,
    ai_in_progress_turn_started_at: BTreeMap<String, Instant>,
    ai_composer_activity_elapsed_second: Option<u64>,
    ai_expanded_timeline_row_ids: BTreeSet<String>,
    ai_pressed_markdown_link: Option<AiPressedMarkdownLink>,
    ai_text_selection: Option<AiTextSelection>,
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
    ai_skills: Vec<SkillMetadata>,
    ai_skills_generation: usize,
    ai_include_hidden_models: bool,
    ai_selected_model: Option<String>,
    ai_selected_effort: Option<String>,
    ai_selected_collaboration_mode: AiCollaborationModeSelection,
    ai_selected_service_tier: AiServiceTierSelection,
    ai_mad_max_mode: bool,
    ai_event_epoch: usize,
    ai_event_task: Task<()>,
    ai_thread_catalog_refresh_epoch: usize,
    ai_thread_catalog_task: Task<()>,
    ai_attachment_picker_task: Task<()>,
    ai_workspace_states: BTreeMap<String, AiWorkspaceState>,
    ai_hidden_runtimes: BTreeMap<String, AiHiddenRuntimeHandle>,
    ai_runtime_starting_workspace_key: Option<String>,
    ai_worker_thread: Option<JoinHandle<()>>,
    ai_command_tx: Option<mpsc::Sender<AiWorkerCommand>>,
    ai_worker_workspace_key: Option<String>,
    ai_draft_workspace_root_override: Option<PathBuf>,
    ai_draft_workspace_target_id: Option<String>,
    ai_terminal_states_by_thread: BTreeMap<String, AiThreadTerminalState>,
    ai_hidden_terminal_runtimes: BTreeMap<String, AiHiddenTerminalRuntimeHandle>,
    ai_terminal_open: bool,
    ai_terminal_follow_output: bool,
    ai_terminal_height_px: f32,
    ai_terminal_input_draft: String,
    ai_terminal_session: AiTerminalSessionState,
    ai_terminal_input_state: Entity<InputState>,
    ai_terminal_focus_handle: FocusHandle,
    ai_terminal_surface_focused: bool,
    ai_terminal_cursor_blink_visible: bool,
    ai_terminal_cursor_blink_active: bool,
    ai_terminal_cursor_output_suppressed: bool,
    ai_terminal_panel_bounds: Option<Bounds<Pixels>>,
    ai_terminal_grid_size: Option<(u16, u16)>,
    ai_terminal_pending_input: Option<String>,
    ai_terminal_event_task: Task<()>,
    ai_terminal_cursor_blink_task: Task<()>,
    ai_terminal_cursor_output_task: Task<()>,
    ai_terminal_runtime: Option<AiTerminalRuntimeHandle>,
    ai_terminal_cursor_blink_generation: usize,
    ai_terminal_cursor_output_generation: usize,
    ai_terminal_runtime_generation: usize,
    ai_terminal_stop_requested: bool,
    workspace_project_states: BTreeMap<String, WorkspaceProjectState>,
    files_terminal_states_by_project: BTreeMap<String, FilesProjectTerminalState>,
    files_hidden_terminal_runtimes: BTreeMap<String, FilesHiddenTerminalRuntimeHandle>,
    files_terminal_open: bool,
    files_terminal_follow_output: bool,
    files_terminal_height_px: f32,
    files_terminal_session: AiTerminalSessionState,
    files_terminal_focus_handle: FocusHandle,
    files_terminal_restore_target: FilesTerminalRestoreTarget,
    files_terminal_surface_focused: bool,
    files_terminal_cursor_blink_visible: bool,
    files_terminal_cursor_blink_active: bool,
    files_terminal_cursor_output_suppressed: bool,
    files_terminal_panel_bounds: Option<Bounds<Pixels>>,
    files_terminal_grid_size: Option<(u16, u16)>,
    files_terminal_pending_input: Option<String>,
    files_terminal_event_task: Task<()>,
    files_terminal_cursor_blink_task: Task<()>,
    files_terminal_cursor_output_task: Task<()>,
    files_terminal_runtime: Option<FilesTerminalRuntimeHandle>,
    files_terminal_cursor_blink_generation: usize,
    files_terminal_cursor_output_generation: usize,
    files_terminal_runtime_generation: usize,
    files_terminal_stop_requested: bool,
    repo_file_search_provider: Rc<RepoFileSearchProvider>,
    repo_file_search_reload_task: Task<()>,
    repo_file_search_loading: bool,
    ai_composer_file_completion_provider: Rc<AiComposerFileCompletionProvider>,
    ai_composer_file_completion_reload_task: Task<()>,
    ai_composer_file_completion_menu: Option<AiComposerFileCompletionMenuState>,
    ai_composer_file_completion_selected_ix: usize,
    ai_composer_file_completion_dismissed_token: Option<ActivePrefixedToken>,
    ai_composer_file_completion_scroll_handle: ScrollHandle,
    ai_composer_slash_command_menu: Option<AiComposerSlashCommandMenuState>,
    ai_composer_slash_command_selected_ix: usize,
    ai_composer_slash_command_dismissed_token: Option<ActivePrefixedToken>,
    ai_composer_slash_command_scroll_handle: ScrollHandle,
    ai_composer_skill_completion_menu: Option<AiComposerSkillCompletionMenuState>,
    ai_composer_skill_completion_selected_ix: usize,
    ai_composer_skill_completion_dismissed_token: Option<ActivePrefixedToken>,
    ai_composer_skill_completion_scroll_handle: ScrollHandle,
    ai_composer_completion_sync_key: Option<AiComposerCompletionSyncKey>,
    ai_worktree_base_branch_picker_state: Entity<HunkPickerState<BranchPickerDelegate>>,
    ai_composer_input_state: Entity<InputState>,
    ai_review_mode_active: bool,
    ai_review_mode_thread_ids: BTreeSet<String>,
    ai_usage_popover_open: bool,
    ai_composer_drafts: BTreeMap<AiComposerDraftKey, AiComposerDraft>,
    ai_composer_status_by_draft: BTreeMap<AiComposerDraftKey, String>,
    ai_composer_status_generation: usize,
    ai_composer_status_generation_by_key: BTreeMap<AiComposerStatusKey, usize>,
    available_project_open_targets: Vec<project_open::ProjectOpenTargetId>,
    files: Vec<ChangedFile>,
    file_status_by_path: BTreeMap<String, FileStatus>,
    project_picker_state: Entity<HunkPickerState<ProjectPickerDelegate>>,
    workspace_target_picker_state: Entity<HunkPickerState<WorkspaceTargetPickerDelegate>>,
    review_left_picker_state: Entity<HunkPickerState<ReviewComparePickerDelegate>>,
    review_right_picker_state: Entity<HunkPickerState<ReviewComparePickerDelegate>>,
    branch_picker_state: Entity<HunkPickerState<BranchPickerDelegate>>,
    branch_input_state: Entity<InputState>,
    branch_input_has_text: bool,
    commit_input_state: Entity<InputState>,
    git_action_epoch: usize,
    git_action_task: Task<()>,
    git_action_loading: bool,
    git_action_label: Option<String>,
    workspace_target_switch_loading: bool,
    git_status_message: Option<String>,
    git_workspace_refresh_epoch: usize,
    git_workspace_refresh_task: Task<()>,
    git_workspace_active_root: Option<PathBuf>,
    git_workspace_loading: bool,
    pending_git_workspace_refresh: Option<GitWorkspaceRefreshRequest>,
    last_git_workspace_fingerprint: Option<RepoSnapshotFingerprint>,
    last_commit_subject: Option<String>,
    recent_commits: Vec<RecentCommitSummary>,
    recent_commits_error: Option<String>,
    collapsed_files: BTreeSet<String>,
    selected_path: Option<String>,
    selected_status: Option<FileStatus>,
    diff_rows: Vec<SideBySideRow>,
    diff_row_metadata: Vec<DiffStreamRowMeta>,
    diff_row_segment_cache: Vec<Option<DiffRowSegmentCache>>,
    file_row_ranges: Vec<FileRowRange>,
    file_line_stats: BTreeMap<String, LineStats>,
    review_surface: ReviewWorkspaceSurfaceState,
    review_files: Vec<ChangedFile>,
    review_file_status_by_path: BTreeMap<String, FileStatus>,
    review_file_line_stats: BTreeMap<String, LineStats>,
    review_overall_line_stats: LineStats,
    review_compare_loading: bool,
    review_compare_error: Option<String>,
    review_workspace_session: Option<review_workspace_session::ReviewWorkspaceSession>,
    review_loaded_snapshot_fingerprint: Option<RepoSnapshotFingerprint>,
    overall_line_stats: LineStats,
    refresh_epoch: usize,
    auto_refresh_unmodified_streak: u32,
    auto_refresh_task: Task<()>,
    repo_watch_task: Task<()>,
    repo_watch_refresh_epoch: usize,
    repo_watch_pending_refresh: Option<SnapshotRefreshRequest>,
    repo_watch_pending_git_workspace_refresh: bool,
    repo_watch_pending_recent_commits_refresh: bool,
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
    recent_commits_epoch: usize,
    recent_commits_task: Task<()>,
    recent_commits_loading: bool,
    recent_commits_active_request: Option<RecentCommitsRefreshRequest>,
    pending_recent_commits_refresh: Option<RecentCommitsRefreshRequest>,
    last_recent_commits_fingerprint: Option<RecentCommitsFingerprint>,
    pending_dirty_paths: BTreeSet<String>,
    last_snapshot_fingerprint: Option<RepoSnapshotFingerprint>,
    open_project_task: Task<()>,
    patch_epoch: usize,
    patch_task: Task<()>,
    patch_loading: bool,
    in_app_menu_bar: Option<Entity<AppMenuBar>>,
    focus_handle: FocusHandle,
    repo_tree_focus_handle: FocusHandle,
    files_editor_focus_handle: FocusHandle,
    drag_selecting_rows: bool,
    scroll_selected_after_reload: bool,
    last_scroll_activity_at: Instant,
    segment_prefetch_anchor_row: Option<usize>,
    segment_prefetch_epoch: usize,
    segment_prefetch_task: Task<()>,
    fps: f32,
    frame_sample_count: u32,
    frame_sample_started_at: Instant,
    ignore_next_frame_sample: bool,
    fps_epoch: usize,
    fps_task: Task<()>,
    ai_perf_metrics: RefCell<AiPerfMetrics>,
    repo_discovery_failed: bool,
    error_message: Option<String>,
    sidebar_collapsed: bool,
    repo_tree: RepoTreeState,
    repo_tree_inline_edit: Option<RepoTreeInlineEditState>,
    repo_tree_context_menu: Option<RepoTreeContextMenuState>,
    workspace_text_context_menu: Option<WorkspaceTextContextMenuState>,
    file_editor_tabs: Vec<FileEditorTab>,
    active_file_editor_tab_id: Option<usize>,
    next_file_editor_tab_id: usize,
    file_editor_tab_scroll_handle: ScrollHandle,
    files_editor: native_files_editor::SharedFilesEditor,
    editor_search_input_state: Entity<InputState>,
    editor_replace_input_state: Entity<InputState>,
    file_quick_open_input_state: Entity<InputState>,
    file_quick_open_visible: bool,
    file_quick_open_matches: Vec<String>,
    file_quick_open_selected_ix: usize,
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
    editor_search_visible: bool,
}

impl Drop for DiffViewer {
    fn drop(&mut self) {
        self.sync_active_file_editor_tab_state();
        for tab in &self.file_editor_tabs {
            tab.files_editor.borrow_mut().shutdown();
        }
        self.files_editor.borrow_mut().shutdown();
        self.stop_all_ai_terminal_runtimes("dropping app");
        self.stop_all_files_terminal_runtimes("dropping app");
        self.shutdown_ai_worker_blocking();
    }
}
