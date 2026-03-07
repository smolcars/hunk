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

pub const MAX_REPO_TREE_ENTRIES: usize = 60_000;

static NESTED_REPO_ROOTS_CACHE: LazyLock<Mutex<HashMap<PathBuf, BTreeSet<String>>>> =
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
struct ResolvedWorkspaceFile {
    path: String,
    rename_from: Option<String>,
    status: FileStatus,
    staged: bool,
    untracked: bool,
    content_signature: u64,
    old_state: Option<FileState>,
    new_state: Option<FileState>,
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

pub fn load_repo_line_stats(path: &Path) -> Result<LineStats> {
    let stats = load_repo_file_line_stats(path)?;
    Ok(sum_line_stats(stats.into_values()))
}

pub fn load_repo_line_stats_without_refresh(path: &Path) -> Result<LineStats> {
    load_repo_line_stats(path)
}

pub fn load_repo_file_line_stats_without_refresh(
    path: &Path,
) -> Result<BTreeMap<String, LineStats>> {
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

fn load_snapshot_internal(path: &Path) -> Result<RepoSnapshot> {
    let repo = open_repo(path)?;
    let seed = load_snapshot_seed(&repo, true, SnapshotLoadMode::RefreshWorkingCopy)?;
    let files = snapshot_files(seed.entries.values());
    let line_stats = sum_line_stats(seed.entries.values().map(|entry| entry.line_stats));
    let working_copy_commit_id =
        synthetic_working_copy_id(seed.head_commit_id.as_deref(), seed.entries.values());
    Ok(RepoSnapshot {
        root: seed.root,
        working_copy_commit_id,
        branch_name: seed.branch_name,
        branch_has_upstream: seed.branch_has_upstream,
        branch_ahead_count: seed.branch_ahead_count,
        branch_behind_count: seed.branch_behind_count,
        branches: seed.branches,
        files,
        line_stats,
        last_commit_subject: seed.last_commit_subject,
    })
}

fn load_workflow_snapshot_internal(
    path: &Path,
    mode: SnapshotLoadMode,
) -> Result<(RepoSnapshotFingerprint, WorkflowSnapshot)> {
    let repo = open_repo(path)?;
    let seed = load_snapshot_seed(&repo, false, mode)?;
    let fingerprint = snapshot_fingerprint(
        seed.root.clone(),
        seed.head_ref_name.clone(),
        seed.head_commit_id.clone(),
        seed.branch_has_upstream,
        seed.branch_ahead_count,
        seed.branch_behind_count,
        &seed.entries,
    );
    let working_copy_commit_id =
        synthetic_working_copy_id(seed.head_commit_id.as_deref(), seed.entries.values());
    let workflow = WorkflowSnapshot {
        root: seed.root,
        working_copy_commit_id,
        branch_name: seed.branch_name,
        branch_has_upstream: seed.branch_has_upstream,
        branch_ahead_count: seed.branch_ahead_count,
        branch_behind_count: seed.branch_behind_count,
        branches: seed.branches,
        files: snapshot_files(seed.entries.values()),
        last_commit_subject: seed.last_commit_subject,
    };
    Ok((fingerprint, workflow))
}

fn load_workflow_snapshot_if_changed_internal(
    path: &Path,
    previous_fingerprint: Option<&RepoSnapshotFingerprint>,
    mode: SnapshotLoadMode,
) -> Result<(RepoSnapshotFingerprint, Option<WorkflowSnapshot>)> {
    let (fingerprint, workflow) = load_workflow_snapshot_internal(path, mode)?;
    if previous_fingerprint == Some(&fingerprint) {
        return Ok((fingerprint, None));
    }
    Ok((fingerprint, Some(workflow)))
}

fn load_snapshot_fingerprint_internal(
    path: &Path,
    mode: SnapshotLoadMode,
) -> Result<RepoSnapshotFingerprint> {
    let repo = open_repo(path)?;
    let seed = load_snapshot_seed(&repo, false, mode)?;
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

fn load_snapshot_seed(
    repo: &GitRepo,
    include_line_stats: bool,
    mode: SnapshotLoadMode,
) -> Result<SnapshotSeed> {
    let head_ref_name = repo
        .repository()
        .head_name()
        .context("failed to resolve Git HEAD name")?
        .map(|name| name.to_string());
    let head_commit_id = repo.repository().head_id().ok().map(|id| id.to_string());
    let branch_name = branch_name_from_head_ref(head_ref_name.as_deref());
    let (branch_has_upstream, branch_ahead_count, branch_behind_count) =
        current_branch_tracking(repo.repository(), head_ref_name.as_deref())?;
    let branches = list_local_branches(repo.repository(), head_ref_name.as_deref())?;
    let entries = match mode {
        SnapshotLoadMode::ReadOnlyLight => {
            collect_workspace_diff_entries_light(repo.repository(), repo.root(), None)?
        }
        SnapshotLoadMode::RefreshWorkingCopy => collect_workspace_diff_entries_full(
            repo.repository(),
            repo.root(),
            None,
            include_line_stats,
        )?,
    };
    let last_commit_subject = last_commit_subject(repo.repository())?;

    Ok(SnapshotSeed {
        root: repo.root().to_path_buf(),
        head_ref_name,
        head_commit_id,
        branch_name,
        branch_has_upstream,
        branch_ahead_count,
        branch_behind_count,
        branches,
        entries,
        last_commit_subject,
    })
}

fn snapshot_files<'a>(
    entries: impl IntoIterator<Item = &'a WorkspaceDiffEntry>,
) -> Vec<ChangedFile> {
    entries
        .into_iter()
        .map(|entry| entry.file.clone())
        .collect()
}

fn load_patches_for_files_from_repo(
    repo: &GitRepo,
    files: &[ChangedFile],
) -> Result<BTreeMap<String, String>> {
    let requested_paths = files
        .iter()
        .map(|file| normalize_path(file.path.as_str()))
        .filter(|path| !path.is_empty())
        .collect::<BTreeSet<_>>();

    if requested_paths.is_empty() {
        return Ok(BTreeMap::new());
    }

    render_patches_for_paths(repo, &requested_paths)
}

fn render_patch_for_path(repo: &GitRepo, file_path: &str) -> Result<String> {
    let mut requested_paths = BTreeSet::new();
    requested_paths.insert(normalize_path(file_path));
    if requested_paths.first().is_some_and(|path| path.is_empty()) {
        return Ok(String::new());
    }

    let patch_map = render_patches_for_paths(repo, &requested_paths)?;
    Ok(patch_map.into_values().next().unwrap_or_default())
}

fn render_patches_for_paths(
    repo: &GitRepo,
    requested_paths: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>> {
    let resolved = resolve_workspace_files(repo.repository(), repo.root(), Some(requested_paths))?;
    let mut patch_map = requested_paths
        .iter()
        .cloned()
        .map(|path| (path, String::new()))
        .collect::<BTreeMap<_, _>>();

    for file in resolved {
        patch_map.insert(file.path.clone(), render_patch_for_resolved_file(&file)?);
    }

    Ok(patch_map)
}

fn load_repo_file_line_stats(path: &Path) -> Result<BTreeMap<String, LineStats>> {
    let repo = open_repo(path)?;
    let entries = collect_workspace_diff_entries_full(repo.repository(), repo.root(), None, true)?;
    Ok(entries
        .into_iter()
        .map(|(path, entry)| (path, entry.line_stats))
        .collect())
}

fn collect_workspace_diff_entries_full(
    repo: &gix::Repository,
    root: &Path,
    requested_paths: Option<&BTreeSet<String>>,
    include_line_stats: bool,
) -> Result<BTreeMap<String, WorkspaceDiffEntry>> {
    let resolved = resolve_workspace_files(repo, root, requested_paths)?;
    Ok(resolved
        .into_iter()
        .map(|file| workspace_diff_entry_from_resolved(file, include_line_stats))
        .collect())
}

fn collect_workspace_diff_entries_light(
    repo: &gix::Repository,
    root: &Path,
    requested_paths: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, WorkspaceDiffEntry>> {
    let candidates = collect_candidate_files(repo, root, requested_paths)?;
    if candidates.is_empty() {
        return Ok(BTreeMap::new());
    }

    let head_tree = repo
        .head_commit()
        .ok()
        .and_then(|commit| commit.tree().ok());
    let (mut filter_pipeline, filter_index_storage) = repo.filter_pipeline(None)?;
    let filter_index = index_state(&filter_index_storage);
    let mut entries = BTreeMap::new();

    for (path, candidate) in candidates {
        if candidate.staged_status.is_some() {
            if let Some(file) = resolve_workspace_file_full(
                repo,
                root,
                head_tree.as_ref(),
                &mut filter_pipeline,
                filter_index,
                path,
                candidate,
            )? {
                let (path, entry) = workspace_diff_entry_from_resolved(file, false);
                entries.insert(path, entry);
            }
            continue;
        }

        let rename_from = candidate.rename_from.clone();
        let old_entry = head_entry_summary(
            head_tree.as_ref(),
            rename_from.as_deref().unwrap_or(path.as_str()),
        )?;
        let new_entry = worktree_entry_summary(root, path.as_str())?;
        let index_has_entry = filter_index
            .entry_by_path(path.as_bytes().as_bstr())
            .is_some();
        let status = if candidate.staged_status == Some(FileStatus::Conflicted)
            || candidate.worktree_status == Some(FileStatus::Conflicted)
        {
            Some(FileStatus::Conflicted)
        } else {
            aggregate_file_status_from_summaries(
                old_entry.as_ref(),
                new_entry.as_ref(),
                index_has_entry,
                rename_from.as_deref(),
            )
        };
        let Some(status) = status else {
            continue;
        };
        let content_signature = workspace_entry_signature_light(
            status,
            old_entry.as_ref(),
            new_entry.as_ref(),
            index_has_entry,
            &candidate,
        );
        entries.insert(
            path.clone(),
            WorkspaceDiffEntry {
                file: ChangedFile {
                    path,
                    status,
                    staged: false,
                    untracked: matches!(status, FileStatus::Untracked),
                },
                line_stats: LineStats::default(),
                content_signature,
            },
        );
    }

    Ok(entries)
}

fn workspace_diff_entry_from_resolved(
    file: ResolvedWorkspaceFile,
    include_line_stats: bool,
) -> (String, WorkspaceDiffEntry) {
    let line_stats = if include_line_stats {
        line_stats_from_file_states(file.old_state.as_ref(), file.new_state.as_ref())
    } else {
        LineStats::default()
    };
    (
        file.path.clone(),
        WorkspaceDiffEntry {
            file: ChangedFile {
                path: file.path,
                status: file.status,
                staged: file.staged,
                untracked: file.untracked,
            },
            line_stats,
            content_signature: file.content_signature,
        },
    )
}

fn resolve_workspace_files(
    repo: &gix::Repository,
    root: &Path,
    requested_paths: Option<&BTreeSet<String>>,
) -> Result<Vec<ResolvedWorkspaceFile>> {
    let candidates = collect_candidate_files(repo, root, requested_paths)?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let head_tree = repo
        .head_commit()
        .ok()
        .and_then(|commit| commit.tree().ok());
    let (mut filter_pipeline, index_storage) = repo.filter_pipeline(None)?;
    let index = index_state(&index_storage);
    let mut resolved = Vec::with_capacity(candidates.len());

    for (path, candidate) in candidates {
        if let Some(file) = resolve_workspace_file_full(
            repo,
            root,
            head_tree.as_ref(),
            &mut filter_pipeline,
            index,
            path,
            candidate,
        )? {
            resolved.push(file);
        }
    }

    Ok(resolved)
}

fn resolve_workspace_file_full(
    repo: &gix::Repository,
    root: &Path,
    head_tree: Option<&gix::Tree<'_>>,
    filter_pipeline: &mut gix::filter::Pipeline<'_>,
    index: &gix::index::State,
    path: String,
    candidate: CandidateFile,
) -> Result<Option<ResolvedWorkspaceFile>> {
    let rename_from = candidate.rename_from.clone();
    let old_entry = head_entry_summary(head_tree, rename_from.as_deref().unwrap_or(path.as_str()))?;
    let old_state = head_file_state(
        repo,
        head_tree,
        rename_from.as_deref().unwrap_or(path.as_str()),
    )?;
    let new_state = worktree_file_state(repo, root, filter_pipeline, index, path.as_str())?;
    let new_entry = worktree_entry_summary(root, path.as_str())?;
    let index_has_entry = index.entry_by_path(path.as_bytes().as_bstr()).is_some();
    let status = if candidate.staged_status == Some(FileStatus::Conflicted)
        || candidate.worktree_status == Some(FileStatus::Conflicted)
    {
        Some(FileStatus::Conflicted)
    } else {
        aggregate_file_status(
            old_state.as_ref(),
            new_state.as_ref(),
            index_has_entry,
            rename_from.as_deref(),
        )
    };
    let Some(status) = status else {
        return Ok(None);
    };
    let content_signature = workspace_entry_signature_light(
        status,
        old_entry.as_ref(),
        new_entry.as_ref(),
        index_has_entry,
        &candidate,
    );
    Ok(Some(ResolvedWorkspaceFile {
        staged: candidate.staged_status.is_some(),
        untracked: matches!(status, FileStatus::Untracked),
        path,
        rename_from,
        status,
        content_signature,
        old_state,
        new_state,
    }))
}

fn collect_candidate_files(
    repo: &gix::Repository,
    root: &Path,
    requested_paths: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, CandidateFile>> {
    let nested_repo_roots = cached_nested_repo_roots_from_fs(root)?;
    let mut files = BTreeMap::<String, CandidateFile>::new();
    let iter = repo
        .status(gix::progress::Discard)?
        .index_worktree_submodules(None)
        .index_worktree_rewrites(Some(Default::default()))
        .tree_index_track_renames(gix::status::tree_index::TrackRenames::AsConfigured)
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(Vec::<gix::bstr::BString>::new())?;

    for item in iter {
        let item = item.context("failed to iterate Git status")?;
        let path = normalize_bstr_path(item.location());
        if path.is_empty() {
            continue;
        }
        if path_is_within_nested_repo(path.as_str(), &nested_repo_roots) {
            continue;
        }
        if requested_paths.is_some_and(|paths| !paths.contains(path.as_str())) {
            continue;
        }

        match item {
            gix::status::Item::TreeIndex(change) => {
                let (status, rename_from) = map_tree_index_status(&change);
                let candidate = files.entry(path).or_default();
                candidate.staged_status = merge_candidate_status(candidate.staged_status, status);
                merge_candidate_rename_from(&mut candidate.staged_rename_from, rename_from);
            }
            gix::status::Item::IndexWorktree(change) => {
                let Some((status, rename_from)) = map_index_worktree_status(&change) else {
                    continue;
                };
                let candidate = files.entry(path).or_default();
                candidate.worktree_status =
                    merge_candidate_status(candidate.worktree_status, status);
                merge_candidate_rename_from(&mut candidate.worktree_rename_from, rename_from);
            }
        }
    }

    resolve_candidate_rename_sources(&mut files);
    Ok(files)
}

fn merge_candidate_rename_from(slot: &mut Option<String>, rename_from: Option<String>) {
    if slot.is_none() {
        *slot = rename_from;
    }
}

fn resolve_candidate_rename_sources(files: &mut BTreeMap<String, CandidateFile>) {
    let snapshot = files.clone();
    for (path, candidate) in files.iter_mut() {
        candidate.rename_from = resolve_candidate_rename_source(&snapshot, path.as_str());
    }
}

fn resolve_candidate_rename_source(
    files: &BTreeMap<String, CandidateFile>,
    path: &str,
) -> Option<String> {
    let mut seen = BTreeSet::new();
    let mut current = path;
    let mut rename_from = None;

    loop {
        let Some(candidate) = files.get(current) else {
            break;
        };
        let Some(next) = candidate
            .staged_rename_from
            .as_deref()
            .or(candidate.worktree_rename_from.as_deref())
        else {
            break;
        };
        if !seen.insert(next.to_string()) {
            break;
        }
        rename_from = Some(next.to_string());
        current = next;
    }

    rename_from
}

fn current_branch_tracking(
    repo: &gix::Repository,
    head_ref_name: Option<&str>,
) -> Result<(bool, usize, usize)> {
    let Some(head_ref_name) = head_ref_name else {
        return Ok((false, 0, 0));
    };
    if !head_ref_name.starts_with("refs/heads/") {
        return Ok((false, 0, 0));
    }

    let head_ref_name = <&gix::refs::FullNameRef>::try_from(head_ref_name)
        .map_err(|err| anyhow!("failed to validate current Git branch reference name: {err}"))?;
    let Some(tracking_ref_name) = repo
        .branch_remote_tracking_ref_name(head_ref_name, gix::remote::Direction::Fetch)
        .transpose()
        .context("failed to resolve Git tracking branch")?
    else {
        return Ok((false, 0, 0));
    };

    let head_commit = match repo.head_id() {
        Ok(id) => id.detach(),
        Err(_) => return Ok((false, 0, 0)),
    };
    let mut tracking_ref = match repo.find_reference(tracking_ref_name.as_ref()) {
        Ok(reference) => reference,
        Err(_) => return Ok((false, 0, 0)),
    };
    let upstream_commit = tracking_ref
        .peel_to_id()
        .context("failed to peel tracking branch target")?
        .detach();

    let ahead = count_unique_commits(repo, head_commit, upstream_commit)?;
    let behind = count_unique_commits(repo, upstream_commit, head_commit)?;
    Ok((true, ahead, behind))
}

fn count_unique_commits(
    repo: &gix::Repository,
    tip: gix::ObjectId,
    hidden: gix::ObjectId,
) -> Result<usize> {
    if tip == hidden {
        return Ok(0);
    }

    let mut count = 0usize;
    for commit in repo.rev_walk([tip]).with_hidden([hidden]).all()? {
        commit.context("failed to walk Git revision graph")?;
        count += 1;
    }
    Ok(count)
}

fn list_local_branches(
    repo: &gix::Repository,
    current_head_ref_name: Option<&str>,
) -> Result<Vec<LocalBranch>> {
    let mut branches = Vec::new();
    let refs_platform = repo
        .references()
        .context("failed to access Git references")?;
    let refs = refs_platform
        .local_branches()
        .context("failed to iterate local Git branches")?
        .peeled()
        .context("failed to enable peeled Git branch iteration")?;

    for reference in refs {
        let mut reference =
            reference.map_err(|err| anyhow!("failed to read Git branch reference: {err}"))?;
        let full_name = reference.name().to_string();
        let name = short_branch_name(full_name.as_str()).unwrap_or(full_name.as_str());
        let tip_unix_time = match reference.peel_to_commit() {
            Ok(commit) => commit.time().ok().map(|time| time.seconds),
            Err(_) => None,
        };
        branches.push(LocalBranch {
            name: name.to_string(),
            is_current: Some(full_name.as_str()) == current_head_ref_name,
            tip_unix_time,
        });
    }

    branches.sort_by(|left, right| {
        right
            .is_current
            .cmp(&left.is_current)
            .then_with(|| right.tip_unix_time.cmp(&left.tip_unix_time))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(branches)
}

fn last_commit_subject(repo: &gix::Repository) -> Result<Option<String>> {
    let Some(commit) = repo.head_commit().ok() else {
        return Ok(None);
    };
    let message = commit.message_raw_sloppy();
    Ok(String::from_utf8_lossy(message.as_ref())
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::to_owned))
}

fn head_file_state(
    repo: &gix::Repository,
    head_tree: Option<&gix::Tree<'_>>,
    path: &str,
) -> Result<Option<FileState>> {
    let Some(head_tree) = head_tree else {
        return Ok(None);
    };
    let Some(entry) = head_tree
        .lookup_entry_by_path(Path::new(path))
        .with_context(|| format!("failed to look up HEAD tree entry for '{path}'"))?
    else {
        return Ok(None);
    };

    let kind = entry.mode().kind();
    let bytes = match file_kind_class(kind) {
        FileKindClass::Regular | FileKindClass::Link => {
            let mut blob = repo
                .find_blob(entry.object_id())
                .with_context(|| format!("failed to load blob for '{path}' from HEAD"))?;
            Some(blob.take_data())
        }
        FileKindClass::Unsupported => None,
    };

    Ok(Some(FileState {
        kind,
        id: entry.object_id(),
        bytes,
    }))
}

fn head_entry_summary(
    head_tree: Option<&gix::Tree<'_>>,
    path: &str,
) -> Result<Option<HeadEntrySummary>> {
    let Some(head_tree) = head_tree else {
        return Ok(None);
    };
    let Some(entry) = head_tree
        .lookup_entry_by_path(Path::new(path))
        .with_context(|| format!("failed to look up HEAD tree entry for '{path}'"))?
    else {
        return Ok(None);
    };

    Ok(Some(HeadEntrySummary {
        kind: entry.mode().kind(),
        id: entry.object_id(),
    }))
}

fn worktree_file_state(
    repo: &gix::Repository,
    root: &Path,
    filter_pipeline: &mut gix::filter::Pipeline<'_>,
    index: &gix::index::State,
    path: &str,
) -> Result<Option<FileState>> {
    let absolute_path = root.join(path);
    let metadata = match fs::symlink_metadata(absolute_path.as_path()) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to inspect worktree file '{}'",
                    absolute_path.display()
                )
            });
        }
    };

    if metadata.is_symlink() {
        let target = fs::read_link(absolute_path.as_path())
            .with_context(|| format!("failed to read symlink '{}'", absolute_path.display()))?;
        let bytes = target.to_string_lossy().into_owned().into_bytes();
        let id =
            gix::objs::compute_hash(repo.object_hash(), gix::objs::Kind::Blob, bytes.as_slice())
                .with_context(|| format!("failed to hash symlink target for '{path}'"))?;
        return Ok(Some(FileState {
            kind: gix::objs::tree::EntryKind::Link,
            id,
            bytes: Some(bytes),
        }));
    }

    if metadata.is_file() {
        let file = fs::File::open(absolute_path.as_path()).with_context(|| {
            format!("failed to open worktree file '{}'", absolute_path.display())
        })?;
        let bytes = read_filter_output(
            filter_pipeline
                .convert_to_git(file, Path::new(path), index)
                .with_context(|| format!("failed to convert worktree file '{path}' to Git form"))?,
        )
        .with_context(|| format!("failed to read converted worktree file '{path}'"))?;
        let kind = if gix::fs::is_executable(&metadata) {
            gix::objs::tree::EntryKind::BlobExecutable
        } else {
            gix::objs::tree::EntryKind::Blob
        };
        let id =
            gix::objs::compute_hash(repo.object_hash(), gix::objs::Kind::Blob, bytes.as_slice())
                .with_context(|| format!("failed to hash worktree file '{path}'"))?;
        return Ok(Some(FileState {
            kind,
            id,
            bytes: Some(bytes),
        }));
    }

    Ok(None)
}

fn worktree_entry_summary(root: &Path, path: &str) -> Result<Option<WorktreeEntrySummary>> {
    let absolute_path = root.join(path);
    let metadata = match fs::symlink_metadata(absolute_path.as_path()) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to inspect worktree file '{}'",
                    absolute_path.display()
                )
            });
        }
    };

    if metadata.is_symlink() {
        let target = fs::read_link(absolute_path.as_path())
            .with_context(|| format!("failed to read symlink '{}'", absolute_path.display()))?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hash_file_metadata(&metadata, &mut hasher);
        target.hash(&mut hasher);
        return Ok(Some(WorktreeEntrySummary {
            kind: gix::objs::tree::EntryKind::Link,
            signature: hasher.finish(),
        }));
    }

    if metadata.is_file() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hash_file_metadata(&metadata, &mut hasher);
        let kind = if gix::fs::is_executable(&metadata) {
            gix::objs::tree::EntryKind::BlobExecutable
        } else {
            gix::objs::tree::EntryKind::Blob
        };
        return Ok(Some(WorktreeEntrySummary {
            kind,
            signature: hasher.finish(),
        }));
    }

    Ok(None)
}

fn aggregate_file_status(
    old_state: Option<&FileState>,
    new_state: Option<&FileState>,
    index_has_entry: bool,
    rename_from: Option<&str>,
) -> Option<FileStatus> {
    if rename_from.is_some() && old_state.is_some() {
        return Some(FileStatus::Renamed);
    }

    match (old_state, new_state) {
        (None, None) => None,
        (None, Some(_)) => Some(if index_has_entry {
            FileStatus::Added
        } else {
            FileStatus::Untracked
        }),
        (Some(_), None) => Some(FileStatus::Deleted),
        (Some(old_state), Some(new_state)) => {
            if file_kind_class(old_state.kind) != file_kind_class(new_state.kind) {
                return Some(FileStatus::TypeChange);
            }
            if old_state.id == new_state.id && old_state.kind == new_state.kind {
                return None;
            }
            Some(FileStatus::Modified)
        }
    }
}

fn aggregate_file_status_from_summaries(
    old_entry: Option<&HeadEntrySummary>,
    new_entry: Option<&WorktreeEntrySummary>,
    index_has_entry: bool,
    rename_from: Option<&str>,
) -> Option<FileStatus> {
    if rename_from.is_some() && old_entry.is_some() {
        return Some(FileStatus::Renamed);
    }

    match (old_entry, new_entry) {
        (None, None) => None,
        (None, Some(_)) => Some(if index_has_entry {
            FileStatus::Added
        } else {
            FileStatus::Untracked
        }),
        (Some(_), None) => Some(FileStatus::Deleted),
        (Some(old_entry), Some(new_entry)) => {
            if file_kind_class(old_entry.kind) != file_kind_class(new_entry.kind) {
                return Some(FileStatus::TypeChange);
            }
            Some(FileStatus::Modified)
        }
    }
}

fn workspace_entry_signature_light(
    status: FileStatus,
    old_entry: Option<&HeadEntrySummary>,
    new_entry: Option<&WorktreeEntrySummary>,
    index_has_entry: bool,
    candidate: &CandidateFile,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    status.hash(&mut hasher);
    index_has_entry.hash(&mut hasher);
    candidate.staged_status.hash(&mut hasher);
    candidate.worktree_status.hash(&mut hasher);
    candidate.rename_from.hash(&mut hasher);
    hash_head_entry_summary(old_entry, &mut hasher);
    hash_worktree_entry_summary(new_entry, &mut hasher);
    hasher.finish()
}

fn hash_head_entry_summary(
    entry: Option<&HeadEntrySummary>,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    match entry {
        Some(entry) => {
            true.hash(hasher);
            file_state_kind_tag(entry.kind).hash(hasher);
            entry.id.hash(hasher);
        }
        None => false.hash(hasher),
    }
}

fn hash_worktree_entry_summary(
    entry: Option<&WorktreeEntrySummary>,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    match entry {
        Some(entry) => {
            true.hash(hasher);
            file_state_kind_tag(entry.kind).hash(hasher);
            entry.signature.hash(hasher);
        }
        None => false.hash(hasher),
    }
}

fn file_state_kind_tag(kind: gix::objs::tree::EntryKind) -> u8 {
    match kind {
        gix::objs::tree::EntryKind::Blob => 1,
        gix::objs::tree::EntryKind::BlobExecutable => 2,
        gix::objs::tree::EntryKind::Link => 3,
        gix::objs::tree::EntryKind::Tree => 4,
        gix::objs::tree::EntryKind::Commit => 5,
    }
}

fn hash_file_metadata(
    metadata: &fs::Metadata,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    metadata.len().hash(hasher);
    if let Ok(modified) = metadata.modified()
        && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
    {
        duration.as_secs().hash(hasher);
        duration.subsec_nanos().hash(hasher);
    }
    #[cfg(unix)]
    {
        metadata.dev().hash(hasher);
        metadata.ino().hash(hasher);
        metadata.mode().hash(hasher);
        metadata.mtime().hash(hasher);
        metadata.mtime_nsec().hash(hasher);
        metadata.ctime().hash(hasher);
        metadata.ctime_nsec().hash(hasher);
    }
}

fn line_stats_from_file_states(
    old_state: Option<&FileState>,
    new_state: Option<&FileState>,
) -> LineStats {
    let Some((old_bytes, new_bytes)) = diffable_bytes(old_state, new_state) else {
        return LineStats::default();
    };
    if is_binary(old_bytes) || is_binary(new_bytes) {
        return LineStats::default();
    }

    let input = InternedInput::new(old_bytes, new_bytes);
    let counter = gix::diff::blob::diff(
        gix::diff::blob::Algorithm::Histogram,
        &input,
        gix::diff::blob::sink::Counter::default(),
    );
    LineStats {
        added: counter.insertions as u64,
        removed: counter.removals as u64,
    }
}

fn diffable_bytes<'a>(
    old_state: Option<&'a FileState>,
    new_state: Option<&'a FileState>,
) -> Option<(&'a [u8], &'a [u8])> {
    if old_state.is_some_and(|state| file_kind_class(state.kind) != FileKindClass::Regular)
        || new_state.is_some_and(|state| file_kind_class(state.kind) != FileKindClass::Regular)
    {
        return None;
    }

    let empty: &'static [u8] = &[];
    let old_bytes = old_state
        .and_then(|state| state.bytes.as_deref())
        .unwrap_or(empty);
    let new_bytes = new_state
        .and_then(|state| state.bytes.as_deref())
        .unwrap_or(empty);
    Some((old_bytes, new_bytes))
}

fn patchable_bytes<'a>(
    old_state: Option<&'a FileState>,
    new_state: Option<&'a FileState>,
) -> Option<(&'a [u8], &'a [u8])> {
    if old_state.is_some_and(|state| file_kind_class(state.kind) == FileKindClass::Unsupported)
        || new_state.is_some_and(|state| file_kind_class(state.kind) == FileKindClass::Unsupported)
    {
        return None;
    }

    let empty: &'static [u8] = &[];
    let old_bytes = old_state
        .and_then(|state| state.bytes.as_deref())
        .unwrap_or(empty);
    let new_bytes = new_state
        .and_then(|state| state.bytes.as_deref())
        .unwrap_or(empty);
    Some((old_bytes, new_bytes))
}

fn render_patch_for_resolved_file(file: &ResolvedWorkspaceFile) -> Result<String> {
    let old_path = file.rename_from.as_deref().unwrap_or(file.path.as_str());
    let old_label = patch_side_label("a", old_path, file.old_state.is_some());
    let new_label = patch_side_label("b", &file.path, file.new_state.is_some());
    let mut patch = format!(
        "diff --git a/{old_path} b/{new_path}\n",
        new_path = file.path
    );

    if let Some(rename_from) = file.rename_from.as_deref()
        && rename_from != file.path.as_str()
    {
        patch.push_str(&format!(
            "rename from {rename_from}\nrename to {}\n",
            file.path
        ));
    }

    match (file.old_state.as_ref(), file.new_state.as_ref()) {
        (None, Some(new_state)) => {
            patch.push_str(&format!(
                "new file mode {}\n",
                file_mode_string(new_state.kind)
            ));
        }
        (Some(old_state), None) => {
            patch.push_str(&format!(
                "deleted file mode {}\n",
                file_mode_string(old_state.kind)
            ));
        }
        (Some(old_state), Some(new_state)) if old_state.kind != new_state.kind => {
            patch.push_str(&format!(
                "old mode {}\nnew mode {}\n",
                file_mode_string(old_state.kind),
                file_mode_string(new_state.kind),
            ));
        }
        _ => {}
    }

    if let Some((old_bytes, new_bytes)) =
        patchable_bytes(file.old_state.as_ref(), file.new_state.as_ref())
    {
        if is_binary(old_bytes) || is_binary(new_bytes) {
            patch.push_str(&format!(
                "Binary files {old_label} and {new_label} differ\n"
            ));
            return Ok(patch);
        }

        patch.push_str(&format!("--- {old_label}\n+++ {new_label}\n"));
        let input = InternedInput::new(old_bytes, new_bytes);
        let unified = gix::diff::blob::diff(
            gix::diff::blob::Algorithm::Histogram,
            &input,
            gix::diff::blob::UnifiedDiff::new(
                &input,
                gix::diff::blob::unified_diff::ConsumeBinaryHunk::new(String::new(), "\n"),
                gix::diff::blob::unified_diff::ContextSize::default(),
            ),
        )?;
        patch.push_str(&unified);
        return Ok(patch);
    }

    patch.push_str(&format!(
        "Binary files {old_label} and {new_label} differ\n"
    ));
    Ok(patch)
}

fn patch_side_label(prefix: &str, path: &str, present: bool) -> String {
    if present {
        format!("{prefix}/{path}")
    } else {
        "/dev/null".to_string()
    }
}

fn file_mode_string(kind: gix::objs::tree::EntryKind) -> &'static str {
    match kind {
        gix::objs::tree::EntryKind::Blob => "100644",
        gix::objs::tree::EntryKind::BlobExecutable => "100755",
        gix::objs::tree::EntryKind::Link => "120000",
        gix::objs::tree::EntryKind::Tree => "040000",
        gix::objs::tree::EntryKind::Commit => "160000",
    }
}

fn load_visible_repo_paths(repo: &gix::Repository, root: &Path) -> Result<BTreeSet<String>> {
    let index = repo.index_or_empty()?;
    let mut paths = BTreeSet::new();
    for (path, ()) in index.entries_with_paths_by_filter_map(|_path, _| Some(())) {
        let path = normalize_bstr_path(path);
        if !path.is_empty() {
            paths.insert(path);
        }
    }
    paths.extend(collect_untracked_repo_paths(repo, root)?);
    Ok(paths)
}

fn collect_untracked_repo_paths(repo: &gix::Repository, root: &Path) -> Result<BTreeSet<String>> {
    let nested_repo_roots = cached_nested_repo_roots_from_fs(root)?;
    let mut paths = BTreeSet::new();
    let iter = repo
        .status(gix::progress::Discard)?
        .index_worktree_submodules(None)
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_index_worktree_iter(Vec::<gix::bstr::BString>::new())?;

    for item in iter {
        let item = item.context("failed to iterate Git worktree status")?;
        let Some(summary) = item.summary() else {
            continue;
        };
        if summary != gix::status::index_worktree::iter::Summary::Added
            && summary != gix::status::index_worktree::iter::Summary::IntentToAdd
        {
            continue;
        }

        let path = normalize_bstr_path(item.rela_path());
        if path.is_empty() {
            continue;
        }
        if path_is_within_nested_repo(path.as_str(), &nested_repo_roots) {
            continue;
        }
        paths.insert(path);
    }

    Ok(paths)
}

fn walk_repo_tree(
    root: &Path,
    current: &Path,
    visible_paths: &BTreeSet<String>,
    entries: &mut Vec<RepoTreeEntry>,
) -> Result<()> {
    if entries.len() >= MAX_REPO_TREE_ENTRIES {
        return Ok(());
    }

    let mut children = read_dir_sorted(current)?;
    for child in children.drain(..) {
        if entries.len() >= MAX_REPO_TREE_ENTRIES {
            break;
        }

        let name = child.file_name();
        let name = name.to_string_lossy();
        if name == ".git" {
            continue;
        }

        let Ok(file_type) = child.file_type() else {
            continue;
        };

        let child_path = child.path();
        let Ok(relative) = child_path.strip_prefix(root) else {
            continue;
        };
        let relative_path = normalize_path(relative.to_string_lossy().as_ref());
        if relative_path.is_empty() {
            continue;
        }

        if file_type.is_dir() {
            let ignored = !path_is_visible_or_ancestor(relative_path.as_str(), visible_paths);
            entries.push(RepoTreeEntry {
                path: relative_path,
                kind: RepoTreeEntryKind::Directory,
                ignored,
            });
            if ignored {
                continue;
            }
            walk_repo_tree(root, child_path.as_path(), visible_paths, entries)?;
            continue;
        }

        if file_type.is_file() || file_type.is_symlink() {
            let ignored = !visible_paths.contains(relative_path.as_str());
            entries.push(RepoTreeEntry {
                path: relative_path,
                kind: RepoTreeEntryKind::File,
                ignored,
            });
        }
    }

    Ok(())
}

fn read_dir_sorted(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)
        .with_context(|| format!("failed to read directory {}", path.display()))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });
    Ok(entries)
}

fn path_is_visible_or_ancestor(path: &str, visible_paths: &BTreeSet<String>) -> bool {
    if visible_paths.contains(path) {
        return true;
    }
    let prefix = format!("{path}/");
    visible_paths
        .iter()
        .any(|visible| visible.starts_with(&prefix))
}

fn cached_nested_repo_roots_from_fs(root: &Path) -> Result<BTreeSet<String>> {
    let cache_key = root.to_path_buf();
    if let Some(cached) = nested_repo_roots_cache_guard().get(&cache_key).cloned() {
        return Ok(cached);
    }

    let roots = nested_repo_roots_from_fs(root)?;
    nested_repo_roots_cache_guard().insert(cache_key, roots.clone());
    Ok(roots)
}

fn nested_repo_roots_from_fs(root: &Path) -> Result<BTreeSet<String>> {
    let mut nested_roots = BTreeSet::new();
    collect_nested_repo_roots(root, root, &mut nested_roots)?;
    Ok(nested_roots)
}

fn collect_nested_repo_roots(
    root: &Path,
    current: &Path,
    nested_roots: &mut BTreeSet<String>,
) -> Result<()> {
    for child in read_dir_sorted(current)? {
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let name = child.file_name();
        let name = name.to_string_lossy();
        if name == ".git" {
            continue;
        }

        let child_path = child.path();
        if directory_is_repo_root(child_path.as_path()) {
            if let Ok(relative) = child_path.strip_prefix(root) {
                let relative_path = normalize_path(relative.to_string_lossy().as_ref());
                if !relative_path.is_empty() {
                    nested_roots.insert(relative_path);
                }
            }
            continue;
        }

        collect_nested_repo_roots(root, child_path.as_path(), nested_roots)?;
    }

    Ok(())
}

fn directory_is_repo_root(path: &Path) -> bool {
    let git_marker = path.join(".git");
    git_marker.is_dir() || git_marker.is_file()
}

fn path_is_within_nested_repo(path: &str, nested_repo_roots: &BTreeSet<String>) -> bool {
    if path.is_empty() {
        return false;
    }

    nested_repo_roots.iter().any(|nested_root| {
        if path == nested_root {
            return true;
        }
        path.strip_prefix(nested_root.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

fn snapshot_fingerprint(
    root: PathBuf,
    head_ref_name: Option<String>,
    head_commit_id: Option<String>,
    branch_has_upstream: bool,
    branch_ahead_count: usize,
    branch_behind_count: usize,
    entries: &BTreeMap<String, WorkspaceDiffEntry>,
) -> RepoSnapshotFingerprint {
    RepoSnapshotFingerprint {
        root,
        head_ref_name,
        head_commit_id,
        branch_has_upstream,
        branch_ahead_count,
        branch_behind_count,
        changed_file_count: entries.len(),
        changed_file_signature: hash_changed_entries(entries.values()),
    }
}

fn hash_changed_entries<'a>(entries: impl IntoIterator<Item = &'a WorkspaceDiffEntry>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for entry in entries {
        entry.file.path.hash(&mut hasher);
        entry.file.status.hash(&mut hasher);
        entry.file.staged.hash(&mut hasher);
        entry.file.untracked.hash(&mut hasher);
        entry.content_signature.hash(&mut hasher);
    }
    hasher.finish()
}

fn synthetic_working_copy_id<'a>(
    head_commit_id: Option<&str>,
    entries: impl IntoIterator<Item = &'a WorkspaceDiffEntry>,
) -> String {
    let entries = entries.into_iter().collect::<Vec<_>>();
    format!(
        "git:{}:{}:{}",
        head_commit_id.unwrap_or("unborn"),
        entries.len(),
        hash_changed_entries(entries.into_iter())
    )
}

fn branch_name_from_head_ref(head_ref_name: Option<&str>) -> String {
    match head_ref_name.and_then(short_branch_name) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => "detached".to_string(),
    }
}

fn short_branch_name(full_name: &str) -> Option<&str> {
    full_name.strip_prefix("refs/heads/")
}

fn file_kind_class(kind: gix::objs::tree::EntryKind) -> FileKindClass {
    match kind {
        gix::objs::tree::EntryKind::Blob | gix::objs::tree::EntryKind::BlobExecutable => {
            FileKindClass::Regular
        }
        gix::objs::tree::EntryKind::Link => FileKindClass::Link,
        gix::objs::tree::EntryKind::Tree | gix::objs::tree::EntryKind::Commit => {
            FileKindClass::Unsupported
        }
    }
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8 * 1024).any(|byte| *byte == 0)
}

fn read_filter_output<R>(outcome: ToGitOutcome<'_, R>) -> Result<Vec<u8>>
where
    R: std::io::Read,
{
    let mut bytes = Vec::new();
    match outcome {
        ToGitOutcome::Unchanged(mut reader) => {
            reader.read_to_end(&mut bytes)?;
        }
        ToGitOutcome::Process(mut reader) => {
            reader.read_to_end(&mut bytes)?;
        }
        ToGitOutcome::Buffer(buffer) => bytes.extend_from_slice(buffer.as_ref()),
    }
    Ok(bytes)
}

fn map_tree_index_status(change: &gix::diff::index::Change) -> (FileStatus, Option<String>) {
    match change {
        gix::diff::index::Change::Addition { .. } => (FileStatus::Added, None),
        gix::diff::index::Change::Deletion { .. } => (FileStatus::Deleted, None),
        gix::diff::index::Change::Modification {
            previous_entry_mode,
            entry_mode,
            ..
        } => {
            let previous_kind = previous_entry_mode
                .to_tree_entry_mode()
                .map(|mode| mode.kind())
                .unwrap_or(gix::objs::tree::EntryKind::Blob);
            let current_kind = entry_mode
                .to_tree_entry_mode()
                .map(|mode| mode.kind())
                .unwrap_or(gix::objs::tree::EntryKind::Blob);
            if file_kind_class(previous_kind) != file_kind_class(current_kind) {
                (FileStatus::TypeChange, None)
            } else {
                (FileStatus::Modified, None)
            }
        }
        gix::diff::index::Change::Rewrite {
            source_location, ..
        } => (
            FileStatus::Renamed,
            normalized_optional_path(normalize_bstr_path(source_location.as_ref())),
        ),
    }
}

fn map_index_worktree_status(
    change: &gix::status::index_worktree::Item,
) -> Option<(FileStatus, Option<String>)> {
    use gix::status::index_worktree::iter::Summary;

    if let gix::status::index_worktree::Item::Rewrite { source, .. } = change {
        return Some((
            FileStatus::Renamed,
            normalized_optional_path(normalize_bstr_path(source.rela_path())),
        ));
    }

    Some((
        match change.summary()? {
            Summary::Added | Summary::IntentToAdd => FileStatus::Untracked,
            Summary::Removed => FileStatus::Deleted,
            Summary::Modified => FileStatus::Modified,
            Summary::Renamed | Summary::Copied => FileStatus::Renamed,
            Summary::TypeChange => FileStatus::TypeChange,
            Summary::Conflict => FileStatus::Conflicted,
        },
        None,
    ))
}

fn merge_candidate_status(
    existing: Option<FileStatus>,
    incoming: FileStatus,
) -> Option<FileStatus> {
    let priority = |status: FileStatus| -> u8 {
        match status {
            FileStatus::Conflicted => 7,
            FileStatus::Deleted => 6,
            FileStatus::Renamed => 5,
            FileStatus::TypeChange => 4,
            FileStatus::Added => 3,
            FileStatus::Untracked => 2,
            FileStatus::Modified => 1,
            FileStatus::Unknown => 0,
        }
    };

    match existing {
        Some(current) if priority(current) >= priority(incoming) => Some(current),
        _ => Some(incoming),
    }
}

fn index_state(index: &gix::worktree::IndexPersistedOrInMemory) -> &gix::index::State {
    match index {
        gix::worktree::IndexPersistedOrInMemory::Persisted(index) => index,
        gix::worktree::IndexPersistedOrInMemory::InMemory(index) => index,
    }
}

fn repo_root_from_repository(repo: &gix::Repository) -> Result<PathBuf> {
    let root = repo.workdir().unwrap_or_else(|| repo.git_dir());
    canonicalize_existing_path(root)
}

fn normalize_bstr_path(path: &BStr) -> String {
    normalize_path(String::from_utf8_lossy(path.as_ref()).as_ref())
}

fn normalized_optional_path(path: String) -> Option<String> {
    (!path.is_empty()).then_some(path)
}

fn normalize_path(path: &str) -> String {
    path.trim().trim_end_matches('/').replace('\\', "/")
}

fn sum_line_stats<I>(stats: I) -> LineStats
where
    I: IntoIterator<Item = LineStats>,
{
    let mut total = LineStats::default();
    for stat in stats {
        total.added = total.added.saturating_add(stat.added);
        total.removed = total.removed.saturating_add(stat.removed);
    }
    total
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize existing path {}", path.display()))
}

fn nested_repo_roots_cache_guard()
-> std::sync::MutexGuard<'static, HashMap<PathBuf, BTreeSet<String>>> {
    match NESTED_REPO_ROOTS_CACHE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
