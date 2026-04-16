const AI_APP_SERVER_TRANSPORT_ENV_VAR: &str = "HUNK_CODEX_APP_SERVER_TRANSPORT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiAppServerTransportPreference {
    Auto,
    Embedded,
    RemoteBundled,
}

impl AiAppServerTransportPreference {
    pub fn from_env() -> Self {
        std::env::var(AI_APP_SERVER_TRANSPORT_ENV_VAR)
            .ok()
            .and_then(|value| Self::parse(value.as_str()))
            .unwrap_or(Self::Auto)
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Some(Self::Auto),
            "embedded" | "in-process" | "inprocess" => Some(Self::Embedded),
            "remote" | "remote-bundled" | "websocket" => Some(Self::RemoteBundled),
            _ => None,
        }
    }

    fn bootstrap_candidates(self) -> Vec<hunk_codex::app_server_client::AppServerTransportKind> {
        match self {
            Self::Auto => vec![
                hunk_codex::app_server_client::AppServerTransportKind::Embedded,
                hunk_codex::app_server_client::AppServerTransportKind::RemoteBundled,
            ],
            Self::Embedded => {
                vec![hunk_codex::app_server_client::AppServerTransportKind::Embedded]
            }
            Self::RemoteBundled => {
                vec![hunk_codex::app_server_client::AppServerTransportKind::RemoteBundled]
            }
        }
    }

    pub(crate) fn starting_status_message(self) -> String {
        match self {
            Self::Auto => "Starting Codex App Server...".to_string(),
            Self::Embedded => "Starting embedded Codex App Server...".to_string(),
            Self::RemoteBundled => "Starting remote bundled Codex App Server...".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AiWorkerStartConfig {
    pub cwd: std::path::PathBuf,
    pub host_working_directory: std::path::PathBuf,
    pub workspace_key: String,
    pub codex_executable: std::path::PathBuf,
    pub codex_home: std::path::PathBuf,
    pub request_timeout: std::time::Duration,
    pub mad_max_mode: bool,
    pub include_hidden_models: bool,
    pub transport_preference: AiAppServerTransportPreference,
}

impl AiWorkerStartConfig {
    pub fn new(
        cwd: std::path::PathBuf,
        codex_executable: std::path::PathBuf,
        codex_home: std::path::PathBuf,
    ) -> Self {
        let workspace_key = cwd.to_string_lossy().to_string();
        let host_working_directory = shared_ai_host_working_directory(cwd.as_path());
        Self {
            cwd,
            host_working_directory,
            workspace_key,
            codex_executable,
            codex_home,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            mad_max_mode: false,
            include_hidden_models: true,
            transport_preference: AiAppServerTransportPreference::from_env(),
        }
    }
}

fn shared_ai_host_working_directory(workspace_root: &std::path::Path) -> std::path::PathBuf {
    hunk_git::worktree::primary_repo_root(workspace_root)
        .unwrap_or_else(|_| workspace_root.to_path_buf())
}

#[cfg(test)]
mod transport_tests {
    use hunk_codex::app_server_client::AppServerTransportKind;

    use super::AiAppServerTransportPreference;

    #[test]
    fn transport_preference_parser_accepts_embedded_aliases() {
        assert_eq!(
            AiAppServerTransportPreference::parse("embedded"),
            Some(AiAppServerTransportPreference::Embedded)
        );
        assert_eq!(
            AiAppServerTransportPreference::parse("in-process"),
            Some(AiAppServerTransportPreference::Embedded)
        );
    }

    #[test]
    fn transport_preference_parser_accepts_remote_aliases() {
        assert_eq!(
            AiAppServerTransportPreference::parse("remote"),
            Some(AiAppServerTransportPreference::RemoteBundled)
        );
        assert_eq!(
            AiAppServerTransportPreference::parse("websocket"),
            Some(AiAppServerTransportPreference::RemoteBundled)
        );
    }

    #[test]
    fn transport_preference_parser_defaults_auto_inputs() {
        assert_eq!(
            AiAppServerTransportPreference::parse(""),
            Some(AiAppServerTransportPreference::Auto)
        );
        assert_eq!(
            AiAppServerTransportPreference::parse("auto"),
            Some(AiAppServerTransportPreference::Auto)
        );
        assert_eq!(AiAppServerTransportPreference::parse("bogus"), None);
    }

    #[test]
    fn auto_transport_candidates_match_supported_transports() {
        let candidates = AiAppServerTransportPreference::Auto.bootstrap_candidates();

        assert_eq!(
            candidates,
            vec![
                AppServerTransportKind::Embedded,
                AppServerTransportKind::RemoteBundled
            ]
        );
    }

    #[test]
    fn explicit_embedded_preference_preserves_embedded_candidate() {
        assert_eq!(
            AiAppServerTransportPreference::Embedded.bootstrap_candidates(),
            vec![AppServerTransportKind::Embedded]
        );
    }
}
