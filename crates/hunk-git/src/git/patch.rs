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

fn index_file_state(
    repo: &gix::Repository,
    index: &gix::index::State,
    path: &str,
) -> Result<Option<FileState>> {
    let Some(entry) = index.entry_by_path(path.as_bytes().as_bstr()) else {
        return Ok(None);
    };
    let kind = entry
        .mode
        .to_tree_entry_mode()
        .map(|mode| mode.kind())
        .unwrap_or(gix::objs::tree::EntryKind::Blob);
    let bytes = match file_kind_class(kind) {
        FileKindClass::Regular | FileKindClass::Link => {
            let mut blob = repo
                .find_blob(entry.id)
                .with_context(|| format!("failed to load blob for '{path}' from index"))?;
            Some(blob.take_data())
        }
        FileKindClass::Unsupported => None,
    };

    Ok(Some(FileState {
        kind,
        id: entry.id,
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

fn index_entry_summary(index: &gix::index::State, path: &str) -> Result<Option<WorktreeEntrySummary>> {
    let Some(entry) = index.entry_by_path(path.as_bytes().as_bstr()) else {
        return Ok(None);
    };
    let kind = entry
        .mode
        .to_tree_entry_mode()
        .map(|mode| mode.kind())
        .unwrap_or(gix::objs::tree::EntryKind::Blob);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut hasher);
    entry.id.hash(&mut hasher);
    Ok(Some(WorktreeEntrySummary {
        kind,
        signature: hasher.finish(),
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
