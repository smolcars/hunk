use std::collections::BTreeSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use jj_lib::object_id::ObjectId;
use jj_lib::ref_name::RefName;
use jj_lib::repo::Repo as _;
use tracing::warn;

use crate::config::ReviewProviderMapping;

mod backend;

use backend::{
    abandon_bookmark_head as abandon_local_bookmark_head, bookmark_remote_sync_state,
    bookmark_review_url, build_graph_snapshot_from_context, can_redo_operation, can_undo_operation,
    checkout_existing_bookmark, checkout_existing_bookmark_with_change_transfer,
    collect_materialized_diff_entries_for_paths, commit_working_copy_changes,
    commit_working_copy_selected_paths, conflict_materialize_options,
    create_bookmark_at_working_copy, current_bookmarks_from_context,
    current_commit_id_from_context, describe_bookmark_head as describe_local_bookmark_head,
    discover_repo_root, git_head_branch_name_from_context, last_commit_subject_from_context,
    list_bookmark_revisions_from_context, list_local_branches_from_context,
    load_changed_files_from_context, load_repo_context, load_repo_context_at_root,
    load_tracked_paths_from_context, materialized_entry_matches_path,
    move_bookmark_to_parent_of_working_copy, normalize_path, push_bookmark,
    redo_last_operation as redo_last_operation_in_context,
    rename_bookmark as rename_local_bookmark, render_patch_for_entry,
    reorder_bookmark_tip_older as reorder_local_bookmark_tip_older, repo_line_stats_from_context,
    restore_all_working_copy_changes as restore_all_wc_changes,
    restore_working_copy_from_revision as restore_wc_from_revision,
    restore_working_copy_selected_paths as restore_wc_selected_paths,
    set_local_bookmark_target_revision,
    squash_bookmark_head_into_parent as squash_local_bookmark_head_into_parent,
    sync_bookmark_from_remote, undo_last_operation as undo_last_operation_in_context,
    walk_repo_tree,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    TypeChange,
    Conflicted,
    Unknown,
}

impl FileStatus {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Modified => "M",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "U",
            Self::TypeChange => "T",
            Self::Conflicted => "!",
            Self::Unknown => "-",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,
    pub staged: bool,
    pub untracked: bool,
}

impl ChangedFile {
    pub fn is_tracked(&self) -> bool {
        !self.untracked
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBranch {
    pub name: String,
    pub is_current: bool,
    pub tip_unix_time: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkRevision {
    pub id: String,
    pub subject: String,
    pub unix_time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoTreeEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoTreeEntry {
    pub path: String,
    pub kind: RepoTreeEntryKind,
    pub ignored: bool,
}

#[derive(Debug, Clone)]
pub struct RepoSnapshot {
    pub root: PathBuf,
    pub branch_name: String,
    pub branch_has_upstream: bool,
    pub branch_ahead_count: usize,
    pub can_undo_operation: bool,
    pub can_redo_operation: bool,
    pub branches: Vec<LocalBranch>,
    pub bookmark_revisions: Vec<BookmarkRevision>,
    pub files: Vec<ChangedFile>,
    pub line_stats: LineStats,
    pub last_commit_subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowSnapshot {
    pub root: PathBuf,
    pub branch_name: String,
    pub branch_has_upstream: bool,
    pub branch_ahead_count: usize,
    pub can_undo_operation: bool,
    pub can_redo_operation: bool,
    pub branches: Vec<LocalBranch>,
    pub bookmark_revisions: Vec<BookmarkRevision>,
    pub files: Vec<ChangedFile>,
    pub last_commit_subject: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphBookmarkScope {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphBookmarkRef {
    pub name: String,
    pub remote: Option<String>,
    pub scope: GraphBookmarkScope,
    pub is_active: bool,
    pub tracked: bool,
    pub needs_push: bool,
    pub conflicted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: String,
    pub subject: String,
    pub unix_time: i64,
    pub bookmarks: Vec<GraphBookmarkRef>,
    pub is_working_copy_parent: bool,
    pub is_active_bookmark_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphSnapshotOptions {
    pub max_nodes: usize,
    pub offset: usize,
    pub include_remote_bookmarks: bool,
}

impl Default for GraphSnapshotOptions {
    fn default() -> Self {
        Self {
            max_nodes: 250,
            offset: 0,
            include_remote_bookmarks: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSnapshot {
    pub root: PathBuf,
    pub active_bookmark: Option<String>,
    pub working_copy_commit_id: String,
    pub working_copy_parent_commit_id: Option<String>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

pub fn graph_bookmark_revision_chain(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    bookmark_name: &str,
    bookmark_remote: Option<&str>,
    bookmark_scope: GraphBookmarkScope,
) -> Vec<String> {
    let Some(tip_node_id) =
        graph_bookmark_tip_node_id(nodes, bookmark_name, bookmark_remote, bookmark_scope)
    else {
        return Vec::new();
    };

    let node_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut stack = vec![tip_node_id.to_string()];
    let mut visited = BTreeSet::new();

    while let Some(node_id) = stack.pop() {
        if !visited.insert(node_id.clone()) {
            continue;
        }

        for edge in edges.iter().filter(|edge| edge.from == node_id) {
            if !node_ids.contains(edge.to.as_str()) {
                continue;
            }
            if !visited.contains(edge.to.as_str()) {
                stack.push(edge.to.clone());
            }
        }
    }

    let mut chain_nodes = visited
        .into_iter()
        .filter_map(|id| {
            nodes
                .iter()
                .find(|node| node.id == id)
                .map(|node| (node.unix_time, node.id.clone()))
        })
        .collect::<Vec<_>>();
    chain_nodes.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    chain_nodes
        .into_iter()
        .map(|(_, node_id)| node_id)
        .collect()
}

pub fn graph_bookmark_drop_validation(
    nodes: &[GraphNode],
    bookmark_name: &str,
    bookmark_remote: Option<&str>,
    bookmark_scope: GraphBookmarkScope,
    target_node_id: &str,
) -> std::result::Result<(), String> {
    if bookmark_scope != GraphBookmarkScope::Local {
        return Err("Only local bookmarks can be moved by drag-and-drop.".to_string());
    }
    if !nodes.iter().any(|node| node.id == target_node_id) {
        return Err("Drop target revision is not in the current graph window.".to_string());
    }
    let Some(source_node_id) =
        graph_bookmark_tip_node_id(nodes, bookmark_name, bookmark_remote, bookmark_scope)
    else {
        return Err(format!(
            "Bookmark '{}' is not present in the current graph window.",
            bookmark_name
        ));
    };
    if source_node_id == target_node_id {
        return Err("Bookmark is already targeting that revision.".to_string());
    }
    Ok(())
}

fn graph_bookmark_tip_node_id<'a>(
    nodes: &'a [GraphNode],
    bookmark_name: &str,
    bookmark_remote: Option<&str>,
    bookmark_scope: GraphBookmarkScope,
) -> Option<&'a str> {
    nodes
        .iter()
        .filter(|node| {
            node.bookmarks.iter().any(|bookmark| {
                graph_bookmark_matches(bookmark, bookmark_name, bookmark_remote, bookmark_scope)
            })
        })
        .max_by(|left, right| {
            left.unix_time
                .cmp(&right.unix_time)
                .then_with(|| right.id.cmp(&left.id))
        })
        .map(|node| node.id.as_str())
}

fn graph_bookmark_matches(
    bookmark: &GraphBookmarkRef,
    name: &str,
    remote: Option<&str>,
    scope: GraphBookmarkScope,
) -> bool {
    bookmark.name == name && bookmark.scope == scope && bookmark.remote.as_deref() == remote
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSnapshotFingerprint {
    root: PathBuf,
    branch_name: String,
    head_target: Option<String>,
    changed_file_count: usize,
    changed_file_signature: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineStats {
    pub added: u64,
    pub removed: u64,
}

impl LineStats {
    pub fn changed(self) -> u64 {
        self.added + self.removed
    }
}

#[derive(Debug, Clone)]
pub struct JjRepo {
    root: PathBuf,
}

pub(super) const MAX_REPO_TREE_ENTRIES: usize = 60_000;
const JJ_STAGE_UNSUPPORTED: &str =
    "JJ does not use a staging index. Stage/unstage actions are unavailable.";
const ACTIVE_BOOKMARK_FILE: &str = "hunk-active-bookmark";
const RESERVED_BOOKMARK_NAMES: &[&str] = &["detached", "unknown"];

fn load_workflow_snapshot_from_context(context: &backend::RepoContext) -> Result<WorkflowSnapshot> {
    let files = load_changed_files_from_context(context)?;
    let current_bookmarks = current_bookmarks_from_context(context)?;
    let active_bookmark = load_active_bookmark_preference(&context.root);
    let git_head_branch = git_head_branch_name_from_context(context);
    let branch_name =
        select_snapshot_branch_name(&current_bookmarks, active_bookmark, git_head_branch);
    let mut branch_selection = current_bookmarks.clone();
    if branch_selection.is_empty() && branch_name != "detached" {
        branch_selection.insert(branch_name.clone());
    }
    let branches = list_local_branches_from_context(context, &branch_selection)?;
    let bookmark_revisions = list_bookmark_revisions_from_context(context, &branch_name, 32)?;
    let (branch_has_upstream, branch_ahead_count) = if branch_name == "detached" {
        (false, 0)
    } else {
        bookmark_remote_sync_state(context, branch_name.as_str())
    };
    let can_undo_operation = can_undo_operation(context)?;
    let can_redo_operation = can_redo_operation(context)?;
    let last_commit_subject = last_commit_subject_from_context(context)?;

    Ok(WorkflowSnapshot {
        root: context.root.clone(),
        branch_name,
        branch_has_upstream,
        branch_ahead_count,
        can_undo_operation,
        can_redo_operation,
        branches,
        bookmark_revisions,
        files,
        last_commit_subject,
    })
}

pub fn load_snapshot(cwd: &Path) -> Result<RepoSnapshot> {
    let context = load_repo_context(cwd, true)?;
    let workflow = load_workflow_snapshot_from_context(&context)?;
    let line_stats = repo_line_stats_from_context(&context)?;
    Ok(RepoSnapshot {
        root: workflow.root,
        branch_name: workflow.branch_name,
        branch_has_upstream: workflow.branch_has_upstream,
        branch_ahead_count: workflow.branch_ahead_count,
        can_undo_operation: workflow.can_undo_operation,
        can_redo_operation: workflow.can_redo_operation,
        branches: workflow.branches,
        bookmark_revisions: workflow.bookmark_revisions,
        files: workflow.files,
        line_stats,
        last_commit_subject: workflow.last_commit_subject,
    })
}

pub fn load_workflow_snapshot(cwd: &Path) -> Result<WorkflowSnapshot> {
    let context = load_repo_context(cwd, true)?;
    load_workflow_snapshot_from_context(&context)
}

pub fn load_graph_snapshot(
    repo_root: &Path,
    options: GraphSnapshotOptions,
) -> Result<GraphSnapshot> {
    let context = load_repo_context_at_root(repo_root, true)?;
    build_graph_snapshot_from_context(&context, options)
}

pub fn load_snapshot_fingerprint(cwd: &Path) -> Result<RepoSnapshotFingerprint> {
    let context = load_repo_context(cwd, true)?;
    let files = load_changed_files_from_context(&context)?;
    let current_bookmarks = current_bookmarks_from_context(&context)?;
    let active_bookmark = load_active_bookmark_preference(&context.root);
    let git_head_branch = git_head_branch_name_from_context(&context);
    let branch_name =
        select_snapshot_branch_name(&current_bookmarks, active_bookmark, git_head_branch);
    let head_target = current_commit_id_from_context(&context)?;
    Ok(snapshot_fingerprint(
        context.root,
        branch_name,
        head_target,
        &files,
    ))
}

pub fn load_patch(repo_root: &Path, file_path: &str, status: FileStatus) -> Result<String> {
    let repo = open_repo_for_patch(repo_root)?;
    load_patch_from_open_repo(&repo, file_path, status)
}

pub fn open_repo_for_patch(repo_root: &Path) -> Result<JjRepo> {
    let root = discover_repo_root(repo_root)?;
    Ok(JjRepo { root })
}

pub fn load_patch_from_open_repo(repo: &JjRepo, file_path: &str, _: FileStatus) -> Result<String> {
    let context = load_repo_context_at_root(&repo.root, true)?;
    let normalized_file = normalize_path(file_path);
    let materialize_options = conflict_materialize_options(&context);
    let mut requested_paths = BTreeSet::new();
    requested_paths.insert(normalized_file.clone());

    for entry in collect_materialized_diff_entries_for_paths(&context, &requested_paths)? {
        if !materialized_entry_matches_path(&entry, normalized_file.as_str()) {
            continue;
        }
        let rendered = render_patch_for_entry(entry, &materialize_options)?;
        return Ok(rendered.patch);
    }

    Ok(String::new())
}

pub fn load_patches_for_files(
    repo_root: &Path,
    files: &[ChangedFile],
) -> Result<std::collections::BTreeMap<String, String>> {
    let context = load_repo_context_at_root(repo_root, true)?;
    let materialize_options = conflict_materialize_options(&context);
    let requested_paths = files
        .iter()
        .map(|file| normalize_path(file.path.as_str()))
        .filter(|path| !path.is_empty())
        .collect::<BTreeSet<_>>();

    if requested_paths.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }

    let mut patch_map = std::collections::BTreeMap::new();
    for entry in collect_materialized_diff_entries_for_paths(&context, &requested_paths)? {
        let source_path = normalize_path(entry.path.source().as_internal_file_string());
        let target_path = normalize_path(entry.path.target().as_internal_file_string());
        let source_matches =
            !source_path.is_empty() && requested_paths.contains(source_path.as_str());
        let target_matches =
            !target_path.is_empty() && requested_paths.contains(target_path.as_str());
        if !source_matches && !target_matches {
            continue;
        }

        let rendered = match render_patch_for_entry(entry, &materialize_options) {
            Ok(rendered) => rendered,
            Err(err) => {
                warn!(
                    "failed to render patch for paths source='{}' target='{}': {err:#}",
                    source_path, target_path
                );
                continue;
            }
        };
        if target_matches {
            patch_map
                .entry(target_path.clone())
                .or_insert_with(|| rendered.patch.clone());
        }
        if source_matches && source_path != target_path {
            patch_map.entry(source_path).or_insert(rendered.patch);
        }
    }

    for path in requested_paths {
        patch_map.entry(path).or_default();
    }

    Ok(patch_map)
}

pub fn load_repo_tree(repo_root: &Path) -> Result<Vec<RepoTreeEntry>> {
    let context = load_repo_context_at_root(repo_root, true)?;
    let tracked_paths = load_tracked_paths_from_context(&context)?;
    let mut entries = Vec::new();
    walk_repo_tree(
        context.root.as_path(),
        context.root.as_path(),
        &tracked_paths,
        &mut entries,
    )?;
    Ok(entries)
}

pub fn count_non_ignored_repo_tree_entries(entries: &[RepoTreeEntry]) -> (usize, usize) {
    let mut file_count = 0usize;
    let mut folder_count = 0usize;

    for entry in entries {
        if entry.ignored {
            continue;
        }

        match entry.kind {
            RepoTreeEntryKind::File => file_count += 1,
            RepoTreeEntryKind::Directory => folder_count += 1,
        }
    }

    (file_count, folder_count)
}

pub fn stage_file(_: &Path, _: &str) -> Result<()> {
    Err(anyhow!(JJ_STAGE_UNSUPPORTED))
}

pub fn unstage_file(_: &Path, _: &str) -> Result<()> {
    Err(anyhow!(JJ_STAGE_UNSUPPORTED))
}

pub fn stage_all(_: &Path) -> Result<()> {
    Err(anyhow!(JJ_STAGE_UNSUPPORTED))
}

pub fn unstage_all(_: &Path) -> Result<()> {
    Err(anyhow!(JJ_STAGE_UNSUPPORTED))
}

pub fn commit_staged(repo_root: &Path, message: &str) -> Result<()> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("commit message cannot be empty"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    if load_changed_files_from_context(&context)?.is_empty() {
        return Err(anyhow!("no changes to commit"));
    }
    let active_bookmark = resolved_active_bookmark(&context)?;

    commit_working_copy_changes(&mut context, trimmed)?;

    if let Some(active_bookmark) = active_bookmark {
        move_bookmark_to_parent_of_working_copy(&mut context, active_bookmark.as_str())?;
    }

    Ok(())
}

pub fn commit_selected_paths(
    repo_root: &Path,
    message: &str,
    selected_paths: &[String],
) -> Result<usize> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("commit message cannot be empty"));
    }
    if selected_paths.is_empty() {
        return Err(anyhow!("no files selected for commit"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    if load_changed_files_from_context(&context)?.is_empty() {
        return Err(anyhow!("no changes to commit"));
    }
    let active_bookmark = resolved_active_bookmark(&context)?;

    let committed_count =
        commit_working_copy_selected_paths(&mut context, trimmed, selected_paths)?;

    if let Some(active_bookmark) = active_bookmark {
        move_bookmark_to_parent_of_working_copy(&mut context, active_bookmark.as_str())?;
    }

    Ok(committed_count)
}

pub fn checkout_or_create_bookmark(repo_root: &Path, bookmark_name: &str) -> Result<()> {
    checkout_or_create_bookmark_with_change_transfer(repo_root, bookmark_name, false)
}

pub fn restore_working_copy_from_revision(
    repo_root: &Path,
    source_revision_id: &str,
) -> Result<()> {
    let source_revision_id = source_revision_id.trim();
    if source_revision_id.is_empty() {
        return Err(anyhow!("source revision id cannot be empty"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    restore_wc_from_revision(&mut context, source_revision_id)
}

pub fn restore_working_copy_paths(repo_root: &Path, paths: &[String]) -> Result<usize> {
    let mut normalized_paths = BTreeSet::new();
    for path in paths {
        let normalized = normalize_path(path);
        if normalized.is_empty() {
            continue;
        }
        normalized_paths.insert(normalized);
    }
    if normalized_paths.is_empty() {
        return Err(anyhow!("no files selected to restore"));
    }

    let selected_paths = normalized_paths.into_iter().collect::<Vec<_>>();
    let mut context = load_repo_context_at_root(repo_root, true)?;
    restore_wc_selected_paths(&mut context, selected_paths.as_slice())
}

pub fn restore_all_working_copy_changes(repo_root: &Path) -> Result<()> {
    let mut context = load_repo_context_at_root(repo_root, true)?;
    restore_all_wc_changes(&mut context)
}

pub fn can_undo_last_operation(repo_root: &Path) -> Result<bool> {
    let context = load_repo_context_at_root(repo_root, false)?;
    can_undo_operation(&context)
}

pub fn undo_last_operation(repo_root: &Path) -> Result<()> {
    let mut context = load_repo_context_at_root(repo_root, true)?;
    undo_last_operation_in_context(&mut context)
}

pub fn can_redo_last_operation(repo_root: &Path) -> Result<bool> {
    let context = load_repo_context_at_root(repo_root, false)?;
    can_redo_operation(&context)
}

pub fn redo_last_operation(repo_root: &Path) -> Result<()> {
    let mut context = load_repo_context_at_root(repo_root, true)?;
    redo_last_operation_in_context(&mut context)
}

pub fn create_bookmark_at_revision(
    repo_root: &Path,
    bookmark_name: &str,
    revision_id: &str,
) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() {
        return Err(anyhow!("bookmark name cannot be empty"));
    }
    if !is_valid_bookmark_name(bookmark_name) {
        return Err(anyhow!("invalid bookmark name: {bookmark_name}"));
    }

    let revision_id = revision_id.trim();
    if revision_id.is_empty() {
        return Err(anyhow!("revision id cannot be empty"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    set_local_bookmark_target_revision(&mut context, bookmark_name, revision_id, false)
}

pub fn move_bookmark_to_revision(
    repo_root: &Path,
    bookmark_name: &str,
    revision_id: &str,
) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() {
        return Err(anyhow!("bookmark name cannot be empty"));
    }
    if bookmark_name == "detached" {
        return Err(anyhow!("cannot move detached bookmark"));
    }

    let revision_id = revision_id.trim();
    if revision_id.is_empty() {
        return Err(anyhow!("revision id cannot be empty"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    set_local_bookmark_target_revision(&mut context, bookmark_name, revision_id, true)
}

pub fn rename_bookmark(
    repo_root: &Path,
    old_bookmark_name: &str,
    new_bookmark_name: &str,
) -> Result<()> {
    let old_bookmark_name = old_bookmark_name.trim();
    if old_bookmark_name.is_empty() {
        return Err(anyhow!("current bookmark name cannot be empty"));
    }

    let new_bookmark_name = new_bookmark_name.trim();
    if new_bookmark_name.is_empty() {
        return Err(anyhow!("new bookmark name cannot be empty"));
    }
    if old_bookmark_name == new_bookmark_name {
        return Err(anyhow!(
            "new bookmark name must differ from current bookmark"
        ));
    }
    if !is_valid_bookmark_name(new_bookmark_name) {
        return Err(anyhow!("invalid bookmark name: {new_bookmark_name}"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    rename_local_bookmark(&mut context, old_bookmark_name, new_bookmark_name)?;

    if load_active_bookmark_preference(&context.root).as_deref() == Some(old_bookmark_name)
        && let Err(err) = persist_active_bookmark_preference(&context.root, new_bookmark_name)
    {
        warn!(
            "failed to persist active bookmark preference for '{}': {err:#}",
            new_bookmark_name
        );
    }

    Ok(())
}

pub fn describe_bookmark_head(
    repo_root: &Path,
    bookmark_name: &str,
    description: &str,
) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() || bookmark_name == "detached" {
        return Err(anyhow!(
            "cannot edit revision description without a bookmark name"
        ));
    }

    let description = description.trim();
    if description.is_empty() {
        return Err(anyhow!("revision description cannot be empty"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    describe_local_bookmark_head(&mut context, bookmark_name, description)
}

pub fn abandon_bookmark_head(repo_root: &Path, bookmark_name: &str) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() || bookmark_name == "detached" {
        return Err(anyhow!("cannot abandon a revision without a bookmark name"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    abandon_local_bookmark_head(&mut context, bookmark_name)
}

pub fn squash_bookmark_head_into_parent(repo_root: &Path, bookmark_name: &str) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() || bookmark_name == "detached" {
        return Err(anyhow!("cannot squash a revision without a bookmark name"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    squash_local_bookmark_head_into_parent(&mut context, bookmark_name)
}

pub fn reorder_bookmark_tip_older(repo_root: &Path, bookmark_name: &str) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() || bookmark_name == "detached" {
        return Err(anyhow!("cannot reorder revisions without a bookmark name"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    reorder_local_bookmark_tip_older(&mut context, bookmark_name)
}

pub fn review_url_for_bookmark(repo_root: &Path, bookmark_name: &str) -> Result<Option<String>> {
    review_url_for_bookmark_with_provider_map(repo_root, bookmark_name, &[])
}

pub fn review_url_for_bookmark_with_provider_map(
    repo_root: &Path,
    bookmark_name: &str,
    provider_mappings: &[ReviewProviderMapping],
) -> Result<Option<String>> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() || bookmark_name == "detached" {
        return Err(anyhow!("cannot build review URL without a bookmark name"));
    }

    let context = load_repo_context_at_root(repo_root, false)?;
    bookmark_review_url(&context, bookmark_name, provider_mappings)
}

pub fn checkout_or_create_bookmark_with_change_transfer(
    repo_root: &Path,
    bookmark_name: &str,
    move_changes_to_bookmark: bool,
) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() {
        return Err(anyhow!("bookmark name cannot be empty"));
    }
    if !is_valid_bookmark_name(bookmark_name) {
        return Err(anyhow!("invalid bookmark name: {bookmark_name}"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    let ref_name = RefName::new(bookmark_name);
    let bookmark_target = context.repo.view().get_local_bookmark(ref_name);
    if bookmark_target.is_present() {
        if move_changes_to_bookmark {
            checkout_existing_bookmark_with_change_transfer(&mut context, bookmark_name)?;
        } else {
            checkout_existing_bookmark(&mut context, bookmark_name)?;
        }
    } else {
        let previous_bookmarks = if move_changes_to_bookmark {
            current_bookmarks_from_context(&context)?
        } else {
            BTreeSet::new()
        };
        create_bookmark_at_working_copy(&mut context, bookmark_name)?;
        if move_changes_to_bookmark {
            for bookmark in previous_bookmarks {
                if bookmark != bookmark_name {
                    move_bookmark_to_parent_of_working_copy(&mut context, bookmark.as_str())?;
                }
            }
        } else {
            move_bookmark_to_parent_of_working_copy(&mut context, bookmark_name)?;
        }
    }

    if let Err(err) = persist_active_bookmark_preference(&context.root, bookmark_name) {
        warn!(
            "failed to persist active bookmark preference for '{}': {err:#}",
            bookmark_name
        );
    }

    Ok(())
}

pub fn push_current_bookmark(repo_root: &Path, bookmark_name: &str, _: bool) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() || bookmark_name == "detached" {
        return Err(anyhow!("cannot push without a bookmark name"));
    }

    let mut context = load_repo_context_at_root(repo_root, false)?;
    push_bookmark(&mut context, bookmark_name)
}

pub fn sync_current_bookmark(repo_root: &Path, bookmark_name: &str) -> Result<()> {
    let bookmark_name = bookmark_name.trim();
    if bookmark_name.is_empty() || bookmark_name == "detached" {
        return Err(anyhow!("cannot sync without a bookmark name"));
    }

    let mut context = load_repo_context_at_root(repo_root, true)?;
    sync_bookmark_from_remote(&mut context, bookmark_name)
}

pub fn sanitize_bookmark_name(input: &str) -> String {
    let lowered = input.trim().to_lowercase();

    let mut normalized = String::with_capacity(lowered.len());
    let mut last_dash = false;
    for ch in lowered.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' | '/' | '.' | '_' | '-' => ch,
            c if c.is_whitespace() => '-',
            _ => '-',
        };

        if mapped == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }

        normalized.push(mapped);
    }

    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        let mut clean = segment
            .trim_matches(|c: char| c == '-' || c == '.')
            .replace("@{", "-")
            .replace(['~', '^', ':', '?', '*', '[', '\\'], "-");

        while clean.contains("--") {
            clean = clean.replace("--", "-");
        }

        while clean.contains("..") {
            clean = clean.replace("..", ".");
        }

        if clean.ends_with(".lock") {
            clean = clean
                .trim_end_matches(".lock")
                .trim_end_matches('.')
                .to_string();
        }

        if !clean.is_empty() {
            segments.push(clean);
        }
    }

    let mut candidate = if segments.is_empty() {
        "bookmark".to_string()
    } else {
        segments.join("/")
    };

    if candidate.eq_ignore_ascii_case("head") {
        candidate = "head-bookmark".to_string();
    }
    if is_reserved_bookmark_name(candidate.as_str()) {
        candidate = format!("{candidate}-bookmark");
    }

    candidate = candidate.trim_matches('/').to_string();

    if !is_valid_bookmark_name(&candidate) {
        candidate = candidate
            .chars()
            .map(|ch| match ch {
                'a'..='z' | '0'..='9' | '-' | '_' | '.' | '/' => ch,
                _ => '-',
            })
            .collect::<String>();

        candidate = candidate
            .split('/')
            .filter_map(|segment| {
                let segment = segment.trim_matches(|c: char| c == '-' || c == '.');
                if segment.is_empty() {
                    None
                } else {
                    Some(segment.to_string())
                }
            })
            .collect::<Vec<_>>()
            .join("/");
    }

    if candidate.is_empty() {
        candidate = "bookmark".to_string();
    }

    if !is_valid_bookmark_name(&candidate) {
        "bookmark-new".to_string()
    } else {
        candidate
    }
}

pub fn is_valid_bookmark_name(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    if is_reserved_bookmark_name(name) {
        return false;
    }

    if name.starts_with('/') || name.ends_with('/') {
        return false;
    }

    if name.starts_with('.') || name.ends_with('.') {
        return false;
    }

    if name.contains("//") || name.contains("..") || name.contains("@{") || name.ends_with(".lock")
    {
        return false;
    }

    if name.chars().any(|ch| {
        ch.is_ascii_control()
            || ch.is_whitespace()
            || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    }) {
        return false;
    }

    name.split('/').all(|segment| {
        !segment.is_empty()
            && !segment.starts_with('.')
            && !segment.ends_with('.')
            && segment != "@"
    })
}

fn is_reserved_bookmark_name(name: &str) -> bool {
    RESERVED_BOOKMARK_NAMES
        .iter()
        .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

fn active_bookmark_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".jj").join(ACTIVE_BOOKMARK_FILE)
}

fn load_active_bookmark_preference(repo_root: &Path) -> Option<String> {
    let path = active_bookmark_path(repo_root);
    let raw = fs::read_to_string(path).ok()?;
    let bookmark = raw.trim();
    if bookmark.is_empty() {
        None
    } else {
        Some(bookmark.to_string())
    }
}

fn persist_active_bookmark_preference(repo_root: &Path, branch_name: &str) -> Result<()> {
    let path = active_bookmark_path(repo_root);
    fs::write(&path, format!("{branch_name}\n")).map_err(|err| {
        anyhow!(
            "failed to write active bookmark preference {}: {err}",
            path.display()
        )
    })
}

fn select_snapshot_branch_name(
    current_bookmarks: &BTreeSet<String>,
    preferred: Option<String>,
    git_head_branch: Option<String>,
) -> String {
    if let Some(preferred) = preferred
        && current_bookmarks.contains(preferred.as_str())
    {
        return preferred;
    }

    if let Some(git_head_branch) = git_head_branch
        && current_bookmarks.contains(git_head_branch.as_str())
    {
        return git_head_branch;
    }

    current_bookmarks
        .iter()
        .next()
        .cloned()
        .unwrap_or_else(|| "detached".to_string())
}

fn select_commit_branch_name(
    context: &backend::RepoContext,
    current_bookmarks: &BTreeSet<String>,
    parent_bookmarks: &BTreeSet<String>,
    preferred: Option<String>,
    git_head_branch: Option<String>,
) -> String {
    let preferred = preferred.filter(|name| current_bookmarks.contains(name.as_str()));
    let git_head_branch = git_head_branch.filter(|name| current_bookmarks.contains(name.as_str()));

    if !parent_bookmarks.is_empty() {
        if let (Some(preferred), Some(git_head_branch)) = (&preferred, &git_head_branch)
            && parent_bookmarks.contains(preferred.as_str())
            && parent_bookmarks.contains(git_head_branch.as_str())
        {
            if preferred == git_head_branch {
                return preferred.clone();
            }

            let preferred_target = local_bookmark_target_hex(context, preferred.as_str());
            let git_head_target = local_bookmark_target_hex(context, git_head_branch.as_str());
            if preferred_target.is_some() && preferred_target == git_head_target {
                return preferred.clone();
            }

            return git_head_branch.clone();
        }

        if let Some(git_head_branch) = &git_head_branch
            && parent_bookmarks.contains(git_head_branch.as_str())
        {
            return git_head_branch.clone();
        }

        if let Some(preferred) = &preferred
            && parent_bookmarks.contains(preferred.as_str())
        {
            return preferred.clone();
        }

        if let Some(parent_bookmark) = parent_bookmarks.iter().next() {
            return parent_bookmark.clone();
        }
    }

    if let (Some(preferred), Some(git_head_branch)) = (&preferred, &git_head_branch) {
        if preferred == git_head_branch {
            return preferred.clone();
        }

        let preferred_target = local_bookmark_target_hex(context, preferred.as_str());
        let git_head_target = local_bookmark_target_hex(context, git_head_branch.as_str());
        if preferred_target.is_some() && preferred_target == git_head_target {
            return preferred.clone();
        }

        return git_head_branch.clone();
    }

    if let Some(preferred) = preferred {
        return preferred;
    }
    if let Some(git_head_branch) = git_head_branch {
        return git_head_branch;
    }

    current_bookmarks
        .iter()
        .next()
        .cloned()
        .unwrap_or_else(|| "detached".to_string())
}

fn resolved_active_bookmark(context: &backend::RepoContext) -> Result<Option<String>> {
    let current_bookmarks = current_bookmarks_from_context(context)?;
    let parent_bookmarks = parent_bookmarks_from_context(context)?;
    let branch_name = select_commit_branch_name(
        context,
        &current_bookmarks,
        &parent_bookmarks,
        load_active_bookmark_preference(&context.root),
        git_head_branch_name_from_context(context),
    );
    if branch_name == "detached" {
        Ok(None)
    } else {
        Ok(Some(branch_name))
    }
}

fn local_bookmark_target_hex(
    context: &backend::RepoContext,
    bookmark_name: &str,
) -> Option<String> {
    let target = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(bookmark_name));
    target.as_normal().map(|id| id.hex().to_string())
}

fn parent_bookmarks_from_context(context: &backend::RepoContext) -> Result<BTreeSet<String>> {
    let Some(wc_commit_id) = context
        .repo
        .view()
        .get_wc_commit_id(context.workspace.workspace_name())
    else {
        return Ok(BTreeSet::new());
    };
    let wc_commit = context
        .repo
        .store()
        .get_commit(wc_commit_id)
        .map_err(|err| anyhow!("failed to load working-copy commit: {err}"))?;
    let Some(parent_id) = wc_commit.parent_ids().first() else {
        return Ok(BTreeSet::new());
    };

    Ok(context
        .repo
        .view()
        .local_bookmarks_for_commit(parent_id)
        .map(|(name, _)| name.as_str().to_string())
        .collect())
}

fn snapshot_fingerprint(
    root: PathBuf,
    branch_name: String,
    head_target: Option<String>,
    files: &[ChangedFile],
) -> RepoSnapshotFingerprint {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file in files {
        file.path.hash(&mut hasher);
        file.status.tag().hash(&mut hasher);
        file.staged.hash(&mut hasher);
        file.untracked.hash(&mut hasher);
    }

    RepoSnapshotFingerprint {
        root,
        branch_name,
        head_target,
        changed_file_count: files.len(),
        changed_file_signature: hasher.finish(),
    }
}
