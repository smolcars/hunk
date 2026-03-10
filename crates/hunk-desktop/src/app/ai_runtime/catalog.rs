#[derive(Debug, Clone)]
pub struct AiWorkspaceThreadCatalog {
    pub workspace_key: String,
    pub state_snapshot: AiState,
    pub active_thread_id: Option<String>,
}

pub fn load_ai_workspace_thread_catalogs(
    workspace_roots: Vec<PathBuf>,
    codex_executable: PathBuf,
    codex_home: PathBuf,
) -> Result<Vec<AiWorkspaceThreadCatalog>, CodexIntegrationError> {
    let started_at = Instant::now();
    if workspace_roots.is_empty() {
        return Ok(Vec::new());
    }

    std::fs::create_dir_all(&codex_home).map_err(CodexIntegrationError::HostProcessIo)?;

    let mut last_retryable_error = None;
    for attempt in 0..HOST_BOOTSTRAP_MAX_ATTEMPTS {
        let port = allocate_loopback_port();
        match load_ai_workspace_thread_catalogs_on_port(
            workspace_roots.as_slice(),
            codex_executable.as_path(),
            codex_home.as_path(),
            port,
        ) {
            Ok(catalogs) => {
                let workspace_count = catalogs.len();
                let thread_count = catalogs
                    .iter()
                    .map(|catalog| {
                        catalog
                            .state_snapshot
                            .threads
                            .values()
                            .filter(|thread| thread.status != ThreadLifecycleStatus::Archived)
                            .count()
                    })
                    .sum::<usize>();
                tracing::info!(
                    requested_workspace_count = workspace_roots.len(),
                    workspace_count,
                    thread_count,
                    attempt = attempt + 1,
                    port,
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    "ai instrumentation: repo-wide thread catalog load completed"
                );
                return Ok(catalogs);
            }
            Err(error) if should_retry_bootstrap_with_new_port(&error) => {
                tracing::warn!(
                    requested_workspace_count = workspace_roots.len(),
                    attempt = attempt + 1,
                    port,
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    error = %error,
                    "ai instrumentation: repo-wide thread catalog load retrying on a new port"
                );
                last_retryable_error = Some(error);
            }
            Err(error) => {
                tracing::warn!(
                    requested_workspace_count = workspace_roots.len(),
                    attempt = attempt + 1,
                    port,
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    error = %error,
                    "ai instrumentation: repo-wide thread catalog load failed"
                );
                return Err(error);
            }
        }
    }

    let error = last_retryable_error.unwrap_or(CodexIntegrationError::HostStartupTimedOut {
        port: 0,
        timeout_ms: HOST_START_TIMEOUT
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    });
    tracing::warn!(
        requested_workspace_count = workspace_roots.len(),
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        error = %error,
        "ai instrumentation: repo-wide thread catalog load exhausted all attempts"
    );
    Err(error)
}

pub(crate) fn archive_ai_thread_for_workspace(
    workspace_root: &std::path::Path,
    thread_id: &str,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
) -> Result<(), CodexIntegrationError> {
    std::fs::create_dir_all(codex_home).map_err(CodexIntegrationError::HostProcessIo)?;

    let mut last_retryable_error = None;
    for _attempt in 0..HOST_BOOTSTRAP_MAX_ATTEMPTS {
        let port = allocate_loopback_port();
        match archive_ai_thread_for_workspace_on_port(
            workspace_root,
            thread_id,
            codex_executable,
            codex_home,
            port,
        ) {
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

fn archive_ai_thread_for_workspace_on_port(
    workspace_root: &std::path::Path,
    thread_id: &str,
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
    port: u16,
) -> Result<(), CodexIntegrationError> {
    let host_config = HostConfig::codex_app_server(
        codex_executable.to_path_buf(),
        shared_ai_host_working_directory(workspace_root),
        codex_home.to_path_buf(),
        port,
    );
    let host = SharedHostLease::acquire(host_config, HOST_START_TIMEOUT)?;

    (|| {
        let endpoint = WebSocketEndpoint::loopback(host.port());
        let mut session = JsonRpcSession::connect(&endpoint)?;
        session.initialize(InitializeOptions::default(), DEFAULT_REQUEST_TIMEOUT)?;

        let mut service = ThreadService::new(workspace_root.to_path_buf());
        if !workspace_thread_exists(&mut service, &mut session, thread_id)? {
            return Ok(());
        }
        if workspace_thread_is_archived(&service, thread_id) {
            return Ok(());
        }

        match service.archive_thread(
            &mut session,
            thread_id.to_string(),
            DEFAULT_REQUEST_TIMEOUT,
        ) {
            Ok(_) => Ok(()),
            Err(error) if is_missing_thread_rollout_error(&error) => {
                let thread_exists = workspace_thread_exists(&mut service, &mut session, thread_id)?;
                if !thread_exists || workspace_thread_is_archived(&service, thread_id) {
                    Ok(())
                } else {
                    Err(error)
                }
            }
            Err(error) => Err(error),
        }
    })()
}

fn workspace_thread_exists(
    service: &mut ThreadService,
    session: &mut JsonRpcSession,
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

fn load_ai_workspace_thread_catalogs_on_port(
    workspace_roots: &[PathBuf],
    codex_executable: &std::path::Path,
    codex_home: &std::path::Path,
    port: u16,
) -> Result<Vec<AiWorkspaceThreadCatalog>, CodexIntegrationError> {
    let started_at = Instant::now();
    let host_working_directory = workspace_roots
        .first()
        .cloned()
        .expect("workspace roots should be present");
    let host_config = HostConfig::codex_app_server(
        codex_executable.to_path_buf(),
        shared_ai_host_working_directory(host_working_directory.as_path()),
        codex_home.to_path_buf(),
        port,
    );
    let host_started_at = Instant::now();
    let host = SharedHostLease::acquire(host_config, HOST_START_TIMEOUT)?;
    let host_acquire_elapsed_ms = host_started_at.elapsed().as_millis() as u64;
    let host_pid = host.pid();
    let host_port = host.port();

    (|| {
        let endpoint = WebSocketEndpoint::loopback(host.port());
        let connect_started_at = Instant::now();
        let mut session = JsonRpcSession::connect(&endpoint)?;
        let websocket_connect_elapsed_ms = connect_started_at.elapsed().as_millis() as u64;
        let initialize_started_at = Instant::now();
        session.initialize(InitializeOptions::default(), DEFAULT_REQUEST_TIMEOUT)?;
        let websocket_initialize_elapsed_ms =
            initialize_started_at.elapsed().as_millis() as u64;
        tracing::info!(
            requested_workspace_count = workspace_roots.len(),
            requested_port = port,
            host_port,
            host_pid = ?host_pid,
            host_acquire_elapsed_ms,
            websocket_connect_elapsed_ms,
            websocket_initialize_elapsed_ms,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            "ai instrumentation: repo-wide thread catalog transport ready"
        );
        let mut catalogs = Vec::with_capacity(workspace_roots.len());
        for workspace_root in workspace_roots {
            if !workspace_root_exists_for_catalog(workspace_root.as_path()) {
                continue;
            }
            let mut service = ThreadService::new(workspace_root.clone());
            let workspace_started_at = Instant::now();
            let response = match service.list_threads(
                &mut session,
                None,
                Some(200),
                DEFAULT_REQUEST_TIMEOUT,
            ) {
                Ok(response) => response,
                Err(error) => {
                    tracing::warn!(
                        workspace_root = %workspace_root.display(),
                        elapsed_ms = workspace_started_at.elapsed().as_millis() as u64,
                        error = %error,
                        "ai instrumentation: skipping workspace during thread catalog refresh"
                    );
                    continue;
                }
            };
            let workspace_key = workspace_root.to_string_lossy().to_string();

            if service.active_thread_for_workspace().is_none()
                && let Some(first_thread) = response.data.first()
            {
                service
                    .state_mut()
                    .set_active_thread_for_cwd(workspace_key.clone(), first_thread.id.clone());
            }

            catalogs.push(AiWorkspaceThreadCatalog {
                workspace_key,
                state_snapshot: service.state().clone(),
                active_thread_id: service.active_thread_for_workspace().map(ToOwned::to_owned),
            });
            tracing::debug!(
                workspace_root = %workspace_root.display(),
                thread_count = response.data.len(),
                active_thread_id = ?service.active_thread_for_workspace(),
                elapsed_ms = workspace_started_at.elapsed().as_millis() as u64,
                "ai instrumentation: workspace thread catalog refreshed"
            );
        }

        Ok(catalogs)
    })()
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
