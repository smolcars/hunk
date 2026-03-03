const UNDO_OP_DESC_PREFIX: &str = "undo: restore to operation ";
const REDO_OP_DESC_PREFIX: &str = "redo: restore to operation ";

pub(super) fn can_undo_operation(context: &RepoContext) -> Result<bool> {
    Ok(has_single_parent_operation(context.repo.operation()))
}

pub(super) fn can_redo_operation(context: &RepoContext) -> Result<bool> {
    let operation = resolve_operation_to_redo(context)?;
    Ok(operation
        .metadata()
        .description
        .starts_with(UNDO_OP_DESC_PREFIX))
}

pub(super) fn undo_last_operation(context: &mut RepoContext) -> Result<()> {
    let operation_to_restore = single_parent_operation(context.repo.operation())?;
    restore_operation(
        context,
        &operation_to_restore,
        format!(
            "{UNDO_OP_DESC_PREFIX}{}",
            operation_to_restore.id().hex()
        ),
        "failed to finalize undo operation",
    )
}

pub(super) fn redo_last_operation(context: &mut RepoContext) -> Result<()> {
    let operation_to_redo = resolve_operation_to_redo(context)?;
    if !operation_to_redo
        .metadata()
        .description
        .starts_with(UNDO_OP_DESC_PREFIX)
    {
        return Err(anyhow!("Nothing to redo"));
    }

    let mut operation_to_restore = single_parent_operation(&operation_to_redo)?;
    if let Some(original_operation_id) =
        operation_id_from_prefix(operation_to_restore.metadata().description.as_str(), REDO_OP_DESC_PREFIX)?
    {
        operation_to_restore = context
            .repo
            .loader()
            .load_operation(&original_operation_id)
            .context("failed to load original operation for redo")?;
    }

    restore_operation(
        context,
        &operation_to_restore,
        format!(
            "{REDO_OP_DESC_PREFIX}{}",
            operation_to_restore.id().hex()
        ),
        "failed to finalize redo operation",
    )
}

fn resolve_operation_to_redo(context: &RepoContext) -> Result<jj_lib::operation::Operation> {
    let mut operation = context.repo.operation().clone();
    if let Some(restored_operation_id) =
        operation_id_from_prefix(operation.metadata().description.as_str(), REDO_OP_DESC_PREFIX)?
    {
        operation = context
            .repo
            .loader()
            .load_operation(&restored_operation_id)
            .context("failed to load restored operation in redo stack")?;
    }
    Ok(operation)
}

fn single_parent_operation(operation: &jj_lib::operation::Operation) -> Result<jj_lib::operation::Operation> {
    let mut parents = operation.parents();
    let parent = parents
        .next()
        .ok_or_else(|| anyhow!("Cannot restore root operation from operation history"))?
        .context("failed to load parent operation from operation history")?;
    if parents.next().is_some() {
        return Err(anyhow!(
            "cannot restore a merge operation from operation history"
        ));
    }
    Ok(parent)
}

fn has_single_parent_operation(operation: &jj_lib::operation::Operation) -> bool {
    let mut parents = operation.parents();
    if parents.next().is_none() {
        return false;
    }
    parents.next().is_none()
}

fn restore_operation(
    context: &mut RepoContext,
    operation_to_restore: &jj_lib::operation::Operation,
    description: String,
    finalize_error: &'static str,
) -> Result<()> {
    let workspace_name = context.workspace.workspace_name().to_owned();
    let previous_wc_commit_id = context.repo.view().get_wc_commit_id(&workspace_name).cloned();
    let restored_view = view_with_restored_repo_and_remote_tracking(
        operation_to_restore.view()?.store_view(),
        context.repo.view().store_view(),
    );

    let mut tx = context.repo.start_transaction();
    tx.repo_mut().set_view(restored_view);
    let repo = tx.commit(description).context(finalize_error)?;

    let new_wc_commit_id = repo.view().get_wc_commit_id(&workspace_name).cloned();
    let mut locked_workspace = context
        .workspace
        .start_working_copy_mutation()
        .context("failed to lock working copy for operation history restore")?;
    if previous_wc_commit_id != new_wc_commit_id
        && let Some(commit_id) = new_wc_commit_id
    {
        let new_wc_commit = repo
            .store()
            .get_commit(&commit_id)
            .context("failed to load working-copy commit after operation history restore")?;
        block_on(locked_workspace.locked_wc().check_out(&new_wc_commit))
            .context("failed to update working-copy files after operation history restore")?;
    }
    locked_workspace
        .finish(repo.op_id().clone())
        .context("failed to persist working-copy state after operation history restore")?;

    context.repo = repo;
    Ok(())
}

fn operation_id_from_prefix(
    description: &str,
    prefix: &str,
) -> Result<Option<jj_lib::op_store::OperationId>> {
    let Some(operation_id_hex) = description.strip_prefix(prefix) else {
        return Ok(None);
    };
    let Some(operation_id) = jj_lib::op_store::OperationId::try_from_hex(operation_id_hex) else {
        return Err(anyhow!(
            "failed to parse operation id '{operation_id_hex}' in operation history"
        ));
    };
    Ok(Some(operation_id))
}

fn view_with_restored_repo_and_remote_tracking(
    restored_view: &jj_lib::op_store::View,
    current_view: &jj_lib::op_store::View,
) -> jj_lib::op_store::View {
    jj_lib::op_store::View {
        head_ids: restored_view.head_ids.clone(),
        local_bookmarks: restored_view.local_bookmarks.clone(),
        local_tags: restored_view.local_tags.clone(),
        remote_views: restored_view.remote_views.clone(),
        git_refs: current_view.git_refs.clone(),
        git_head: current_view.git_head.clone(),
        wc_commit_ids: restored_view.wc_commit_ids.clone(),
    }
}
