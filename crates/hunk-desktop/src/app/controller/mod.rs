use anyhow::Context as _;
use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use gpui_component::Sizable as _;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::DialogFooter;
use notify::Watcher;
use std::cell::RefCell;
use std::rc::Rc;
use tracing::{debug, error, warn};

use crate::app::ai_git_progress::{
    ai_create_branch_and_push_progress_steps, ai_delete_worktree_progress_steps,
};
use crate::app::ai_thread_flow::{
    AiCodexGenerationConfig, AiCommitGenerationContext, AiCommitMessage,
    ai_branch_generation_seed_for_thread, ai_branch_name_for_prompt, ai_branch_name_for_thread,
    ai_commit_message_for_thread, try_ai_branch_name_for_prompt, try_ai_commit_message,
};
use crate::app::markdown_links::open_url_in_browser;
use crate::app::workspace_view::WorkspaceSidebarKind;

use super::data::{
    DiffSegmentQuality, DiffStream, DiffStreamRowKind, RepoTreeNodeKind, build_changed_files_tree,
    build_diff_row_segment_cache_from_cells, build_diff_stream_from_patch_map, build_repo_tree,
    count_repo_tree_kind, flatten_repo_tree_rows, is_markdown_path, line_number_column_width,
    load_file_editor_document, save_file_editor_document,
};
use super::*;
use hunk_forge::{
    CreateReviewInput, ForgeCredentialKind, ForgeCredentialMetadata, ForgeCredentialResolution,
    ForgeRepoCredentialBinding, ForgeRepoRef, ForgeReviewClient, GitHubReviewClient,
    OpenReviewQuery, OpenReviewSummary, resolve_credential_for_repo,
};
use hunk_git::branch::{
    RenameBranchIfSafeOutcome, ReviewRemote, rename_branch_if_current_unpublished,
    review_remote_for_branch_with_provider_map, review_remote_for_named_remote_with_provider_map,
    review_url_for_branch_with_provider_map, sanitize_branch_name,
};
use hunk_git::compare::{CompareSource, load_compare_snapshot, resolve_default_base_branch_name};
use hunk_git::git::{
    RepoSnapshotFingerprint, WorkflowSnapshot, count_non_ignored_repo_tree_entries,
    invalidate_repo_metadata_caches, load_remote_tracking_branches_without_refresh,
    load_repo_file_line_stats_for_paths_without_refresh, load_repo_file_line_stats_without_refresh,
    load_repo_tree, load_snapshot_fingerprint, load_workflow_snapshot,
    load_workflow_snapshot_if_changed, load_workflow_snapshot_if_changed_without_refresh,
    load_workflow_snapshot_with_fingerprint,
    load_workflow_snapshot_with_fingerprint_without_refresh,
};
use hunk_git::history::{
    DEFAULT_RECENT_AUTHORED_COMMIT_LIMIT, load_recent_authored_commits_fingerprint,
    load_recent_authored_commits_if_changed, load_recent_authored_commits_with_fingerprint,
};
use hunk_git::mutation::{
    activate_or_create_branch as checkout_or_create_branch_with_change_transfer,
    commit_all_with_details as commit_staged_with_details, commit_index_with_details,
    create_branch_from_base_with_change_transfer, restore_working_copy_paths, stage_paths,
    staged_index_context_for_ai, unstage_paths, working_copy_context_for_ai,
};
use hunk_git::network::{
    fetch_remote_branches, pull_current_branch_with_rebase as pull_branch_with_rebase,
    push_current_branch, sync_branch_from_remote_if_tracked, sync_current_branch,
};

include!("core.rs");
include!("core_runtime.rs");
include!("markdown_links.rs");
include!("project_open.rs");
include!("forge_auth_store.rs");
include!("github_auth_flow.rs");
include!("git_ops_review.rs");
include!("git_ops.rs");
include!("recent_commits.rs");
include!("review_compare.rs");
include!("workspace_mode.rs");
include!("terminal_runtime_store.rs");
include!("ai.rs");
include!("ai_composer_completion.rs");
include!("ai_git_ops.rs");
include!("file_terminal.rs");
include!("file_tree.rs");
include!("file_tree_fs.rs");
include!("file_quick_open.rs");
include!("editor_reuse.rs");
include!("editor_search.rs");
include!("editor.rs");
include!("comments.rs");
include!("comments_match.rs");
include!("selection.rs");
include!("context_menu.rs");
include!("scroll.rs");
include!("desktop_notifications.rs");
include!("ai_perf.rs");
include!("fps.rs");
include!("about.rs");
include!("settings.rs");
include!("updates.rs");
