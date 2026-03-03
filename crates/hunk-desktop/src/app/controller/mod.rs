use anyhow::Context as _;
use futures::StreamExt;
use futures::channel::mpsc;
use notify::Watcher;
use tracing::{error, info};

use super::data::{
    DiffSegmentQuality, DiffStream, DiffStreamRowKind, RepoTreeNodeKind,
    base_segment_quality_for_file, build_changed_files_tree,
    build_diff_row_segment_cache_from_cells, build_diff_stream_from_patch_map, build_repo_tree,
    count_repo_tree_kind, decimal_digits, effective_segment_quality, flatten_repo_tree_rows,
    is_markdown_path, line_number_column_width, load_file_editor_document, message_row,
    save_file_editor_document,
};
use super::*;
use hunk_jj::jj::{
    GraphBookmarkScope, GraphSnapshot, GraphSnapshotOptions, RepoSnapshot, abandon_bookmark_head,
    checkout_or_create_bookmark_with_change_transfer, commit_selected_paths, commit_staged,
    count_non_ignored_repo_tree_entries, create_bookmark_at_revision, describe_bookmark_head,
    graph_bookmark_drop_validation, graph_bookmark_revision_chain, load_graph_snapshot,
    load_patches_for_files, load_repo_tree, load_snapshot, load_snapshot_fingerprint,
    move_bookmark_to_revision, push_current_bookmark,
    redo_last_operation as redo_last_jj_operation, rename_bookmark, reorder_bookmark_tip_older,
    restore_all_working_copy_changes, restore_working_copy_from_revision,
    restore_working_copy_paths, review_url_for_bookmark_with_provider_map, sanitize_bookmark_name,
    squash_bookmark_head_into_parent, sync_current_bookmark,
    undo_last_operation as undo_last_jj_operation,
};

include!("core.rs");
include!("core_runtime.rs");
include!("git_ops.rs");
include!("operation_history.rs");
include!("workspace_mode.rs");
include!("jj_graph.rs");
include!("file_tree.rs");
include!("editor.rs");
include!("comments.rs");
include!("comments_match.rs");
include!("selection.rs");
include!("scroll.rs");
include!("fps.rs");
include!("settings.rs");
