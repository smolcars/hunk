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

    let mut last_error = None;
    for transport_kind in AiAppServerTransportPreference::from_env().bootstrap_candidates() {
        match load_ai_workspace_thread_catalog_with_transport(
            workspace_root.as_path(),
            codex_executable.as_path(),
            codex_home.as_path(),
            transport_kind,
        ) {
            Ok(catalog) => return Ok(catalog),
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or(CodexIntegrationError::WebSocketTransport(
        "unable to load workspace AI catalog from any configured transport".to_string(),
    )))
}

pub(crate) fn archive_ai_thread_for_workspace(
    workspace_root: &std::path::Path,
    thread_id: &str,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
) -> Result<(), CodexIntegrationError> {
    std::fs::create_dir_all(codex_home).map_err(CodexIntegrationError::HostProcessIo)?;

    let mut last_error = None;
    for transport_kind in AiAppServerTransportPreference::from_env().bootstrap_candidates() {
        match archive_ai_thread_for_workspace_with_transport(
            workspace_root,
            thread_id,
            codex_executable,
            codex_home,
            transport_kind,
        ) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or(CodexIntegrationError::WebSocketTransport(
        "unable to archive workspace AI thread from any configured transport".to_string(),
    )))
}

fn archive_ai_thread_for_workspace_with_transport(
    workspace_root: &std::path::Path,
    thread_id: &str,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
    transport_kind: AppServerTransportKind,
) -> Result<(), CodexIntegrationError> {
    match transport_kind {
        AppServerTransportKind::Embedded => {
            let mut session = ManagedAppServerClient::Embedded(EmbeddedAppServerClient::start(
                EmbeddedAppServerClientStartArgs::new(
                    codex_home.to_path_buf(),
                    workspace_root.to_path_buf(),
                    codex_executable.to_path_buf(),
                    "hunk-desktop".to_string(),
                    env!("CARGO_PKG_VERSION").to_string(),
                ),
            )?);
            archive_ai_thread_for_workspace_with_session(workspace_root, thread_id, &mut session)
        }
        AppServerTransportKind::RemoteBundled => archive_ai_thread_for_workspace_remote(
            workspace_root,
            thread_id,
            codex_executable,
            codex_home,
        ),
    }
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

fn load_ai_workspace_thread_catalog_with_transport(
    workspace_root: &std::path::Path,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
    transport_kind: AppServerTransportKind,
) -> Result<Option<AiWorkspaceThreadCatalog>, CodexIntegrationError> {
    match transport_kind {
        AppServerTransportKind::Embedded => {
            let mut session = ManagedAppServerClient::Embedded(EmbeddedAppServerClient::start(
                EmbeddedAppServerClientStartArgs::new(
                    codex_home.to_path_buf(),
                    workspace_root.to_path_buf(),
                    codex_executable.to_path_buf(),
                    "hunk-desktop".to_string(),
                    env!("CARGO_PKG_VERSION").to_string(),
                ),
            )?);
            load_ai_workspace_thread_catalog_with_session(workspace_root, &mut session)
        }
        AppServerTransportKind::RemoteBundled => {
            load_ai_workspace_thread_catalog_remote(workspace_root, codex_executable, codex_home)
        }
    }
}

fn archive_ai_thread_for_workspace_with_session(
    workspace_root: &std::path::Path,
    thread_id: &str,
    session: &mut ManagedAppServerClient,
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

fn archive_ai_thread_for_workspace_remote(
    workspace_root: &std::path::Path,
    thread_id: &str,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
) -> Result<(), CodexIntegrationError> {
    let mut last_retryable_error = None;
    for _attempt in 0..HOST_BOOTSTRAP_MAX_ATTEMPTS {
        let port = allocate_loopback_port();
        let host_config = HostConfig::codex_app_server(
            codex_executable.to_path_buf(),
            shared_ai_host_working_directory(workspace_root),
            codex_home.to_path_buf(),
            port,
        );
        let host = SharedHostLease::acquire(host_config, HOST_START_TIMEOUT)?;
        let mut session = ManagedAppServerClient::Remote(RemoteAppServerClient::connect_loopback(
            host.port(),
            DEFAULT_REQUEST_TIMEOUT,
        )?);

        match archive_ai_thread_for_workspace_with_session(workspace_root, thread_id, &mut session)
        {
            Ok(()) => return Ok(()),
            Err(error) if should_retry_bootstrap_with_new_port(&error) => {
                last_retryable_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_retryable_error.unwrap_or(CodexIntegrationError::HostStartupTimedOut {
        port: 0,
        timeout_ms: HOST_START_TIMEOUT
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    }))
}

fn load_ai_workspace_thread_catalog_with_session(
    workspace_root: &std::path::Path,
    session: &mut ManagedAppServerClient,
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

fn load_ai_workspace_thread_catalog_remote(
    workspace_root: &std::path::Path,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
) -> Result<Option<AiWorkspaceThreadCatalog>, CodexIntegrationError> {
    let mut last_retryable_error = None;
    for _attempt in 0..HOST_BOOTSTRAP_MAX_ATTEMPTS {
        let port = allocate_loopback_port();
        let host_config = HostConfig::codex_app_server(
            codex_executable.to_path_buf(),
            shared_ai_host_working_directory(workspace_root),
            codex_home.to_path_buf(),
            port,
        );
        let host = SharedHostLease::acquire(host_config, HOST_START_TIMEOUT)?;
        let mut session = ManagedAppServerClient::Remote(RemoteAppServerClient::connect_loopback(
            host.port(),
            DEFAULT_REQUEST_TIMEOUT,
        )?);

        match load_ai_workspace_thread_catalog_with_session(workspace_root, &mut session) {
            Ok(catalog) => return Ok(catalog),
            Err(error) if should_retry_bootstrap_with_new_port(&error) => {
                last_retryable_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_retryable_error.unwrap_or(CodexIntegrationError::HostStartupTimedOut {
        port: 0,
        timeout_ms: HOST_START_TIMEOUT
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
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
        let temp_dir = std::env::temp_dir().join(format!(
            "hunk-ai-runtime-catalog-test-{unique_suffix}"
        ));
        let existing = temp_dir.join("workspace");
        std::fs::create_dir_all(&existing).expect("workspace dir should exist");
        let missing = temp_dir.join("missing-workspace");

        assert!(workspace_root_exists_for_catalog(existing.as_path()));
        assert!(!workspace_root_exists_for_catalog(missing.as_path()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
