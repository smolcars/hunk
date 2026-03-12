use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read as _;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use anyhow::{Context as _, Result, anyhow};
use gix::bstr::{BStr, ByteSlice as _};
use gix::diff::blob::intern::InternedInput;
use gix::filter::plumbing::pipeline::convert::ToGitOutcome;

use crate::worktree::{
    WorkspaceTargetKind, list_workspace_targets, repo_relative_path_is_within_managed_worktrees,
};

pub const MAX_REPO_TREE_ENTRIES: usize = 60_000;

static NESTED_REPO_ROOTS_CACHE: LazyLock<Mutex<HashMap<PathBuf, NestedRepoPathCache>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub const fn tag(self) -> &'static str {
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
    pub unstaged: bool,
    pub untracked: bool,
}

impl ChangedFile {
    pub const fn is_tracked(&self) -> bool {
        !self.untracked
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBranch {
    pub name: String,
    pub is_current: bool,
    pub tip_unix_time: Option<i64>,
    pub attached_workspace_target_id: Option<String>,
    pub attached_workspace_target_root: Option<PathBuf>,
    pub attached_workspace_target_label: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineStats {
    pub added: u64,
    pub removed: u64,
}

impl LineStats {
    pub const fn changed(self) -> u64 {
        self.added + self.removed
    }
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
    pub working_copy_commit_id: String,
    pub branch_name: String,
    pub branch_has_upstream: bool,
    pub branch_ahead_count: usize,
    pub branch_behind_count: usize,
    pub branches: Vec<LocalBranch>,
    pub files: Vec<ChangedFile>,
    pub line_stats: LineStats,
    pub last_commit_subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowSnapshot {
    pub root: PathBuf,
    pub working_copy_commit_id: String,
    pub branch_name: String,
    pub branch_has_upstream: bool,
    pub branch_ahead_count: usize,
    pub branch_behind_count: usize,
    pub branches: Vec<LocalBranch>,
    pub files: Vec<ChangedFile>,
    pub last_commit_subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSnapshotFingerprint {
    root: PathBuf,
    head_ref_name: Option<String>,
    head_commit_id: Option<String>,
    branch_has_upstream: bool,
    branch_ahead_count: usize,
    branch_behind_count: usize,
    changed_file_count: usize,
    changed_file_signature: u64,
}

impl RepoSnapshotFingerprint {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn head_ref_name(&self) -> Option<&str> {
        self.head_ref_name.as_deref()
    }

    pub fn head_commit_id(&self) -> Option<&str> {
        self.head_commit_id.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct GitRepo {
    root: PathBuf,
    repo: gix::Repository,
}

#[derive(Debug, Clone)]
pub struct GitPatchSession {
    repo: GitRepo,
}

#[derive(Debug, Clone, Default)]
struct CandidateFile {
    staged_status: Option<FileStatus>,
    worktree_status: Option<FileStatus>,
    staged_rename_from: Option<String>,
    worktree_rename_from: Option<String>,
    rename_from: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkspaceDiffEntry {
    file: ChangedFile,
    line_stats: LineStats,
    content_signature: u64,
}

#[derive(Debug, Clone, Default)]
struct NestedRepoPathCache {
    nested_roots: BTreeSet<String>,
    checked_paths: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct FileState {
    kind: gix::objs::tree::EntryKind,
    id: gix::ObjectId,
    bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKindClass {
    Regular,
    Link,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotLoadMode {
    ReadOnlyLight,
    RefreshWorkingCopy,
}

#[derive(Debug, Clone, Copy)]
struct HeadEntrySummary {
    kind: gix::objs::tree::EntryKind,
    id: gix::ObjectId,
}

#[derive(Debug, Clone, Copy)]
struct WorktreeEntrySummary {
    kind: gix::objs::tree::EntryKind,
    signature: u64,
}

#[derive(Debug, Clone)]
struct SnapshotSeed {
    root: PathBuf,
    head_ref_name: Option<String>,
    head_commit_id: Option<String>,
    branch_name: String,
    branch_has_upstream: bool,
    branch_ahead_count: usize,
    branch_behind_count: usize,
    branches: Vec<LocalBranch>,
    entries: BTreeMap<String, WorkspaceDiffEntry>,
    last_commit_subject: Option<String>,
}

#[derive(Debug, Clone)]
struct BranchWorkspaceOccupancy {
    target_id: String,
    target_root: PathBuf,
    target_label: String,
}

#[derive(Debug, Clone)]
struct ResolvedWorkspaceFile {
    path: String,
    rename_from: Option<String>,
    status: FileStatus,
    staged: bool,
    unstaged: bool,
    untracked: bool,
    content_signature: u64,
    old_state: Option<FileState>,
    new_state: Option<FileState>,
}

struct NestedRepoFilter<'a> {
    root: &'a Path,
    cache: NestedRepoPathCache,
}

impl<'a> NestedRepoFilter<'a> {
    fn load(root: &'a Path) -> Self {
        let cache = nested_repo_roots_cache_guard()
            .get(root)
            .cloned()
            .unwrap_or_default();
        Self { root, cache }
    }

    fn contains_path(&mut self, path: &str) -> bool {
        if path.is_empty() || path_is_within_nested_repo(path, &self.cache.nested_roots) {
            return !path.is_empty();
        }

        let mut current = String::new();
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(component);

            if self.cache.checked_paths.contains(current.as_str()) {
                continue;
            }
            if directory_is_repo_root(self.root.join(current.as_str()).as_path()) {
                self.cache.nested_roots.insert(current);
                return true;
            }
            self.cache.checked_paths.insert(current.clone());
        }

        false
    }

    fn persist(self) {
        nested_repo_roots_cache_guard().insert(self.root.to_path_buf(), self.cache);
    }
}

impl GitRepo {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn git_dir(&self) -> &Path {
        self.repo.git_dir()
    }

    pub fn workdir(&self) -> Option<&Path> {
        self.repo.workdir()
    }

    pub const fn repository(&self) -> &gix::Repository {
        &self.repo
    }

    pub fn into_repository(self) -> gix::Repository {
        self.repo
    }

    pub fn snapshot_fingerprint(&self) -> Result<RepoSnapshotFingerprint> {
        let seed = load_snapshot_seed(self, false, SnapshotLoadMode::RefreshWorkingCopy)?;
        Ok(snapshot_fingerprint(
            seed.root,
            seed.head_ref_name,
            seed.head_commit_id,
            seed.branch_has_upstream,
            seed.branch_ahead_count,
            seed.branch_behind_count,
            &seed.entries,
        ))
    }
}

pub fn discover_repo_root(path: &Path) -> Result<PathBuf> {
    let repo = gix::discover(path)
        .with_context(|| format!("failed to discover Git repository from {}", path.display()))?;
    repo_root_from_repository(&repo)
}

pub fn open_repo(path: &Path) -> Result<GitRepo> {
    let root = discover_repo_root(path)?;
    open_repo_at_root(root.as_path())
}

pub fn open_repo_at_root(repo_root: &Path) -> Result<GitRepo> {
    let canonical_repo_root = canonicalize_existing_path(repo_root)?;
    let mut repo = gix::open(canonical_repo_root.as_path()).with_context(|| {
        format!(
            "failed to open Git repository at {}",
            canonical_repo_root.display()
        )
    })?;
    if let Ok(index) = repo.index_or_empty() {
        let cache_bytes = repo.compute_object_cache_size_for_tree_diffs(&index);
        repo.object_cache_size_if_unset(cache_bytes);
    }
    let root = repo_root_from_repository(&repo)?;
    Ok(GitRepo { root, repo })
}

pub fn load_snapshot(path: &Path) -> Result<RepoSnapshot> {
    load_snapshot_internal(path)
}

pub fn load_snapshot_without_refresh(path: &Path) -> Result<RepoSnapshot> {
    // RepoSnapshot always carries aggregate line stats, so this API intentionally stays on the
    // detailed snapshot path until there is a separate light-weight repo snapshot shape.
    load_snapshot_internal(path)
}

pub fn load_workflow_snapshot(path: &Path) -> Result<WorkflowSnapshot> {
    let (_, workflow) =
        load_workflow_snapshot_internal(path, SnapshotLoadMode::RefreshWorkingCopy)?;
    Ok(workflow)
}

pub fn load_workflow_snapshot_without_refresh(path: &Path) -> Result<WorkflowSnapshot> {
    let (_, workflow) = load_workflow_snapshot_internal(path, SnapshotLoadMode::ReadOnlyLight)?;
    Ok(workflow)
}

pub fn load_workflow_snapshot_with_fingerprint(
    path: &Path,
) -> Result<(RepoSnapshotFingerprint, WorkflowSnapshot)> {
    load_workflow_snapshot_internal(path, SnapshotLoadMode::RefreshWorkingCopy)
}

pub fn load_workflow_snapshot_with_fingerprint_without_refresh(
    path: &Path,
) -> Result<(RepoSnapshotFingerprint, WorkflowSnapshot)> {
    load_workflow_snapshot_internal(path, SnapshotLoadMode::ReadOnlyLight)
}

pub fn load_workflow_snapshot_if_changed(
    path: &Path,
    previous_fingerprint: Option<&RepoSnapshotFingerprint>,
) -> Result<(RepoSnapshotFingerprint, Option<WorkflowSnapshot>)> {
    load_workflow_snapshot_if_changed_internal(
        path,
        previous_fingerprint,
        SnapshotLoadMode::RefreshWorkingCopy,
    )
}

pub fn load_workflow_snapshot_if_changed_without_refresh(
    path: &Path,
    previous_fingerprint: Option<&RepoSnapshotFingerprint>,
) -> Result<(RepoSnapshotFingerprint, Option<WorkflowSnapshot>)> {
    load_workflow_snapshot_if_changed_internal(
        path,
        previous_fingerprint,
        SnapshotLoadMode::ReadOnlyLight,
    )
}

pub fn load_snapshot_fingerprint(path: &Path) -> Result<RepoSnapshotFingerprint> {
    load_snapshot_fingerprint_internal(path, SnapshotLoadMode::RefreshWorkingCopy)
}

pub fn load_snapshot_fingerprint_without_refresh(path: &Path) -> Result<RepoSnapshotFingerprint> {
    load_snapshot_fingerprint_internal(path, SnapshotLoadMode::ReadOnlyLight)
}

pub fn load_patch(repo_root: &Path, file_path: &str, _: FileStatus) -> Result<String> {
    let session = open_patch_session(repo_root)?;
    render_patch_for_path(&session.repo, file_path)
}

pub fn load_patches_for_files(
    repo_root: &Path,
    files: &[ChangedFile],
) -> Result<BTreeMap<String, String>> {
    let session = open_patch_session(repo_root)?;
    load_patches_for_files_from_session(&session, files)
}

pub fn open_patch_session(repo_root: &Path) -> Result<GitPatchSession> {
    let repo = open_repo(repo_root)?;
    Ok(GitPatchSession { repo })
}

pub fn load_patches_for_files_from_session(
    session: &GitPatchSession,
    files: &[ChangedFile],
) -> Result<BTreeMap<String, String>> {
    load_patches_for_files_from_repo(&session.repo, files)
}

pub fn expand_selected_paths_for_renames(
    repo_root: &Path,
    selected_paths: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    let repo = open_repo(repo_root)?;
    expand_selected_paths_for_renames_from_repo(&repo, selected_paths)
}

pub fn load_repo_line_stats(path: &Path) -> Result<LineStats> {
    let stats = load_repo_file_line_stats(path)?;
    Ok(sum_line_stats(stats.into_values()))
}

pub fn load_repo_line_stats_without_refresh(path: &Path) -> Result<LineStats> {
    // Exact line stats currently require the detailed diff path, even for callers that otherwise
    // prefer the workflow "without refresh" fast path.
    load_repo_line_stats(path)
}

pub fn load_repo_file_line_stats_without_refresh(
    path: &Path,
) -> Result<BTreeMap<String, LineStats>> {
    // Exact line stats currently require the detailed diff path, even for callers that otherwise
    // prefer the workflow "without refresh" fast path.
    load_repo_file_line_stats(path)
}

pub fn load_repo_file_line_stats_for_paths_without_refresh(
    path: &Path,
    paths: &BTreeSet<String>,
) -> Result<BTreeMap<String, LineStats>> {
    let repo = open_repo(path)?;
    let entries =
        collect_workspace_diff_entries_full(repo.repository(), repo.root(), Some(paths), true)?;
    Ok(entries
        .into_iter()
        .map(|(path, entry)| (path, entry.line_stats))
        .collect())
}

pub fn load_repo_tree(repo_root: &Path) -> Result<Vec<RepoTreeEntry>> {
    let repo = open_repo_at_root(repo_root)?;
    let visible_paths = load_visible_repo_paths(repo.repository(), repo.root())?;
    let mut entries = Vec::new();
    walk_repo_tree(repo.root(), repo.root(), &visible_paths, &mut entries)?;
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

pub fn invalidate_repo_metadata_caches(repo_root: &Path) {
    let root = repo_root.to_path_buf();
    let mut cache = nested_repo_roots_cache_guard();
    cache.remove(&root);
}
include!("git/snapshot.rs");
include!("git/workspace.rs");
include!("git/patch.rs");
include!("git/tree.rs");
