fn load_visible_repo_paths(repo: &gix::Repository, root: &Path) -> Result<BTreeSet<String>> {
    let index = repo.index_or_empty()?;
    let mut paths = BTreeSet::new();
    for (path, ()) in index.entries_with_paths_by_filter_map(|_path, _| Some(())) {
        let path = normalize_bstr_path(path);
        if !path.is_empty()
            && !repo_relative_path_is_within_managed_worktrees(path.as_str())
            && visible_repo_file_exists(root, path.as_str())
        {
            paths.insert(path);
        }
    }
    paths.extend(
        collect_untracked_repo_paths(repo, root)?
            .into_iter()
            .filter(|path| visible_repo_file_exists(root, path.as_str())),
    );
    Ok(paths)
}

fn visible_repo_file_exists(root: &Path, repo_relative_path: &str) -> bool {
    let path = root.join(repo_relative_path);
    match fs::symlink_metadata(path.as_path()) {
        Ok(metadata) => metadata.is_file() || metadata.file_type().is_symlink(),
        Err(_) => false,
    }
}

fn collect_untracked_repo_paths(repo: &gix::Repository, root: &Path) -> Result<BTreeSet<String>> {
    let mut nested_repo_filter = NestedRepoFilter::load(root);
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
        if repo_relative_path_is_within_managed_worktrees(path.as_str()) {
            continue;
        }
        if nested_repo_filter.contains_path(path.as_str()) {
            continue;
        }
        paths.insert(path);
    }

    nested_repo_filter.persist();
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
        if repo_relative_path_is_within_managed_worktrees(relative_path.as_str()) {
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
        entry.file.unstaged.hash(&mut hasher);
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
-> std::sync::MutexGuard<'static, HashMap<PathBuf, NestedRepoPathCache>> {
    match NESTED_REPO_ROOTS_CACHE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
