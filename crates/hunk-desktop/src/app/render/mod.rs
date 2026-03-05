use super::data::{
    DiffStreamRowKind, RepoTreeNodeKind, cached_runtime_fallback_segments, is_markdown_path,
};
use super::highlight::SyntaxTokenKind;
use super::*;
use gpui_component::Disableable as _;
use gpui_component::Sizable as _;
use gpui_component::animation::cubic_bezier;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::scroll::{Scrollbar, ScrollbarShow};
use gpui_component::{Icon, IconName};
use hunk_codex::state::{ItemStatus, ThreadLifecycleStatus};
use hunk_domain::markdown_preview::{
    MarkdownCodeTokenKind, MarkdownInlineSpan, MarkdownPreviewBlock,
};
use hunk_jj::jj_graph_tree::GraphLaneRow;

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
include!("commit_bookmark_picker.rs");
include!("jj_graph_loading.rs");
include!("jj_graph_right_pane_v2.rs");
include!("jj_graph.rs");
include!("jj_graph_inspector.rs");
include!("jj_graph_focus_strip.rs");
include!("file_banner.rs");
include!("file_status.rs");
include!("comments.rs");
include!("diff.rs");
include!("file_editor.rs");
include!("ai_loading.rs");
include!("ai.rs");
include!("ai_helpers.rs");
include!("settings.rs");
include!("root.rs");
