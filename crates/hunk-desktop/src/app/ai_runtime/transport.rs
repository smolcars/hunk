#[derive(Debug, Clone)]
pub struct AiWorkerStartConfig {
    pub cwd: std::path::PathBuf,
    pub workspace_key: String,
    pub codex_executable: std::path::PathBuf,
    pub codex_home: std::path::PathBuf,
    pub request_timeout: std::time::Duration,
    pub mad_max_mode: bool,
    pub include_hidden_models: bool,
    pub browser_tools_enabled: bool,
}

impl AiWorkerStartConfig {
    pub fn new(
        cwd: std::path::PathBuf,
        codex_executable: std::path::PathBuf,
        codex_home: std::path::PathBuf,
    ) -> Self {
        let workspace_key = cwd.to_string_lossy().to_string();
        Self {
            cwd,
            workspace_key,
            codex_executable,
            codex_home,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            mad_max_mode: false,
            include_hidden_models: true,
            browser_tools_enabled: false,
        }
    }

    pub(crate) fn starting_status_message(&self) -> String {
        "Starting embedded Codex App Server...".to_string()
    }
}

#[cfg(test)]
mod transport_tests {
    use super::AiWorkerStartConfig;

    #[test]
    fn worker_start_config_uses_cwd_as_workspace_key() {
        let config = AiWorkerStartConfig::new(
            std::path::PathBuf::from("/repo/worktrees/task-a"),
            std::path::PathBuf::from("/bin/codex"),
            std::path::PathBuf::from("/tmp/codex-home"),
        );
        assert_eq!(config.workspace_key, "/repo/worktrees/task-a");
    }

    #[test]
    fn worker_start_config_status_message_is_embedded_only() {
        let config = AiWorkerStartConfig::new(
            std::path::PathBuf::from("/repo/worktrees/task-a"),
            std::path::PathBuf::from("/bin/codex"),
            std::path::PathBuf::from("/tmp/codex-home"),
        );

        assert_eq!(
            config.starting_status_message(),
            "Starting embedded Codex App Server..."
        );
    }

    #[test]
    fn worker_start_config_disables_browser_tools_by_default() {
        let config = AiWorkerStartConfig::new(
            std::path::PathBuf::from("/repo/worktrees/task-a"),
            std::path::PathBuf::from("/bin/codex"),
            std::path::PathBuf::from("/tmp/codex-home"),
        );

        assert!(!config.browser_tools_enabled);
    }
}
