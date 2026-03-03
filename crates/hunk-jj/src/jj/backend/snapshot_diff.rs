fn refresh_working_copy_snapshot(context: &mut RepoContext) -> Result<()> {
    import_git_head_for_snapshot(context)?;
    ensure_local_bookmark_for_git_head(context)?;

    let workspace_name = context.workspace.workspace_name().to_owned();
    let wc_commit =
        current_wc_commit_with_repo(context.repo.as_ref(), context.workspace.workspace_name())?;
    let old_tree = wc_commit.tree();

    let mut locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .context("failed to lock jj working copy")?;

    let snapshot_options = SnapshotOptions {
        base_ignores: jj_lib::gitignore::GitIgnoreFile::empty(),
        progress: None,
        start_tracking_matcher: &EverythingMatcher,
        force_tracking_matcher: &NothingMatcher,
        max_new_file_size: u64::MAX,
    };

    let (new_tree, _) = block_on(locked_workspace.locked_wc().snapshot(&snapshot_options))
        .context("failed to snapshot jj working copy")?;

    let mut repo = context.repo.clone();
    if new_tree.tree_ids_and_labels() != old_tree.tree_ids_and_labels() {
        let mut tx = repo.start_transaction();
        let rewritten_wc = tx
            .repo_mut()
            .rewrite_commit(&wc_commit)
            .set_tree(new_tree)
            .write()
            .context("failed to record working-copy snapshot")?;
        tx.repo_mut()
            .set_wc_commit(workspace_name.clone(), rewritten_wc.id().clone())
            .context("failed to update working-copy commit")?;
        tx.repo_mut()
            .rebase_descendants()
            .context("failed to rebase descendants after snapshot")?;
        repo = tx
            .commit("snapshot working copy")
            .context("failed to finalize working-copy snapshot")?;
    }

    locked_workspace
        .finish(repo.op_id().clone())
        .context("failed to persist jj working-copy state")?;
    context.repo = repo;

    import_git_refs_for_snapshot(context)?;
    Ok(())
}

fn import_git_head_for_snapshot(context: &mut RepoContext) -> Result<()> {
    let mut tx = context.repo.start_transaction();
    git::import_head(tx.repo_mut()).context("failed to import Git HEAD into JJ view")?;
    if !tx.repo().has_changes() {
        return Ok(());
    }

    if let Some(new_git_head_id) = tx.repo().view().git_head().as_normal().cloned() {
        let workspace_name = context.workspace.workspace_name().to_owned();
        let new_git_head_commit = tx
            .repo()
            .store()
            .get_commit(&new_git_head_id)
            .context("failed to load imported Git HEAD commit")?;
        let wc_commit = tx
            .repo_mut()
            .check_out(workspace_name, &new_git_head_commit)
            .context("failed to reset working-copy parent to Git HEAD")?;

        let mut locked_workspace = context
            .workspace
            .start_working_copy_mutation()
            .context("failed to lock working copy while importing Git HEAD")?;
        block_on(locked_workspace.locked_wc().reset(&wc_commit))
            .context("failed to reset working-copy state to imported Git HEAD")?;
        tx.repo_mut()
            .rebase_descendants()
            .context("failed to rebase descendants after Git HEAD import")?;

        let repo = tx
            .commit("import git head")
            .context("failed to finalize Git HEAD import operation")?;
        locked_workspace
            .finish(repo.op_id().clone())
            .context("failed to persist working-copy state after importing Git HEAD")?;
        context.repo = repo;
        return Ok(());
    }

    let repo = tx
        .commit("import git head")
        .context("failed to record imported Git HEAD state")?;
    persist_working_copy_state(context, repo, "after importing Git HEAD")
}

fn ensure_local_bookmark_for_git_head(context: &mut RepoContext) -> Result<()> {
    let Some(branch_name) = git_head_branch_name_from_context(context) else {
        return Ok(());
    };

    if context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name.as_str()))
        .is_present()
    {
        return Ok(());
    }

    let Some(git_head_id) = context.repo.view().git_head().as_normal().cloned() else {
        return Ok(());
    };

    let mut tx = context.repo.start_transaction();
    tx.repo_mut().set_local_bookmark_target(
        RefName::new(branch_name.as_str()),
        RefTarget::normal(git_head_id),
    );

    let repo = tx
        .commit(format!("create bookmark {branch_name} from git head"))
        .with_context(|| {
            format!("failed to create local bookmark '{branch_name}' from Git HEAD")
        })?;
    persist_working_copy_state(context, repo, "after creating Git HEAD bookmark")
}

fn import_git_refs_for_snapshot(context: &mut RepoContext) -> Result<()> {
    let import_options = git_import_options_from_settings(&context.settings)?;
    let mut tx = context.repo.start_transaction();
    git::import_refs(tx.repo_mut(), &import_options)
        .context("failed to import Git refs into JJ view")?;
    if !tx.repo().has_changes() {
        return Ok(());
    }

    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after importing Git refs")?;
    let repo = tx
        .commit("import git refs")
        .context("failed to finalize Git ref import operation")?;
    persist_working_copy_state(context, repo, "after importing Git refs")
}

fn current_wc_commit_with_repo(
    repo: &ReadonlyRepo,
    workspace_name: &WorkspaceName,
) -> Result<Commit> {
    let wc_commit_id = repo
        .view()
        .get_wc_commit_id(workspace_name)
        .ok_or_else(|| {
            anyhow!(
                "workspace '{}' has no working-copy commit",
                workspace_name.as_symbol()
            )
        })?;
    repo.store()
        .get_commit(wc_commit_id)
        .context("failed to load working-copy commit")
}

fn current_wc_commit(context: &RepoContext) -> Result<Commit> {
    current_wc_commit_with_repo(context.repo.as_ref(), context.workspace.workspace_name())
}

fn persist_working_copy_state(
    context: &mut RepoContext,
    repo: Arc<ReadonlyRepo>,
    operation: &str,
) -> Result<()> {
    let locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .with_context(|| format!("failed to lock working copy {operation}"))?;
    locked_workspace
        .finish(repo.op_id().clone())
        .with_context(|| format!("failed to persist working-copy state {operation}"))?;
    context.repo = repo;
    Ok(())
}

pub(super) fn load_changed_files_from_context(context: &RepoContext) -> Result<Vec<ChangedFile>> {
    let wc_commit = current_wc_commit(context)?;
    let base_tree = wc_commit.parent_tree(context.repo.as_ref())?;
    let current_tree = wc_commit.tree();
    let nested_repo_roots = nested_repo_roots_for_context(context)?;

    let mut file_map = BTreeMap::<String, ChangedFile>::new();
    for entry in block_on_stream(base_tree.diff_stream(&current_tree, &EverythingMatcher)) {
        let path = normalize_path(entry.path.as_internal_file_string());
        if path.is_empty() {
            continue;
        }
        if path_is_within_nested_repo(path.as_str(), nested_repo_roots) {
            continue;
        }

        let values = entry.values?;
        let status = map_tree_diff_status(&values);
        let untracked = status == FileStatus::Added;
        let incoming = ChangedFile {
            path: path.clone(),
            status,
            staged: false,
            untracked,
        };

        file_map
            .entry(path)
            .and_modify(|existing| {
                existing.status = merge_file_status(existing.status, incoming.status);
                existing.untracked &= incoming.untracked;
            })
            .or_insert(incoming);
    }

    Ok(file_map.into_values().collect())
}

fn map_tree_diff_status(values: &Diff<MergedTreeValue>) -> FileStatus {
    if !values.before.is_resolved() || !values.after.is_resolved() {
        return FileStatus::Conflicted;
    }

    if values.before.is_absent() && !values.after.is_absent() {
        return FileStatus::Added;
    }
    if !values.before.is_absent() && values.after.is_absent() {
        return FileStatus::Deleted;
    }

    if values.before.is_tree() || values.after.is_tree() {
        return FileStatus::TypeChange;
    }

    FileStatus::Modified
}

fn merge_file_status(existing: FileStatus, incoming: FileStatus) -> FileStatus {
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

    if priority(incoming) >= priority(existing) {
        incoming
    } else {
        existing
    }
}

pub(super) fn current_bookmarks_from_context(context: &RepoContext) -> Result<BTreeSet<String>> {
    let wc_commit = current_wc_commit(context)?;
    let mut current: BTreeSet<String> = context
        .repo
        .view()
        .local_bookmarks_for_commit(wc_commit.id())
        .map(|(name, _)| name.as_str().to_string())
        .collect();

    if let Some(parent_id) = wc_commit.parent_ids().first() {
        current.extend(
            context
                .repo
                .view()
                .local_bookmarks_for_commit(parent_id)
                .map(|(name, _)| name.as_str().to_string()),
        );
    }

    Ok(current)
}

pub(super) fn git_head_branch_name_from_context(context: &RepoContext) -> Option<String> {
    let git_repo = git::get_git_repo(context.repo.store()).ok()?;
    let head_ref = git_repo.find_reference("HEAD").ok()?;
    let target = head_ref.target();
    let target_name = target.try_name()?;
    let target_name = std::str::from_utf8(target_name.as_bstr()).ok()?;
    target_name
        .strip_prefix("refs/heads/")
        .map(|name| name.to_string())
}

pub(super) fn bookmark_remote_sync_state(
    context: &RepoContext,
    branch_name: &str,
) -> (bool, usize) {
    let mut has_upstream = false;
    let mut revisions_to_push = 0usize;

    for (remote, _) in context.repo.view().remote_views() {
        if remote == REMOTE_NAME_FOR_LOCAL_GIT_REPO {
            continue;
        }

        let Some((_, targets)) = context
            .repo
            .view()
            .local_remote_bookmarks(remote)
            .find(|(name, _)| name.as_str() == branch_name)
        else {
            continue;
        };

        // Treat any present remote bookmark as upstream, even if tracking metadata
        // is temporarily conflicted.
        if !targets.remote_ref.is_present() {
            continue;
        }

        has_upstream = true;
        if let BookmarkPushAction::Update(update) = classify_bookmark_push_action(targets) {
            let ahead_count = bookmark_push_update_revision_count(context, &update);
            revisions_to_push = revisions_to_push.max(ahead_count);
        }
    }

    (has_upstream, revisions_to_push)
}

fn bookmark_push_update_revision_count(
    context: &RepoContext,
    update: &jj_lib::refs::BookmarkPushUpdate,
) -> usize {
    let Some(new_target) = update.new_target.clone() else {
        return 0;
    };

    let mut ahead = RevsetExpression::commit(new_target).ancestors();
    if let Some(old_target) = update.old_target.clone() {
        ahead = ahead.minus(&RevsetExpression::commit(old_target).ancestors());
    }

    let Ok(revset) = ahead.evaluate(context.repo.as_ref()) else {
        return 0;
    };
    revset.iter().flatten().count()
}

pub(super) fn list_local_branches_from_context(
    context: &RepoContext,
    current: &BTreeSet<String>,
) -> Result<Vec<LocalBranch>> {
    let mut branches = Vec::new();
    for (name, target) in context.repo.view().local_bookmarks() {
        if !target.is_present() {
            continue;
        }

        let tip_unix_time = target
            .as_normal()
            .map(|id| -> Result<i64> {
                let commit = context.repo.store().get_commit(id)?;
                Ok(commit.committer().timestamp.timestamp.0 / 1000)
            })
            .transpose()?;

        branches.push(LocalBranch {
            is_current: current.contains(name.as_str()),
            name: name.as_str().to_string(),
            tip_unix_time,
        });
    }

    branches.sort_by(|a, b| {
        b.is_current
            .cmp(&a.is_current)
            .then_with(|| b.tip_unix_time.cmp(&a.tip_unix_time))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(branches)
}

pub(super) fn list_bookmark_revisions_from_context(
    context: &RepoContext,
    branch_name: &str,
    limit: usize,
) -> Result<Vec<BookmarkRevision>> {
    if limit == 0 || branch_name.trim().is_empty() || branch_name == "detached" {
        return Ok(Vec::new());
    }

    let Some(mut current_id) = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name))
        .as_normal()
        .cloned()
    else {
        return Ok(Vec::new());
    };

    let mut revisions = Vec::with_capacity(limit);
    let mut seen_ids = BTreeSet::new();

    while revisions.len() < limit {
        let current_hex = current_id.hex();
        if !seen_ids.insert(current_hex.clone()) {
            break;
        }

        let commit = context
            .repo
            .store()
            .get_commit(&current_id)
            .with_context(|| format!("failed to load bookmark revision {current_hex}"))?;
        let subject = commit
            .description()
            .lines()
            .next()
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .unwrap_or("(no description)")
            .to_string();
        let unix_time = commit.committer().timestamp.timestamp.0 / 1000;

        revisions.push(BookmarkRevision {
            id: current_hex,
            subject,
            unix_time,
        });

        let Some(parent_id) = commit.parent_ids().first().cloned() else {
            break;
        };
        current_id = parent_id;
    }

    Ok(revisions)
}

pub(super) fn current_commit_id_from_context(context: &RepoContext) -> Result<Option<String>> {
    Ok(Some(current_wc_commit(context)?.id().hex()))
}

pub(super) fn last_commit_subject_from_context(context: &RepoContext) -> Result<Option<String>> {
    let wc_commit = current_wc_commit(context)?;
    let Some(parent_id) = wc_commit.parent_ids().first() else {
        return Ok(None);
    };

    let parent = context
        .repo
        .store()
        .get_commit(parent_id)
        .context("failed to load parent commit")?;
    let subject = parent
        .description()
        .lines()
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    if subject.is_empty() {
        Ok(None)
    } else {
        Ok(Some(subject))
    }
}

pub(super) fn repo_line_stats_from_context(context: &RepoContext) -> Result<LineStats> {
    let materialize_options = conflict_materialize_options(context);
    let nested_repo_roots = nested_repo_roots_for_context(context)?;
    let mut stats = LineStats::default();
    let wc_commit = current_wc_commit(context)?;
    let base_tree = wc_commit.parent_tree(context.repo.as_ref())?;
    let current_tree = wc_commit.tree();
    let copy_records = CopyRecords::default();
    let stream = materialized_diff_stream(
        context.repo.store().as_ref(),
        base_tree.diff_stream_with_copies(&current_tree, &EverythingMatcher, &copy_records),
        Diff::new(base_tree.labels(), current_tree.labels()),
    );

    for entry in block_on_stream(stream) {
        if materialized_entry_within_nested_repo(&entry, nested_repo_roots) {
            continue;
        }
        let entry_stats = line_stats_for_entry(entry, &materialize_options)?;
        stats.added += entry_stats.added;
        stats.removed += entry_stats.removed;
    }

    Ok(stats)
}

fn nested_repo_roots_for_context(context: &RepoContext) -> Result<&BTreeSet<String>> {
    if let Some(cached) = context.nested_repo_roots_cache.get() {
        return Ok(cached);
    }

    let roots = nested_repo_roots_from_fs(&context.root)?;
    let _ = context.nested_repo_roots_cache.set(roots);
    context
        .nested_repo_roots_cache
        .get()
        .ok_or_else(|| anyhow!("failed to cache nested repository roots"))
}

pub(super) fn conflict_materialize_options(context: &RepoContext) -> ConflictMaterializeOptions {
    ConflictMaterializeOptions {
        marker_style: ConflictMarkerStyle::Git,
        marker_len: None,
        merge: context.repo.store().merge_options().clone(),
    }
}

pub(super) fn collect_materialized_diff_entries_for_paths(
    context: &RepoContext,
    paths: &BTreeSet<String>,
) -> Result<Vec<MaterializedTreeDiffEntry>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut repo_paths = Vec::with_capacity(paths.len());
    for path in paths {
        let repo_path = RepoPathBuf::from_relative_path(Path::new(path))
            .with_context(|| format!("invalid repository path '{path}'"))?;
        repo_paths.push(repo_path);
    }

    let wc_commit = current_wc_commit(context)?;
    let base_tree = wc_commit.parent_tree(context.repo.as_ref())?;
    let current_tree = wc_commit.tree();
    let copy_records = CopyRecords::default();
    let matcher = FilesMatcher::new(repo_paths.iter());

    let stream = materialized_diff_stream(
        context.repo.store().as_ref(),
        base_tree.diff_stream_with_copies(&current_tree, &matcher, &copy_records),
        Diff::new(base_tree.labels(), current_tree.labels()),
    );

    let mut entries = Vec::new();
    for entry in block_on_stream(stream) {
        entries.push(entry);
    }
    Ok(entries)
}

pub(super) fn materialized_entry_matches_path(
    entry: &MaterializedTreeDiffEntry,
    file_path: &str,
) -> bool {
    let target = normalize_path(entry.path.target().as_internal_file_string());
    let source = normalize_path(entry.path.source().as_internal_file_string());
    target == file_path || source == file_path
}

pub(super) fn render_patch_for_entry(
    entry: MaterializedTreeDiffEntry,
    materialize_options: &ConflictMaterializeOptions,
) -> Result<RenderedPatch> {
    let values = entry.values?;
    let source_path = normalize_path(entry.path.source().as_internal_file_string());
    let target_path = normalize_path(entry.path.target().as_internal_file_string());

    let before_part = git_diff_part(entry.path.source(), values.before, materialize_options)?;
    let after_part = git_diff_part(entry.path.target(), values.after, materialize_options)?;

    let mut patch = String::new();
    let display_source = if source_path.is_empty() {
        target_path.as_str()
    } else {
        source_path.as_str()
    };
    let display_target = if target_path.is_empty() {
        source_path.as_str()
    } else {
        target_path.as_str()
    };

    patch.push_str(&format!(
        "diff --git a/{display_source} b/{display_target}\n"
    ));

    match (before_part.mode, after_part.mode) {
        (None, Some(new_mode)) => patch.push_str(&format!("new file mode {new_mode}\n")),
        (Some(old_mode), None) => patch.push_str(&format!("deleted file mode {old_mode}\n")),
        (Some(old_mode), Some(new_mode)) if old_mode != new_mode => {
            patch.push_str(&format!("old mode {old_mode}\n"));
            patch.push_str(&format!("new mode {new_mode}\n"));
        }
        _ => {}
    }

    match (before_part.mode, after_part.mode) {
        (Some(mode), Some(new_mode)) if mode == new_mode => {
            patch.push_str(&format!(
                "index {}..{} {mode}\n",
                before_part.hash, after_part.hash
            ));
        }
        _ => {
            patch.push_str(&format!(
                "index {}..{}\n",
                before_part.hash, after_part.hash
            ));
        }
    }

    let before_label = if before_part.mode.is_some() {
        format!("a/{display_source}")
    } else {
        "/dev/null".to_string()
    };
    let after_label = if after_part.mode.is_some() {
        format!("b/{display_target}")
    } else {
        "/dev/null".to_string()
    };

    if before_part.content.is_binary || after_part.content.is_binary {
        if before_part.content.contents != after_part.content.contents {
            patch.push_str(&format!(
                "Binary files {before_label} and {after_label} differ\n"
            ));
        }
        return Ok(RenderedPatch { patch });
    }

    let hunks = unified_diff_hunks(
        Diff::new(
            before_part.content.contents.as_ref(),
            after_part.content.contents.as_ref(),
        ),
        3,
        LineCompareMode::Exact,
    );

    if hunks.is_empty() {
        return Ok(RenderedPatch { patch });
    }

    patch.push_str(&format!("--- {before_label}\n"));
    patch.push_str(&format!("+++ {after_label}\n"));

    for hunk in hunks {
        patch.push_str(&format_unified_hunk_header(&hunk));
        for (line_type, tokens) in hunk.lines {
            let prefix = match line_type {
                DiffLineType::Context => ' ',
                DiffLineType::Removed => '-',
                DiffLineType::Added => '+',
            };
            append_hunk_line(&mut patch, prefix, &tokens);
        }
    }

    Ok(RenderedPatch { patch })
}

fn line_stats_for_entry(
    entry: MaterializedTreeDiffEntry,
    materialize_options: &ConflictMaterializeOptions,
) -> Result<LineStats> {
    let values = entry.values?;
    let before_part = git_diff_part(entry.path.source(), values.before, materialize_options)?;
    let after_part = git_diff_part(entry.path.target(), values.after, materialize_options)?;

    if before_part.content.is_binary || after_part.content.is_binary {
        return Ok(LineStats::default());
    }

    let hunks = unified_diff_hunks(
        Diff::new(
            before_part.content.contents.as_ref(),
            after_part.content.contents.as_ref(),
        ),
        3,
        LineCompareMode::Exact,
    );

    Ok(line_stats_from_hunks(&hunks))
}

fn format_unified_hunk_header(hunk: &UnifiedDiffHunk<'_>) -> String {
    let (left_start, left_count) = format_unified_range(&hunk.left_line_range);
    let (right_start, right_count) = format_unified_range(&hunk.right_line_range);
    format!("@@ -{left_start},{left_count} +{right_start},{right_count} @@\n")
}

fn format_unified_range(range: &std::ops::Range<usize>) -> (usize, usize) {
    let count = range.end.saturating_sub(range.start);
    let start = if count == 0 {
        range.start
    } else {
        range.start.saturating_add(1)
    };
    (start, count)
}

fn append_hunk_line(
    patch: &mut String,
    prefix: char,
    tokens: &[(jj_lib::diff_presentation::DiffTokenType, &[u8])],
) {
    let mut bytes = Vec::new();
    for (_, part) in tokens {
        bytes.extend_from_slice(part);
    }

    patch.push(prefix);
    patch.push_str(String::from_utf8_lossy(&bytes).as_ref());
    if !bytes.ends_with(b"\n") {
        patch.push('\n');
    }
}

fn materialized_entry_within_nested_repo(
    entry: &MaterializedTreeDiffEntry,
    nested_repo_roots: &BTreeSet<String>,
) -> bool {
    let source = normalize_path(entry.path.source().as_internal_file_string());
    let target = normalize_path(entry.path.target().as_internal_file_string());
    path_is_within_nested_repo(source.as_str(), nested_repo_roots)
        || path_is_within_nested_repo(target.as_str(), nested_repo_roots)
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

fn line_stats_from_hunks(hunks: &[UnifiedDiffHunk<'_>]) -> LineStats {
    let mut stats = LineStats::default();
    for hunk in hunks {
        for (line_type, _) in &hunk.lines {
            match line_type {
                DiffLineType::Added => stats.added += 1,
                DiffLineType::Removed => stats.removed += 1,
                DiffLineType::Context => {}
            }
        }
    }
    stats
}

pub(super) fn load_tracked_paths_from_context(context: &RepoContext) -> Result<BTreeSet<String>> {
    let wc_commit = current_wc_commit(context)?;
    let tree = wc_commit.tree();

    let mut tracked = BTreeSet::new();
    for (path, value) in tree.entries() {
        let value = value?;
        if value.is_absent() || value.is_tree() {
            continue;
        }
        let path = normalize_path(path.as_internal_file_string());
        if !path.is_empty() {
            tracked.insert(path);
        }
    }
    Ok(tracked)
}
