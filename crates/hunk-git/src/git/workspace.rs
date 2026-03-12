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
        let mut new_entry = worktree_entry_summary(root, path.as_str())?;
        let index_has_entry = filter_index
            .entry_by_path(path.as_bytes().as_bstr())
            .is_some();
        let mut status = if candidate.staged_status == Some(FileStatus::Conflicted)
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
        if status.is_none() && candidate.staged_status.is_some() {
            new_entry = index_entry_summary(filter_index, path.as_str())?;
            status = aggregate_file_status_from_summaries(
                old_entry.as_ref(),
                new_entry.as_ref(),
                index_has_entry,
                rename_from.as_deref(),
            )
            .or(candidate.staged_status);
        }
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
                    unstaged: candidate.worktree_status.is_some(),
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
                unstaged: file.unstaged,
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

fn expand_selected_paths_for_renames_from_repo(
    repo: &GitRepo,
    selected_paths: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    if selected_paths.is_empty() {
        return Ok(BTreeSet::new());
    }

    let candidates =
        collect_candidate_files(repo.repository(), repo.root(), Some(selected_paths))?;
    let mut expanded_paths = selected_paths.clone();
    for (path, candidate) in candidates {
        expanded_paths.insert(path);
        if let Some(rename_from) = candidate.rename_from {
            expanded_paths.insert(rename_from);
        }
    }

    Ok(expanded_paths)
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
    let mut new_state = worktree_file_state(repo, root, filter_pipeline, index, path.as_str())?;
    let mut new_entry = worktree_entry_summary(root, path.as_str())?;
    let index_has_entry = index.entry_by_path(path.as_bytes().as_bstr()).is_some();
    let mut status = if candidate.staged_status == Some(FileStatus::Conflicted)
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
    if status.is_none() && candidate.staged_status.is_some() {
        new_state = index_file_state(repo, index, path.as_str())?;
        new_entry = index_entry_summary(index, path.as_str())?;
        status = aggregate_file_status(
            old_state.as_ref(),
            new_state.as_ref(),
            index_has_entry,
            rename_from.as_deref(),
        )
        .or(candidate.staged_status);
    }
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
        unstaged: candidate.worktree_status.is_some(),
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
    let mut nested_repo_filter = NestedRepoFilter::load(root);
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
        if repo_relative_path_is_within_managed_worktrees(path.as_str()) {
            continue;
        }
        if nested_repo_filter.contains_path(path.as_str()) {
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
    nested_repo_filter.persist();
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

fn list_branch_workspace_occupancy(
    path: &Path,
) -> Result<HashMap<String, BranchWorkspaceOccupancy>> {
    let mut occupancy = HashMap::new();
    for target in list_workspace_targets(path)? {
        if matches!(target.branch_name.as_str(), "detached" | "unborn") {
            continue;
        }
        occupancy.insert(
            target.branch_name.clone(),
            BranchWorkspaceOccupancy {
                target_id: target.id,
                target_root: target.root,
                target_label: workspace_target_branch_label(target.kind, target.name.as_str()),
            },
        );
    }
    Ok(occupancy)
}

fn workspace_target_branch_label(kind: WorkspaceTargetKind, name: &str) -> String {
    match kind {
        WorkspaceTargetKind::PrimaryCheckout => "Primary Checkout".to_string(),
        WorkspaceTargetKind::LinkedWorktree => name.to_string(),
    }
}

fn list_local_branches(
    repo: &gix::Repository,
    current_head_ref_name: Option<&str>,
    workspace_occupancy_by_branch: &HashMap<String, BranchWorkspaceOccupancy>,
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
        let occupancy = workspace_occupancy_by_branch.get(name);
        branches.push(LocalBranch {
            name: name.to_string(),
            is_current: Some(full_name.as_str()) == current_head_ref_name,
            tip_unix_time,
            attached_workspace_target_id: occupancy.map(|target| target.target_id.clone()),
            attached_workspace_target_root: occupancy.map(|target| target.target_root.clone()),
            attached_workspace_target_label: occupancy.map(|target| target.target_label.clone()),
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
