use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use git2::{DiffOptions, ObjectType, Oid, Patch, Repository, Tree};

use crate::git::{ChangedFile, FileStatus, LineStats};
use crate::git2_helpers::open_git2_repo;
use crate::worktree::repo_relative_path_is_within_managed_worktrees;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareSource {
    WorkspaceTarget { target_id: String, root: PathBuf },
    Branch { name: String },
}

#[derive(Debug, Clone)]
pub struct CompareSnapshot {
    pub files: Vec<ChangedFile>,
    pub file_line_stats: BTreeMap<String, LineStats>,
    pub overall_line_stats: LineStats,
    pub patches_by_path: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ComparePathKind {
    Regular,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparePathState {
    bytes: Option<Vec<u8>>,
    mode: Option<u32>,
    kind: ComparePathKind,
}

impl ComparePathState {
    fn missing() -> Self {
        Self {
            bytes: None,
            mode: None,
            kind: ComparePathKind::Other,
        }
    }

    fn is_present(&self) -> bool {
        self.mode.is_some()
    }

    fn supports_text_patch(&self) -> bool {
        !self.is_present() || self.kind == ComparePathKind::Regular
    }

    fn patch_bytes(&self) -> &[u8] {
        self.bytes.as_deref().unwrap_or(&[])
    }
}

pub fn compare_branch_source_id(branch_name: &str) -> String {
    format!("branch:{branch_name}")
}

pub fn compare_workspace_target_source_id(target_id: &str) -> String {
    format!("workspace:{target_id}")
}

pub fn resolve_default_base_branch_name(repo_root: &Path) -> Result<Option<String>> {
    let repo = gix::discover(repo_root).with_context(|| {
        format!(
            "failed to discover Git repository from {}",
            repo_root.display()
        )
    })?;
    if let Some(branch_name) = remote_default_branch_name(&repo)? {
        return Ok(Some(branch_name));
    }
    for candidate in ["main", "master"] {
        if local_branch_exists(repo_root, candidate)? {
            return Ok(Some(candidate.to_string()));
        }
    }
    Ok(None)
}

pub fn load_compare_snapshot(
    primary_repo_root: &Path,
    left: &CompareSource,
    right: &CompareSource,
) -> Result<CompareSnapshot> {
    let common_repo = open_repository(primary_repo_root)?;
    let left = resolve_compare_source(&common_repo, left)?;
    let right = resolve_compare_source(&common_repo, right)?;

    let mut candidate_paths =
        collect_tree_pair_diff_paths(&common_repo, left.head_tree_oid, right.head_tree_oid)?;
    if let Some(paths) = collect_workspace_diff_paths(left.workspace_root.as_deref())? {
        candidate_paths.extend(paths);
    }
    if let Some(paths) = collect_workspace_diff_paths(right.workspace_root.as_deref())? {
        candidate_paths.extend(paths);
    }

    let mut files = Vec::new();
    let mut file_line_stats = BTreeMap::new();
    let mut patches_by_path = BTreeMap::new();
    let mut overall_line_stats = LineStats::default();

    for path in candidate_paths {
        let old_state = load_compare_source_state(&common_repo, &left, path.as_str())?;
        let new_state = load_compare_source_state(&common_repo, &right, path.as_str())?;
        if old_state == new_state {
            continue;
        }

        let (patch, line_stats) =
            render_patch_and_line_stats(path.as_str(), &old_state, &new_state)?;
        let status = compare_file_status(&old_state, &new_state);
        files.push(ChangedFile {
            path: path.clone(),
            status,
            staged: false,
            untracked: false,
        });
        file_line_stats.insert(path.clone(), line_stats);
        patches_by_path.insert(path, patch);
        overall_line_stats.added = overall_line_stats.added.saturating_add(line_stats.added);
        overall_line_stats.removed = overall_line_stats
            .removed
            .saturating_add(line_stats.removed);
    }

    Ok(CompareSnapshot {
        files,
        file_line_stats,
        overall_line_stats,
        patches_by_path,
    })
}

#[derive(Debug, Clone)]
struct ResolvedCompareSource {
    workspace_root: Option<PathBuf>,
    head_tree_oid: Option<Oid>,
}

fn resolve_compare_source(
    repo: &Repository,
    source: &CompareSource,
) -> Result<ResolvedCompareSource> {
    match source {
        CompareSource::WorkspaceTarget { root, .. } => {
            let workspace_repo = open_repository(root)?;
            let head_tree_oid = head_tree_oid(&workspace_repo)?;
            Ok(ResolvedCompareSource {
                workspace_root: Some(root.clone()),
                head_tree_oid,
            })
        }
        CompareSource::Branch { name } => Ok(ResolvedCompareSource {
            workspace_root: None,
            head_tree_oid: Some(branch_tree_oid(repo, name.as_str())?),
        }),
    }
}

fn collect_tree_pair_diff_paths(
    repo: &Repository,
    left_tree_oid: Option<Oid>,
    right_tree_oid: Option<Oid>,
) -> Result<BTreeSet<String>> {
    let left_tree = peel_tree(repo, left_tree_oid)?;
    let right_tree = peel_tree(repo, right_tree_oid)?;
    let mut options = diff_options();
    let diff = repo
        .diff_tree_to_tree(left_tree.as_ref(), right_tree.as_ref(), Some(&mut options))
        .context("failed to diff compare source trees")?;
    Ok(diff_delta_paths(&diff))
}

fn collect_workspace_diff_paths(workspace_root: Option<&Path>) -> Result<Option<BTreeSet<String>>> {
    let Some(workspace_root) = workspace_root else {
        return Ok(None);
    };

    let repo = open_repository(workspace_root)?;
    let head_tree = peel_tree(&repo, head_tree_oid(&repo)?)?;
    let mut options = diff_options();
    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut options))
        .with_context(|| {
            format!(
                "failed to diff workspace changes for {}",
                workspace_root.display()
            )
        })?;
    Ok(Some(diff_delta_paths(&diff)))
}

fn diff_delta_paths(diff: &git2::Diff<'_>) -> BTreeSet<String> {
    diff.deltas()
        .filter_map(|delta| {
            delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .and_then(path_to_repo_string)
        })
        .filter(|path| !repo_relative_path_is_within_managed_worktrees(path.as_str()))
        .collect()
}

fn load_compare_source_state(
    repo: &Repository,
    source: &ResolvedCompareSource,
    path: &str,
) -> Result<ComparePathState> {
    if let Some(workspace_root) = source.workspace_root.as_ref() {
        let absolute_path = workspace_root.join(path);
        return read_workspace_path_state(absolute_path.as_path());
    }

    let Some(tree_oid) = source.head_tree_oid else {
        return Ok(ComparePathState::missing());
    };
    let tree = repo
        .find_tree(tree_oid)
        .with_context(|| format!("failed to open compare tree {tree_oid}"))?;
    tree_path_state(repo, &tree, path)
}

fn render_patch_and_line_stats(
    path: &str,
    old_state: &ComparePathState,
    new_state: &ComparePathState,
) -> Result<(String, LineStats)> {
    let mode_headers = render_mode_headers(old_state, new_state);
    if old_state.patch_bytes() == new_state.patch_bytes() {
        return Ok((
            render_metadata_only_patch(path, old_state, new_state, mode_headers.as_str()),
            LineStats::default(),
        ));
    }

    if !old_state.supports_text_patch() || !new_state.supports_text_patch() {
        return Ok((
            render_binary_patch(path, old_state, new_state, mode_headers.as_str()),
            LineStats::default(),
        ));
    }

    let mut options = diff_options();
    let old_bytes = old_state.patch_bytes();
    let new_bytes = new_state.patch_bytes();
    if is_binary(old_bytes) || is_binary(new_bytes) {
        return Ok((
            render_binary_patch(path, old_state, new_state, mode_headers.as_str()),
            LineStats::default(),
        ));
    }

    let mut patch = Patch::from_buffers(
        old_bytes,
        Some(Path::new(path)),
        new_bytes,
        Some(Path::new(path)),
        Some(&mut options),
    )
    .with_context(|| format!("failed to render patch for {path}"))?;
    let patch_text = patch
        .to_buf()
        .with_context(|| format!("failed to render patch buffer for {path}"))?
        .as_str()
        .ok_or_else(|| anyhow!("compare patch for '{path}' is not valid UTF-8"))?
        .to_string();
    let (_, additions, deletions) = patch
        .line_stats()
        .with_context(|| format!("failed to compute patch line stats for {path}"))?;
    Ok((
        prepend_mode_headers(path, patch_text, mode_headers.as_str()),
        LineStats {
            added: additions as u64,
            removed: deletions as u64,
        },
    ))
}

fn render_metadata_only_patch(
    path: &str,
    _old_state: &ComparePathState,
    _new_state: &ComparePathState,
    mode_headers: &str,
) -> String {
    let mut patch = format!("diff --git a/{path} b/{path}\n");
    patch.push_str(mode_headers);
    patch
}

fn render_binary_patch(
    path: &str,
    old_state: &ComparePathState,
    new_state: &ComparePathState,
    mode_headers: &str,
) -> String {
    let old_label = patch_side_label("a", path, old_state.is_present());
    let new_label = patch_side_label("b", path, new_state.is_present());
    let mut patch = render_metadata_only_patch(path, old_state, new_state, mode_headers);
    patch.push_str(&format!("--- {old_label}\n+++ {new_label}\n"));
    patch.push_str(&format!(
        "Binary files {old_label} and {new_label} differ\n"
    ));
    patch
}

fn patch_side_label(prefix: &str, path: &str, present: bool) -> String {
    if present {
        format!("{prefix}/{path}")
    } else {
        "/dev/null".to_string()
    }
}

fn compare_file_status(old_state: &ComparePathState, new_state: &ComparePathState) -> FileStatus {
    match (old_state.is_present(), new_state.is_present()) {
        (false, true) => FileStatus::Added,
        (true, false) => FileStatus::Deleted,
        _ => FileStatus::Modified,
    }
}

fn open_repository(path: &Path) -> Result<Repository> {
    open_git2_repo(path)
}

fn head_tree_oid(repo: &Repository) -> Result<Option<Oid>> {
    let head = match repo.head() {
        Ok(head) => head,
        Err(err)
            if err.code() == git2::ErrorCode::UnbornBranch
                || err.code() == git2::ErrorCode::NotFound =>
        {
            return Ok(None);
        }
        Err(err) => return Err(err).context("failed to resolve compare source HEAD"),
    };
    let commit = head
        .peel_to_commit()
        .context("failed to resolve compare source HEAD commit")?;
    Ok(Some(commit.tree_id()))
}

fn branch_tree_oid(repo: &Repository, branch_name: &str) -> Result<Oid> {
    let reference = repo
        .find_branch(branch_name, git2::BranchType::Local)
        .with_context(|| format!("branch '{branch_name}' does not exist"))?
        .into_reference();
    let commit = reference
        .peel_to_commit()
        .with_context(|| format!("failed to resolve branch '{branch_name}' commit"))?;
    Ok(commit.tree_id())
}

fn peel_tree(repo: &Repository, tree_oid: Option<Oid>) -> Result<Option<Tree<'_>>> {
    tree_oid
        .map(|tree_oid| {
            repo.find_tree(tree_oid)
                .with_context(|| format!("failed to resolve tree {tree_oid}"))
        })
        .transpose()
}

fn tree_path_state(repo: &Repository, tree: &Tree<'_>, path: &str) -> Result<ComparePathState> {
    let entry = match tree.get_path(Path::new(path)) {
        Ok(entry) => entry,
        Err(err)
            if err.code() == git2::ErrorCode::NotFound
                || err.code() == git2::ErrorCode::InvalidSpec =>
        {
            return Ok(ComparePathState::missing());
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to resolve tree path '{path}'"));
        }
    };
    let mode = Some(entry.filemode_raw() as u32);
    let object = entry
        .to_object(repo)
        .with_context(|| format!("failed to open tree object for '{path}'"))?;
    if object.kind() != Some(ObjectType::Blob) {
        return Ok(ComparePathState {
            bytes: None,
            mode,
            kind: ComparePathKind::Other,
        });
    }
    let blob = object
        .peel_to_blob()
        .with_context(|| format!("failed to open blob for '{path}'"))?;
    let kind = if mode == Some(0o120000) {
        ComparePathKind::Symlink
    } else {
        ComparePathKind::Regular
    };
    Ok(ComparePathState {
        bytes: Some(blob.content().to_vec()),
        mode,
        kind,
    })
}

fn read_workspace_path_state(path: &Path) -> Result<ComparePathState> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ComparePathState::missing());
        }
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to inspect workspace path {}", path.display()));
        }
    };

    if metadata.is_symlink() {
        let target = fs::read_link(path)
            .with_context(|| format!("failed to read symlink {}", path.display()))?;
        return Ok(ComparePathState {
            bytes: Some(target.to_string_lossy().into_owned().into_bytes()),
            mode: Some(0o120000),
            kind: ComparePathKind::Symlink,
        });
    }

    if metadata.is_file() {
        let mode = if gix::fs::is_executable(&metadata) {
            0o100755
        } else {
            0o100644
        };
        return Ok(ComparePathState {
            bytes: Some(
                fs::read(path)
                    .with_context(|| format!("failed to read workspace file {}", path.display()))?,
            ),
            mode: Some(mode),
            kind: ComparePathKind::Regular,
        });
    }

    Ok(ComparePathState {
        bytes: None,
        mode: Some(0o040000),
        kind: ComparePathKind::Other,
    })
}

fn diff_options() -> DiffOptions {
    let mut options = DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_unmodified(false)
        .ignore_submodules(true);
    options
}

fn remote_default_branch_name(repo: &gix::Repository) -> Result<Option<String>> {
    let remote_name = repo
        .find_default_remote(gix::remote::Direction::Fetch)
        .and_then(Result::ok)
        .and_then(|remote| {
            remote.name().and_then(|name| match name {
                gix::remote::Name::Symbol(name) => Some(name.to_string()),
                gix::remote::Name::Url(_) => None,
            })
        })
        .or_else(|| {
            repo.remote_names()
                .into_iter()
                .next()
                .map(|name| name.as_ref().to_string())
        });
    let Some(remote_name) = remote_name else {
        return Ok(None);
    };

    let default_remote_head_ref = format!("refs/remotes/{remote_name}/HEAD");
    let Ok(reference) = repo.find_reference(default_remote_head_ref.as_str()) else {
        return Ok(None);
    };
    Ok(reference
        .target()
        .try_name()
        .map(|name| name.to_string())
        .and_then(|name| {
            name.strip_prefix(format!("refs/remotes/{remote_name}/").as_str())
                .map(str::to_owned)
        }))
}

fn local_branch_exists(repo_root: &Path, branch_name: &str) -> Result<bool> {
    let repo = open_repository(repo_root)?;
    Ok(repo
        .find_branch(branch_name, git2::BranchType::Local)
        .is_ok())
}

fn path_to_repo_string(path: &Path) -> Option<String> {
    Some(path.to_string_lossy().replace('\\', "/"))
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8 * 1024).any(|byte| *byte == 0)
}

fn render_mode_headers(old_state: &ComparePathState, new_state: &ComparePathState) -> String {
    match (old_state.mode, new_state.mode) {
        (None, Some(new_mode)) => format!("new file mode {}\n", format_mode(new_mode)),
        (Some(old_mode), None) => format!("deleted file mode {}\n", format_mode(old_mode)),
        (Some(old_mode), Some(new_mode)) if old_mode != new_mode => format!(
            "old mode {}\nnew mode {}\n",
            format_mode(old_mode),
            format_mode(new_mode),
        ),
        _ => String::new(),
    }
}

fn prepend_mode_headers(path: &str, patch_text: String, mode_headers: &str) -> String {
    if mode_headers.is_empty() {
        return patch_text;
    }

    if let Some(first_newline) = patch_text.find('\n') {
        let (first_line, remainder) = patch_text.split_at(first_newline + 1);
        return format!("{first_line}{mode_headers}{remainder}");
    }

    format!("diff --git a/{path} b/{path}\n{mode_headers}{patch_text}")
}

fn format_mode(mode: u32) -> String {
    format!("{mode:06o}")
}
