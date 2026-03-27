use super::data::{
    DiffStreamRowKind, RepoTreeNodeKind, cached_runtime_fallback_segments, is_markdown_path,
};
use super::theme::*;
use super::*;
use crate::app::markdown_links::{MarkdownLinkRange, markdown_inline_text_and_link_ranges};
use gpui::{Hsla, fill, size};
use gpui_component::Disableable as _;
use gpui_component::Sizable as _;
use gpui_component::animation::cubic_bezier;
use gpui_component::button::{Button, ButtonVariants as _, DropdownButton};
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::scroll::{Scrollbar, ScrollbarShow};
use gpui_component::{Icon, IconName};
use hunk_codex::state::{ItemStatus, ThreadLifecycleStatus};
use hunk_domain::markdown_preview::{
    MarkdownCodeTokenKind, MarkdownInlineSpan, MarkdownPreviewBlock,
};

fn change_status_label_color(
    status: FileStatus,
    cx: &mut Context<DiffViewer>,
) -> (&'static str, Hsla) {
    match status {
        FileStatus::Added => ("ADD", cx.theme().success),
        FileStatus::Modified => ("MOD", cx.theme().warning),
        FileStatus::Deleted => ("DEL", cx.theme().danger),
        FileStatus::Renamed => ("REN", cx.theme().accent),
        FileStatus::Untracked => ("NEW", cx.theme().success),
        FileStatus::TypeChange => ("TYP", cx.theme().warning),
        FileStatus::Conflicted => ("CON", cx.theme().danger),
        FileStatus::Unknown => ("---", cx.theme().muted_foreground),
    }
}

include!("toolbar.rs");
include!("tree.rs");
include!("commit.rs");
include!("workspace_change_row.rs");
include!("git_workspace_loading.rs");
include!("git_recent_commits.rs");
include!("git_workspace_panel.rs");
include!("git_workspace.rs");
include!("file_banner.rs");
include!("file_status.rs");
include!("comments.rs");
include!("syntax_colors.rs");
include!("diff.rs");
include!("diff_rows.rs");
include!("context_menu.rs");
include!("file_editor.rs");
include!("file_editor_surface.rs");
include!("file_quick_open.rs");
include!("ai_loading.rs");
include!("ai.rs");
include!("ai_composer.rs");
include!("ai_timeline_list_view.rs");
include!("ai_workspace_sections.rs");
include!("ai_helpers.rs");
include!("settings.rs");
include!("root.rs");
