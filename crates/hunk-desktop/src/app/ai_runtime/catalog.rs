#[derive(Debug, Clone)]
pub struct AiWorkspaceThreadCatalog {
    pub workspace_key: String,
    pub state_snapshot: AiState,
    pub active_thread_id: Option<String>,
}

pub fn load_ai_workspace_thread_catalog(
    workspace_root: PathBuf,
    codex_executable: PathBuf,
    codex_home: PathBuf,
) -> Result<Option<AiWorkspaceThreadCatalog>, CodexIntegrationError> {
    if !workspace_root_exists_for_catalog(workspace_root.as_path()) {
        return Ok(None);
    }

    std::fs::create_dir_all(&codex_home).map_err(CodexIntegrationError::HostProcessIo)?;

    load_ai_workspace_thread_catalog_with_session(
        workspace_root.as_path(),
        &mut EmbeddedAppServerClient::start(EmbeddedAppServerClientStartArgs::new(
            codex_home,
            workspace_root.clone(),
            codex_executable,
            "hunk-desktop".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ))?,
    )
}

pub(crate) fn archive_ai_thread_for_workspace(
    workspace_root: &std::path::Path,
    thread_id: &str,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
) -> Result<(), CodexIntegrationError> {
    std::fs::create_dir_all(codex_home).map_err(CodexIntegrationError::HostProcessIo)?;

    archive_ai_thread_for_workspace_with_session(
        workspace_root,
        thread_id,
        &mut EmbeddedAppServerClient::start(EmbeddedAppServerClientStartArgs::new(
            codex_home.to_path_buf(),
            workspace_root.to_path_buf(),
            codex_executable.to_path_buf(),
            "hunk-desktop".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ))?,
    )
}

fn workspace_thread_exists(
    service: &mut ThreadService,
    session: &mut impl AppServerClient,
    thread_id: &str,
) -> Result<bool, CodexIntegrationError> {
    let response = service.list_threads(session, None, Some(200), DEFAULT_REQUEST_TIMEOUT)?;
    Ok(response.data.iter().any(|thread| thread.id == thread_id))
}

fn workspace_thread_is_archived(service: &ThreadService, thread_id: &str) -> bool {
    service
        .state()
        .threads
        .get(thread_id)
        .is_some_and(|thread| thread.status == ThreadLifecycleStatus::Archived)
}

fn archive_ai_thread_for_workspace_with_session(
    workspace_root: &std::path::Path,
    thread_id: &str,
    session: &mut EmbeddedAppServerClient,
) -> Result<(), CodexIntegrationError> {
    let mut service = ThreadService::new(workspace_root.to_path_buf());
    if !workspace_thread_exists(&mut service, session, thread_id)? {
        return Ok(());
    }
    if workspace_thread_is_archived(&service, thread_id) {
        return Ok(());
    }

    match service.archive_thread(session, thread_id.to_string(), DEFAULT_REQUEST_TIMEOUT) {
        Ok(_) => Ok(()),
        Err(error) if is_missing_thread_rollout_error(&error) => {
            let thread_exists = workspace_thread_exists(&mut service, session, thread_id)?;
            if !thread_exists || workspace_thread_is_archived(&service, thread_id) {
                Ok(())
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    }
}

fn load_ai_workspace_thread_catalog_with_session(
    workspace_root: &std::path::Path,
    session: &mut EmbeddedAppServerClient,
) -> Result<Option<AiWorkspaceThreadCatalog>, CodexIntegrationError> {
    let workspace_root = workspace_root.to_path_buf();
    let mut service = ThreadService::new(workspace_root.clone());
    let response = service.list_threads(session, None, Some(200), DEFAULT_REQUEST_TIMEOUT)?;
    let workspace_key = workspace_root.to_string_lossy().to_string();

    if service.active_thread_for_workspace().is_none()
        && let Some(first_thread) = response.data.first()
    {
        service
            .state_mut()
            .set_active_thread_for_cwd(workspace_key.clone(), first_thread.id.clone());
    }

    Ok(Some(AiWorkspaceThreadCatalog {
        workspace_key,
        state_snapshot: service.state().clone(),
        active_thread_id: service.active_thread_for_workspace().map(ToOwned::to_owned),
    }))
}

fn workspace_root_exists_for_catalog(workspace_root: &std::path::Path) -> bool {
    workspace_root.exists()
}

#[cfg(test)]
mod tests {
    use super::workspace_root_exists_for_catalog;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_workspace_root_is_skipped_from_catalog_refresh() {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let missing = std::env::temp_dir().join(format!(
            "hunk-ai-missing-workspace-{unique_suffix}"
        ));

        assert!(!missing.exists());
        assert!(!workspace_root_exists_for_catalog(missing.as_path()));
    }
}
