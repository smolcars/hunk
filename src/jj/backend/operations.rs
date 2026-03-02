pub(super) fn commit_working_copy_changes(context: &mut RepoContext, message: &str) -> Result<()> {
    let workspace_name = context.workspace.workspace_name().to_owned();
    let wc_commit = current_wc_commit(context)?;

    let mut tx = context.repo.start_transaction();
    let committed = tx
        .repo_mut()
        .rewrite_commit(&wc_commit)
        .set_description(message)
        .write()
        .context("failed to create committed revision")?;
    let new_wc = tx
        .repo_mut()
        .new_commit(vec![committed.id().clone()], committed.tree())
        .write()
        .context("failed to create next working-copy revision")?;
    tx.repo_mut()
        .set_wc_commit(workspace_name.clone(), new_wc.id().clone())
        .context("failed to update working-copy commit")?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after commit")?;

    let repo = tx
        .commit(format!("commit: {message}"))
        .context("failed to finalize commit")?;
    persist_working_copy_state(context, repo, "after commit")
}

pub(super) fn commit_working_copy_selected_paths(
    context: &mut RepoContext,
    message: &str,
    selected_paths: &[String],
) -> Result<usize> {
    if selected_paths.is_empty() {
        return Err(anyhow!("no files selected for commit"));
    }

    let workspace_name = context.workspace.workspace_name().to_owned();
    let wc_commit = current_wc_commit(context)?;
    let base_tree = wc_commit.parent_tree(context.repo.as_ref())?;
    let wc_tree = wc_commit.tree();

    let mut normalized_paths = BTreeSet::new();
    for path in selected_paths {
        let normalized = normalize_path(path);
        if normalized.is_empty() {
            continue;
        }
        normalized_paths.insert(normalized);
    }
    if normalized_paths.is_empty() {
        return Err(anyhow!("no valid files selected for commit"));
    }

    let mut repo_paths = Vec::with_capacity(normalized_paths.len());
    for normalized in &normalized_paths {
        let repo_path = RepoPathBuf::from_relative_path(Path::new(normalized.as_str()))
            .with_context(|| format!("invalid repository path '{normalized}'"))?;
        repo_paths.push(repo_path);
    }

    let matcher = jj_lib::matchers::FilesMatcher::new(repo_paths.iter());
    let selected_tree = block_on(restore_tree(
        &wc_tree,
        &base_tree,
        "working copy".to_string(),
        "parent".to_string(),
        &matcher,
    ))
    .context("failed to select files for commit")?;

    if selected_tree.tree_ids_and_labels() == base_tree.tree_ids_and_labels() {
        return Err(anyhow!("selected files have no changes to commit"));
    }

    let mut tx = context.repo.start_transaction();
    let committed = tx
        .repo_mut()
        .rewrite_commit(&wc_commit)
        .set_description(message)
        .set_tree(selected_tree)
        .write()
        .context("failed to create commit for selected files")?;
    let new_wc = tx
        .repo_mut()
        .new_commit(vec![committed.id().clone()], wc_tree)
        .write()
        .context("failed to create next working-copy revision after partial commit")?;
    tx.repo_mut()
        .set_wc_commit(workspace_name.clone(), new_wc.id().clone())
        .context("failed to update working-copy commit after partial commit")?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after partial commit")?;

    let repo = tx
        .commit(format!("commit selected paths: {message}"))
        .context("failed to finalize partial commit")?;
    persist_working_copy_state(context, repo, "after partial commit")?;
    Ok(repo_paths.len())
}

pub(super) fn restore_working_copy_from_revision(
    context: &mut RepoContext,
    source_revision_id: &str,
) -> Result<()> {
    let source_revision_id = source_revision_id.trim();
    if source_revision_id.is_empty() {
        return Err(anyhow!("source revision id cannot be empty"));
    }
    let Some(source_commit_id) = jj_lib::backend::CommitId::try_from_hex(source_revision_id) else {
        return Err(anyhow!("invalid source revision id '{source_revision_id}'"));
    };

    let source_commit = context
        .repo
        .store()
        .get_commit(&source_commit_id)
        .with_context(|| format!("failed to load source revision '{source_revision_id}'"))?;
    let workspace_name = context.workspace.workspace_name().to_owned();
    let wc_commit = current_wc_commit(context)?;

    let mut locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .context("failed to lock working copy for restore")?;

    let mut tx = context.repo.start_transaction();
    let rewritten_wc = tx
        .repo_mut()
        .rewrite_commit(&wc_commit)
        .set_tree(source_commit.tree())
        .write()
        .context("failed to rewrite working-copy commit for restore")?;
    tx.repo_mut()
        .set_wc_commit(workspace_name.clone(), rewritten_wc.id().clone())
        .context("failed to update working-copy commit for restore")?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after restore")?;

    let short_source = source_revision_id.chars().take(12).collect::<String>();
    let repo = tx
        .commit(format!("restore working copy from revision {short_source}"))
        .context("failed to finalize working-copy restore")?;

    let new_wc_commit = current_wc_commit_with_repo(repo.as_ref(), &workspace_name)?;
    block_on(locked_workspace.locked_wc().check_out(&new_wc_commit))
        .context("failed to update working-copy files after restore")?;
    locked_workspace
        .finish(repo.op_id().clone())
        .context("failed to persist working-copy state after restore")?;

    context.repo = repo;
    Ok(())
}

pub(super) fn restore_working_copy_selected_paths(
    context: &mut RepoContext,
    selected_paths: &[String],
) -> Result<usize> {
    if selected_paths.is_empty() {
        return Err(anyhow!("no files selected to restore"));
    }

    let workspace_name = context.workspace.workspace_name().to_owned();
    let wc_commit = current_wc_commit(context)?;
    let base_tree = wc_commit.parent_tree(context.repo.as_ref())?;
    let wc_tree = wc_commit.tree();

    let mut normalized_paths = BTreeSet::new();
    for path in selected_paths {
        let normalized = normalize_path(path);
        if normalized.is_empty() {
            continue;
        }
        normalized_paths.insert(normalized);
    }
    if normalized_paths.is_empty() {
        return Err(anyhow!("no valid files selected to restore"));
    }

    let mut repo_paths = Vec::with_capacity(normalized_paths.len());
    for normalized in &normalized_paths {
        let repo_path = RepoPathBuf::from_relative_path(Path::new(normalized.as_str()))
            .with_context(|| format!("invalid repository path '{normalized}'"))?;
        repo_paths.push(repo_path);
    }

    let matcher = FilesMatcher::new(repo_paths.iter());
    let restored_tree = block_on(restore_tree(
        &base_tree,
        &wc_tree,
        "parent".to_string(),
        "working copy".to_string(),
        &matcher,
    ))
    .context("failed to restore selected files in working copy")?;

    if restored_tree.tree_ids_and_labels() == wc_tree.tree_ids_and_labels() {
        return Err(anyhow!("selected files have no changes to restore"));
    }

    let mut locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .context("failed to lock working copy for selected-path restore")?;

    let mut tx = context.repo.start_transaction();
    let rewritten_wc = tx
        .repo_mut()
        .rewrite_commit(&wc_commit)
        .set_tree(restored_tree)
        .write()
        .context("failed to rewrite working-copy commit for selected-path restore")?;
    tx.repo_mut()
        .set_wc_commit(workspace_name.clone(), rewritten_wc.id().clone())
        .context("failed to update working-copy commit for selected-path restore")?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after selected-path restore")?;

    let repo = tx
        .commit("restore selected working-copy paths")
        .context("failed to finalize selected-path restore")?;

    let new_wc_commit = current_wc_commit_with_repo(repo.as_ref(), &workspace_name)?;
    block_on(locked_workspace.locked_wc().check_out(&new_wc_commit))
        .context("failed to update working-copy files after selected-path restore")?;
    locked_workspace
        .finish(repo.op_id().clone())
        .context("failed to persist working-copy state after selected-path restore")?;

    context.repo = repo;
    Ok(repo_paths.len())
}

pub(super) fn restore_all_working_copy_changes(context: &mut RepoContext) -> Result<()> {
    let workspace_name = context.workspace.workspace_name().to_owned();
    let wc_commit = current_wc_commit(context)?;
    let base_tree = wc_commit.parent_tree(context.repo.as_ref())?;
    let wc_tree = wc_commit.tree();

    if base_tree.tree_ids_and_labels() == wc_tree.tree_ids_and_labels() {
        return Err(anyhow!("working copy has no changes to restore"));
    }

    let mut locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .context("failed to lock working copy for restore")?;

    let mut tx = context.repo.start_transaction();
    let rewritten_wc = tx
        .repo_mut()
        .rewrite_commit(&wc_commit)
        .set_tree(base_tree)
        .write()
        .context("failed to rewrite working-copy commit for restore")?;
    tx.repo_mut()
        .set_wc_commit(workspace_name.clone(), rewritten_wc.id().clone())
        .context("failed to update working-copy commit for restore")?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after restore")?;

    let repo = tx
        .commit("restore all working-copy changes")
        .context("failed to finalize working-copy restore")?;

    let new_wc_commit = current_wc_commit_with_repo(repo.as_ref(), &workspace_name)?;
    block_on(locked_workspace.locked_wc().check_out(&new_wc_commit))
        .context("failed to update working-copy files after restore")?;
    locked_workspace
        .finish(repo.op_id().clone())
        .context("failed to persist working-copy state after restore")?;

    context.repo = repo;
    Ok(())
}

pub(super) fn move_bookmark_to_parent_of_working_copy(
    context: &mut RepoContext,
    branch_name: &str,
) -> Result<bool> {
    let bookmark_target = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name));
    if !bookmark_target.is_present() {
        return Ok(false);
    }

    let wc_commit = current_wc_commit(context)?;
    let Some(parent_id) = wc_commit.parent_ids().first().cloned() else {
        return Ok(false);
    };

    let mut tx = context.repo.start_transaction();
    tx.repo_mut()
        .set_local_bookmark_target(RefName::new(branch_name), RefTarget::normal(parent_id));
    let repo = tx
        .commit(format!("move bookmark {branch_name} to committed revision"))
        .with_context(|| format!("failed to advance bookmark '{branch_name}'"))?;
    persist_working_copy_state(context, repo, "after moving bookmark")?;
    Ok(true)
}

pub(super) fn set_local_bookmark_target_revision(
    context: &mut RepoContext,
    branch_name: &str,
    revision_id: &str,
    require_existing_bookmark: bool,
) -> Result<()> {
    let revision_id = revision_id.trim();
    let Some(target_commit_id) = jj_lib::backend::CommitId::try_from_hex(revision_id) else {
        return Err(anyhow!("invalid revision id '{revision_id}'"));
    };
    context
        .repo
        .store()
        .get_commit(&target_commit_id)
        .with_context(|| format!("failed to load target revision '{revision_id}'"))?;
    ensure_revision_is_not_working_copy_commit(context, branch_name, &target_commit_id)?;

    let bookmark_target = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name));
    if require_existing_bookmark && !bookmark_target.is_present() {
        return Err(anyhow!("bookmark '{branch_name}' does not exist"));
    }
    if !require_existing_bookmark && bookmark_target.is_present() {
        return Err(anyhow!("bookmark '{branch_name}' already exists"));
    }
    if bookmark_target.as_normal() == Some(&target_commit_id) {
        return Ok(());
    }

    let mut tx = context.repo.start_transaction();
    tx.repo_mut().set_local_bookmark_target(
        RefName::new(branch_name),
        RefTarget::normal(target_commit_id),
    );
    let repo = tx
        .commit(format!("set bookmark {branch_name} to revision {revision_id}"))
        .with_context(|| format!("failed to update bookmark '{branch_name}'"))?;
    persist_working_copy_state(context, repo, "after setting bookmark target")
}

fn ensure_revision_is_not_working_copy_commit(
    context: &RepoContext,
    branch_name: &str,
    target_commit_id: &jj_lib::backend::CommitId,
) -> Result<()> {
    let wc_commit = current_wc_commit(context)?;
    if wc_commit.id() != target_commit_id {
        return Ok(());
    }

    let short_id = target_commit_id.hex().chars().take(12).collect::<String>();
    Err(anyhow!(
        "cannot point bookmark '{branch_name}' to mutable working-copy revision {short_id}; \
select a committed revision instead"
    ))
}

pub(super) fn checkout_existing_bookmark(
    context: &mut RepoContext,
    branch_name: &str,
) -> Result<()> {
    let workspace_name = context.workspace.workspace_name().to_owned();
    let bookmark_target = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name));
    let commit_id = bookmark_target
        .as_normal()
        .cloned()
        .ok_or_else(|| anyhow!("bookmark '{branch_name}' is conflicted or has no target"))?;
    let target_commit = context
        .repo
        .store()
        .get_commit(&commit_id)
        .with_context(|| format!("failed to load bookmark target for '{branch_name}'"))?;

    let mut locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .context("failed to lock working copy for bookmark checkout")?;

    let mut tx = context.repo.start_transaction();
    let new_wc = tx
        .repo_mut()
        .new_commit(vec![target_commit.id().clone()], target_commit.tree())
        .write()
        .with_context(|| format!("failed to create working-copy commit for '{branch_name}'"))?;
    tx.repo_mut()
        .set_wc_commit(workspace_name.clone(), new_wc.id().clone())
        .with_context(|| format!("failed to set working-copy commit for '{branch_name}'"))?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after bookmark checkout")?;
    let repo = tx
        .commit(format!("checkout bookmark {branch_name}"))
        .context("failed to finalize bookmark checkout")?;

    let new_wc_commit = current_wc_commit_with_repo(repo.as_ref(), &workspace_name)?;
    block_on(locked_workspace.locked_wc().check_out(&new_wc_commit))
        .context("failed to update working-copy files for bookmark checkout")?;
    locked_workspace
        .finish(repo.op_id().clone())
        .context("failed to persist working-copy state after bookmark checkout")?;

    context.repo = repo;
    Ok(())
}

pub(super) fn checkout_existing_bookmark_with_change_transfer(
    context: &mut RepoContext,
    branch_name: &str,
) -> Result<()> {
    let workspace_name = context.workspace.workspace_name().to_owned();
    let source_wc_commit = current_wc_commit(context)?;
    let source_wc_tree = source_wc_commit.tree();
    let bookmark_target = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name));
    let commit_id = bookmark_target
        .as_normal()
        .cloned()
        .ok_or_else(|| anyhow!("bookmark '{branch_name}' is conflicted or has no target"))?;
    let target_commit = context
        .repo
        .store()
        .get_commit(&commit_id)
        .with_context(|| format!("failed to load bookmark target for '{branch_name}'"))?;

    let mut locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .context("failed to lock working copy for bookmark checkout")?;

    let mut tx = context.repo.start_transaction();
    let new_wc = tx
        .repo_mut()
        .new_commit(vec![target_commit.id().clone()], source_wc_tree)
        .write()
        .with_context(|| {
            format!("failed to create working-copy commit for '{branch_name}' with moved changes")
        })?;
    tx.repo_mut()
        .set_wc_commit(workspace_name.clone(), new_wc.id().clone())
        .with_context(|| format!("failed to set working-copy commit for '{branch_name}'"))?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after bookmark checkout")?;
    let repo = tx
        .commit(format!(
            "checkout bookmark {branch_name} and move working-copy changes"
        ))
        .context("failed to finalize bookmark checkout")?;

    let new_wc_commit = current_wc_commit_with_repo(repo.as_ref(), &workspace_name)?;
    block_on(locked_workspace.locked_wc().check_out(&new_wc_commit))
        .context("failed to update working-copy files for bookmark checkout")?;
    locked_workspace
        .finish(repo.op_id().clone())
        .context("failed to persist working-copy state after bookmark checkout")?;

    context.repo = repo;
    Ok(())
}

pub(super) fn create_bookmark_at_working_copy(
    context: &mut RepoContext,
    branch_name: &str,
) -> Result<()> {
    let wc_commit = current_wc_commit(context)?;

    let mut tx = context.repo.start_transaction();
    tx.repo_mut().set_local_bookmark_target(
        RefName::new(branch_name),
        RefTarget::normal(wc_commit.id().clone()),
    );
    let repo = tx
        .commit(format!("create bookmark {branch_name}"))
        .context("failed to create bookmark")?;
    persist_working_copy_state(context, repo, "after bookmark creation")
}

pub(super) fn rename_bookmark(
    context: &mut RepoContext,
    old_branch_name: &str,
    new_branch_name: &str,
) -> Result<()> {
    let old_bookmark = RefName::new(old_branch_name);
    let new_bookmark = RefName::new(new_branch_name);
    let view = context.repo.view();

    let old_target = view.get_local_bookmark(old_bookmark).clone();
    if old_target.is_absent() {
        return Err(anyhow!("bookmark '{old_branch_name}' does not exist"));
    }
    if view.get_local_bookmark(new_bookmark).is_present() {
        return Err(anyhow!("bookmark '{new_branch_name}' already exists"));
    }

    let mut tx = context.repo.start_transaction();
    tx.repo_mut()
        .set_local_bookmark_target(new_bookmark, old_target);
    tx.repo_mut()
        .set_local_bookmark_target(old_bookmark, RefTarget::absent());
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after bookmark rename")?;

    let repo = tx
        .commit(format!(
            "rename bookmark {old_branch_name} to {new_branch_name}"
        ))
        .context("failed to finalize bookmark rename")?;
    persist_working_copy_state(context, repo, "after bookmark rename")
}

fn ensure_bookmark_is_checked_out(context: &RepoContext, branch_name: &str) -> Result<()> {
    let Some(bookmark_tip_id) = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name))
        .as_normal()
        .cloned()
    else {
        return Err(anyhow!("bookmark '{branch_name}' does not exist"));
    };

    let wc_commit = current_wc_commit(context)?;
    let Some(wc_parent_id) = wc_commit.parent_ids().first() else {
        return Err(anyhow!(
            "cannot operate on bookmark '{branch_name}' because the working copy has no parent"
        ));
    };

    if wc_parent_id != &bookmark_tip_id {
        return Err(anyhow!(
            "bookmark '{branch_name}' is not active; activate it before running this action"
        ));
    }

    if let Some(active_bookmark) = load_active_bookmark_preference(&context.root)
        && active_bookmark != branch_name
        && context
            .repo
            .view()
            .local_bookmarks_for_commit(&bookmark_tip_id)
            .any(|(name, _)| name.as_str() == active_bookmark)
    {
        return Err(anyhow!(
            "bookmark '{branch_name}' is not active; activate it before running this action"
        ));
    }

    Ok(())
}

pub(super) fn describe_bookmark_head(
    context: &mut RepoContext,
    branch_name: &str,
    description: &str,
) -> Result<()> {
    ensure_bookmark_is_checked_out(context, branch_name)?;

    let bookmark = RefName::new(branch_name);
    let Some(commit_id) = context
        .repo
        .view()
        .get_local_bookmark(bookmark)
        .as_normal()
        .cloned()
    else {
        return Err(anyhow!("bookmark '{branch_name}' does not exist"));
    };

    let commit = context
        .repo
        .store()
        .get_commit(&commit_id)
        .with_context(|| format!("failed to load bookmark head for '{branch_name}'"))?;

    let mut tx = context.repo.start_transaction();
    let rewritten = tx
        .repo_mut()
        .rewrite_commit(&commit)
        .set_description(description)
        .write()
        .with_context(|| format!("failed to rewrite bookmark head for '{branch_name}'"))?;
    tx.repo_mut().set_local_bookmark_target(
        bookmark,
        RefTarget::normal(rewritten.id().clone()),
    );
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after rewriting bookmark head")?;

    let repo = tx
        .commit(format!("describe bookmark {branch_name}"))
        .context("failed to finalize bookmark description update")?;
    persist_working_copy_state(context, repo, "after bookmark describe")
}

pub(super) fn abandon_bookmark_head(context: &mut RepoContext, branch_name: &str) -> Result<()> {
    ensure_bookmark_is_checked_out(context, branch_name)?;

    let bookmark = RefName::new(branch_name);
    let Some(commit_id) = context
        .repo
        .view()
        .get_local_bookmark(bookmark)
        .as_normal()
        .cloned()
    else {
        return Err(anyhow!("bookmark '{branch_name}' does not exist"));
    };

    let commit = context
        .repo
        .store()
        .get_commit(&commit_id)
        .with_context(|| format!("failed to load bookmark head for '{branch_name}'"))?;
    if commit.id() == context.repo.store().root_commit_id() {
        return Err(anyhow!("cannot abandon the root revision"));
    }

    let mut tx = context.repo.start_transaction();
    tx.repo_mut().record_abandoned_commit(&commit);
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after abandoning bookmark head")?;

    let repo = tx
        .commit(format!("abandon bookmark {branch_name} head"))
        .context("failed to finalize bookmark head abandon")?;
    persist_working_copy_state(context, repo, "after bookmark head abandon")
}

pub(super) fn squash_bookmark_head_into_parent(
    context: &mut RepoContext,
    branch_name: &str,
) -> Result<()> {
    ensure_bookmark_is_checked_out(context, branch_name)?;

    let bookmark = RefName::new(branch_name);
    let Some(commit_id) = context
        .repo
        .view()
        .get_local_bookmark(bookmark)
        .as_normal()
        .cloned()
    else {
        return Err(anyhow!("bookmark '{branch_name}' does not exist"));
    };

    let source_commit = context
        .repo
        .store()
        .get_commit(&commit_id)
        .with_context(|| format!("failed to load bookmark head for '{branch_name}'"))?;
    if source_commit.parent_ids().len() != 1 {
        return Err(anyhow!(
            "cannot squash bookmark '{branch_name}' tip because it has multiple parents"
        ));
    }

    let Some(parent_id) = source_commit.parent_ids().first().cloned() else {
        return Err(anyhow!("cannot squash a root revision"));
    };
    if parent_id == *context.repo.store().root_commit_id() {
        return Err(anyhow!(
            "cannot squash bookmark '{branch_name}' tip into the root revision"
        ));
    }

    let destination_commit = context
        .repo
        .store()
        .get_commit(&parent_id)
        .with_context(|| format!("failed to load parent revision for '{branch_name}'"))?;
    let source_selection = CommitWithSelection {
        parent_tree: source_commit.parent_tree(context.repo.as_ref())?,
        selected_tree: source_commit.tree(),
        commit: source_commit,
    };

    let mut tx = context.repo.start_transaction();
    let squashed = squash_commits(
        tx.repo_mut(),
        &[source_selection],
        &destination_commit,
        false,
    )
    .with_context(|| format!("failed to squash bookmark '{branch_name}' head into parent"))?
    .ok_or_else(|| anyhow!("no revision changes available to squash"))?;

    squashed
        .commit_builder
        .write()
        .with_context(|| format!("failed to write squashed parent for '{branch_name}'"))?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after squash")?;

    let repo = tx
        .commit(format!("squash bookmark {branch_name} head into parent"))
        .context("failed to finalize bookmark squash")?;
    persist_working_copy_state(context, repo, "after bookmark head squash")
}

pub(super) fn reorder_bookmark_tip_older(
    context: &mut RepoContext,
    branch_name: &str,
) -> Result<()> {
    ensure_bookmark_is_checked_out(context, branch_name)?;

    let revisions = list_bookmark_revisions_from_context(context, branch_name, 3)?;
    let root_revision_id = context.repo.store().root_commit_id().hex();
    let non_root_revisions = revisions
        .iter()
        .filter(|revision| revision.id != root_revision_id)
        .count();
    if non_root_revisions < 2 {
        return Err(anyhow!(
            "need at least two revisions to reorder the bookmark stack"
        ));
    }

    let Some(tip_commit_id) = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name))
        .as_normal()
        .cloned()
    else {
        return Err(anyhow!("bookmark '{branch_name}' does not exist"));
    };
    let tip_revision_id = tip_commit_id.hex();
    let tip_commit = context
        .repo
        .store()
        .get_commit(&tip_commit_id)
        .with_context(|| format!("failed to load tip revision '{tip_revision_id}' for reorder"))?;
    if tip_commit.parent_ids().len() != 1 {
        return Err(anyhow!(
            "cannot reorder bookmark '{branch_name}' tip because it has multiple parents"
        ));
    }
    let anchor_revision = revisions
        .get(2)
        .map(|revision| revision.id.clone())
        .unwrap_or_else(|| context.repo.store().root_commit_id().hex());
    let Some(anchor_commit_id) = jj_lib::backend::CommitId::try_from_hex(anchor_revision.as_str())
    else {
        return Err(anyhow!(
            "failed to resolve reorder anchor revision '{anchor_revision}'"
        ));
    };
    let anchor_child_ids = direct_child_revision_ids(context.repo.as_ref(), &anchor_commit_id)
        .context("failed to resolve reorder anchor descendants")?;

    let mut tx = context.repo.start_transaction();
    let location = MoveCommitsLocation {
        new_parent_ids: vec![anchor_commit_id],
        new_child_ids: anchor_child_ids,
        target: MoveCommitsTarget::Commits(vec![tip_commit_id]),
    };
    move_commits(tx.repo_mut(), &location, &RebaseOptions::default())
        .context("failed to reorder bookmark stack")?;
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after bookmark reorder")?;
    let repo = tx
        .commit(format!("reorder bookmark {branch_name} tip older"))
        .context("failed to finalize bookmark reorder")?;
    persist_working_copy_state(context, repo, "after bookmark tip reorder")?;
    move_bookmark_to_parent_of_working_copy(context, branch_name)?
        .then_some(())
        .ok_or_else(|| anyhow!("bookmark '{branch_name}' no longer exists after reorder"))?;
    Ok(())
}

fn direct_child_revision_ids(
    repo: &ReadonlyRepo,
    commit_id: &jj_lib::backend::CommitId,
) -> Result<Vec<jj_lib::backend::CommitId>> {
    let revset = RevsetExpression::commits(vec![commit_id.clone()])
        .children()
        .evaluate(repo)
        .context("failed to evaluate child revision revset")?;
    let mut child_ids = Vec::new();
    for child_id in revset.iter() {
        child_ids.push(child_id.context("failed to resolve child revision id")?);
    }
    Ok(child_ids)
}

pub(super) fn push_bookmark(context: &mut RepoContext, branch_name: &str) -> Result<()> {
    ensure_bookmark_target_is_not_working_copy_commit(context, branch_name)?;
    ensure_bookmark_tip_identity(context, branch_name)?;
    let remote_name = resolve_push_remote_name(context, branch_name)?;
    let remote = RemoteName::new(remote_name.as_str());
    ensure_remote_bookmark_is_tracked(context, branch_name, remote, remote_name.as_str())?;

    let maybe_targets = context
        .repo
        .view()
        .local_remote_bookmarks(remote)
        .find(|(name, _)| name.as_str() == branch_name)
        .map(|(_, targets)| targets);

    let targets = maybe_targets
        .ok_or_else(|| anyhow!("bookmark '{branch_name}' does not exist in this repository"))?;

    let push_action = classify_bookmark_push_action(targets);
    let update = match push_action {
        BookmarkPushAction::Update(update) => update,
        BookmarkPushAction::AlreadyMatches => return Ok(()),
        BookmarkPushAction::LocalConflicted => {
            return Err(anyhow!(
                "bookmark '{branch_name}' is conflicted locally and cannot be pushed"
            ));
        }
        BookmarkPushAction::RemoteConflicted => {
            return Err(anyhow!(
                "remote tracking state for bookmark '{branch_name}' is conflicted"
            ));
        }
        BookmarkPushAction::RemoteUntracked => {
            return Err(anyhow!(
                "bookmark '{branch_name}' has an untracked remote ref after tracking attempt"
            ));
        }
    };

    let push_targets = git::GitBranchPushTargets {
        branch_updates: vec![(RefNameBuf::from(branch_name), update)],
    };
    let subprocess_options = GitSubprocessOptions::from_settings(&context.settings)
        .context("failed to resolve git subprocess settings")?;

    let mut tx = context.repo.start_transaction();
    let mut callback = NoopGitSubprocessCallback;
    git::push_branches(
        tx.repo_mut(),
        subprocess_options,
        remote,
        &push_targets,
        &mut callback,
    )
    .with_context(|| {
        format!("failed to push bookmark '{branch_name}' to remote '{remote_name}'")
    })?;

    let repo = tx
        .commit(format!("push bookmark {branch_name}"))
        .context("failed to finalize push operation")?;
    persist_working_copy_state(context, repo, "after push")
}

fn ensure_bookmark_target_is_not_working_copy_commit(
    context: &RepoContext,
    branch_name: &str,
) -> Result<()> {
    let Some(target_commit_id) = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name))
        .as_normal()
    else {
        return Ok(());
    };

    let wc_commit = current_wc_commit(context)?;
    if wc_commit.id() != target_commit_id {
        return Ok(());
    }

    Err(anyhow!(
        "cannot push bookmark '{branch_name}' because it points to the mutable working-copy \
revision (@); commit or retarget it to a committed revision first"
    ))
}

fn ensure_bookmark_tip_identity(context: &mut RepoContext, branch_name: &str) -> Result<()> {
    let target = context
        .repo
        .view()
        .get_local_bookmark(RefName::new(branch_name))
        .as_normal()
        .cloned();
    let Some(commit_id) = target else {
        return Ok(());
    };

    let commit = context
        .repo
        .store()
        .get_commit(&commit_id)
        .with_context(|| format!("failed to load bookmark target for '{branch_name}'"))?;
    if !commit_has_missing_identity(&commit) {
        return Ok(());
    }

    let signature = context.settings.signature();
    if signature.name.trim().is_empty() || signature.email.trim().is_empty() {
        return Err(anyhow!(
            "bookmark '{branch_name}' has a commit with missing author/committer metadata. \
Set user.name/user.email (JJ or Git config) and try again."
        ));
    }

    let mut tx = context.repo.start_transaction();
    let rewritten = tx
        .repo_mut()
        .rewrite_commit(&commit)
        .set_author(signature.clone())
        .set_committer(signature)
        .write()
        .with_context(|| format!("failed to rewrite bookmark commit metadata for '{branch_name}'"))?;
    tx.repo_mut().set_local_bookmark_target(
        RefName::new(branch_name),
        RefTarget::normal(rewritten.id().clone()),
    );
    tx.repo_mut()
        .rebase_descendants()
        .context("failed to rebase descendants after rewriting bookmark metadata")?;

    let repo = tx
        .commit(format!("update metadata for bookmark {branch_name}"))
        .context("failed to finalize bookmark metadata update")?;
    persist_working_copy_state(context, repo, "after rewriting bookmark metadata")
}

fn commit_has_missing_identity(commit: &Commit) -> bool {
    signature_has_missing_identity(commit.author()) || signature_has_missing_identity(commit.committer())
}

fn signature_has_missing_identity(signature: &jj_lib::backend::Signature) -> bool {
    signature.name.trim().is_empty() || signature.email.trim().is_empty()
}

pub(super) fn sync_bookmark_from_remote(
    context: &mut RepoContext,
    branch_name: &str,
) -> Result<()> {
    if !load_changed_files_from_context(context)?.is_empty() {
        return Err(anyhow!(
            "cannot sync while the working copy has uncommitted changes"
        ));
    }

    let remote_name = resolve_push_remote_name(context, branch_name)?;
    let remote = RemoteName::new(remote_name.as_str());
    let subprocess_options = GitSubprocessOptions::from_settings(&context.settings)
        .context("failed to resolve git subprocess settings")?;
    let import_options = git_import_options_from_settings(&context.settings)?;
    let fetch_refspecs = git::expand_fetch_refspecs(
        remote,
        git::GitFetchRefExpression {
            bookmark: StringExpression::exact(branch_name),
            tag: StringExpression::none(),
        },
    )
    .with_context(|| format!("failed to prepare fetch refspecs for bookmark '{branch_name}'"))?;

    let mut tx = context.repo.start_transaction();
    {
        let mut fetcher = git::GitFetch::new(tx.repo_mut(), subprocess_options, &import_options)
            .context("failed to initialize Git fetch operation")?;
        let mut callback = NoopGitSubprocessCallback;
        fetcher
            .fetch(remote, fetch_refspecs, &mut callback, None, None)
            .with_context(|| {
                format!("failed to fetch bookmark '{branch_name}' from remote '{remote_name}'")
            })?;
        fetcher
            .import_refs()
            .context("failed to import fetched refs into JJ view")?;
    }

    let repo = tx
        .commit(format!("sync bookmark {branch_name} from {remote_name}"))
        .context("failed to finalize sync operation")?;
    persist_working_copy_state(context, repo, "after sync")?;

    ensure_remote_bookmark_is_tracked(context, branch_name, remote, remote_name.as_str())?;
    checkout_existing_bookmark(context, branch_name)
        .with_context(|| format!("failed to refresh working copy for '{branch_name}'"))?;

    Ok(())
}

fn git_import_options_from_settings(settings: &UserSettings) -> Result<git::GitImportOptions> {
    let auto_local_bookmark = settings
        .get_bool("git.auto-local-bookmark")
        .context("failed to read git.auto-local-bookmark setting")?;
    let abandon_unreachable_commits = settings
        .get_bool("git.abandon-unreachable-commits")
        .context("failed to read git.abandon-unreachable-commits setting")?;

    Ok(git::GitImportOptions {
        auto_local_bookmark,
        abandon_unreachable_commits,
        remote_auto_track_bookmarks: HashMap::new(),
    })
}

fn ensure_remote_bookmark_is_tracked(
    context: &mut RepoContext,
    branch_name: &str,
    remote: &RemoteName,
    remote_name: &str,
) -> Result<()> {
    let Some((_, targets)) = context
        .repo
        .view()
        .local_remote_bookmarks(remote)
        .find(|(name, _)| name.as_str() == branch_name)
    else {
        return Ok(());
    };
    if targets.remote_ref.is_tracked() {
        return Ok(());
    }

    let symbol = RefName::new(branch_name).to_remote_symbol(remote);
    let mut tx = context.repo.start_transaction();
    tx.repo_mut()
        .track_remote_bookmark(symbol)
        .with_context(|| {
            format!(
                "failed to track remote bookmark '{}@{}' before operation",
                branch_name, remote_name
            )
        })?;

    let repo = tx
        .commit(format!("track remote bookmark {branch_name}@{remote_name}"))
        .context("failed to finalize remote bookmark tracking operation")?;
    persist_working_copy_state(context, repo, "after tracking remote bookmark")
}

fn resolve_push_remote_name(context: &RepoContext, branch_name: &str) -> Result<String> {
    let view = context.repo.view();
    let mut first_present_remote = None;

    for (remote, _) in view.remote_views() {
        if remote == REMOTE_NAME_FOR_LOCAL_GIT_REPO {
            continue;
        }

        let Some((_, targets)) = view
            .local_remote_bookmarks(remote)
            .find(|(name, _)| name.as_str() == branch_name)
        else {
            continue;
        };
        if !targets.remote_ref.is_present() {
            continue;
        }

        if targets.remote_ref.is_tracked() {
            return Ok(remote.as_str().to_string());
        }
        if first_present_remote.is_none() {
            first_present_remote = Some(remote.as_str().to_string());
        }
    }

    if let Some(remote_name) = first_present_remote {
        return Ok(remote_name);
    }

    if view
        .remote_views()
        .any(|(remote, _)| remote.as_str() == "origin")
    {
        return Ok("origin".to_string());
    }

    if let Some((remote, _)) = view
        .remote_views()
        .find(|(remote, _)| *remote != REMOTE_NAME_FOR_LOCAL_GIT_REPO)
    {
        return Ok(remote.as_str().to_string());
    }

    Err(anyhow!("no Git remote configured for push"))
}

pub(super) fn bookmark_review_url(
    context: &RepoContext,
    branch_name: &str,
    provider_mappings: &[crate::config::ReviewProviderMapping],
) -> Result<Option<String>> {
    let remote_name = resolve_push_remote_name(context, branch_name)?;
    let Some(remote_url) = resolve_remote_url_from_jj_lib(context, remote_name.as_str())?
    else {
        return Ok(None);
    };

    Ok(review_url_for_remote(
        remote_url.as_str(),
        branch_name,
        provider_mappings,
    ))
}

fn resolve_remote_url_from_jj_lib(context: &RepoContext, remote_name: &str) -> Result<Option<String>> {
    let git_repo = git::get_git_repo(context.repo.store())
        .context("failed to load backing Git repository for review URL")?;
    let Some(remote_result) = git_repo.try_find_remote(remote_name) else {
        return Ok(None);
    };
    let remote = remote_result.with_context(|| {
        format!("failed to load configured remote '{remote_name}' for review URL")
    })?;
    Ok(remote
        .url(gix::remote::Direction::Fetch)
        .map(|url| url.to_string()))
}

fn review_url_for_remote(
    remote_url: &str,
    branch_name: &str,
    provider_mappings: &[crate::config::ReviewProviderMapping],
) -> Option<String> {
    let (host, base_url) = normalized_remote_base_url(remote_url)?;
    let provider = review_provider_from_host(host.as_str(), provider_mappings)?;
    let encoded_branch = percent_encode(branch_name);
    match provider {
        crate::config::ReviewProviderKind::GitLab => Some(format!(
            "{base_url}/-/merge_requests/new?merge_request[source_branch]={encoded_branch}"
        )),
        crate::config::ReviewProviderKind::GitHub => {
            Some(format!("{base_url}/compare/{encoded_branch}?expand=1"))
        }
    }
}

fn normalized_remote_base_url(remote_url: &str) -> Option<(String, String)> {
    if let Some((authority, path)) = remote_url
        .strip_prefix("https://")
        .or_else(|| remote_url.strip_prefix("http://"))
        .and_then(split_authority_and_path)
    {
        let host = sanitize_host(authority)?;
        let clean_path = trim_remote_path(path);
        return Some((host.clone(), format!("https://{host}/{}", clean_path)));
    }

    if let Some(stripped) = remote_url.strip_prefix("ssh://") {
        let after_user = stripped
            .split_once('@')
            .map_or(stripped, |(_, remainder)| remainder);
        let (authority, path) = split_authority_and_path(after_user)?;
        let host = sanitize_host(authority)?;
        let clean_path = trim_remote_path(path);
        return Some((host.clone(), format!("https://{host}/{}", clean_path)));
    }

    if let Some((authority, path)) = split_scp_like(remote_url) {
        let host = sanitize_host(authority)?;
        let clean_path = trim_remote_path(path);
        return Some((host.clone(), format!("https://{host}/{}", clean_path)));
    }

    None
}

fn split_authority_and_path(value: &str) -> Option<(&str, &str)> {
    value.split_once('/')
}

fn split_scp_like(remote_url: &str) -> Option<(&str, &str)> {
    if remote_url.contains("://") {
        return None;
    }
    let (authority, path) = remote_url.split_once(':')?;
    if authority.is_empty() || path.is_empty() {
        return None;
    }
    if authority.contains('/') {
        return None;
    }
    if authority.len() == 1 && authority.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }
    Some((authority, path))
}

fn sanitize_host(authority: &str) -> Option<String> {
    let without_user = authority.rsplit('@').next().unwrap_or(authority);
    if without_user.is_empty() {
        return None;
    }
    let without_port = if without_user.starts_with('[') {
        without_user
            .split_once(']')
            .map(|(host, _)| format!("{host}]"))
            .filter(|host| !host.is_empty())?
    } else {
        without_user.split(':').next()?.to_string()
    };
    (!without_port.is_empty()).then_some(without_port)
}

fn trim_remote_path(path: &str) -> String {
    path.trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string()
}

fn review_provider_from_host(
    host: &str,
    provider_mappings: &[crate::config::ReviewProviderMapping],
) -> Option<crate::config::ReviewProviderKind> {
    if let Some(provider) = review_provider_from_mappings(host, provider_mappings) {
        return Some(provider);
    }

    if host.contains("gitlab") {
        Some(crate::config::ReviewProviderKind::GitLab)
    } else if host.contains("github") {
        Some(crate::config::ReviewProviderKind::GitHub)
    } else {
        None
    }
}

fn review_provider_from_mappings(
    host: &str,
    provider_mappings: &[crate::config::ReviewProviderMapping],
) -> Option<crate::config::ReviewProviderKind> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }

    provider_mappings
        .iter()
        .find(|mapping| host_matches_provider_pattern(host.as_str(), mapping.host.as_str()))
        .map(|mapping| mapping.provider)
}

fn host_matches_provider_pattern(host: &str, raw_pattern: &str) -> bool {
    let pattern = raw_pattern.trim().trim_end_matches('.').to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        if host == suffix {
            return true;
        }
        if host.len() <= suffix.len() || !host.ends_with(suffix) {
            return false;
        }
        let separator_ix = host.len().saturating_sub(suffix.len() + 1);
        return host.as_bytes().get(separator_ix) == Some(&b'.');
    }
    host == pattern
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(format!("%{byte:02X}").as_str());
        }
    }
    encoded
}
