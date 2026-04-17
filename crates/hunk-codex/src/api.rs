use crate::protocol::ClientInfo;
use crate::protocol::InitializeCapabilities;
use crate::protocol::InitializeParams;

/// Supported transport identifiers in Hunk's Codex integration.
pub const SUPPORTED_TRANSPORTS: &[&str] = &["embedded", "websocket"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializeOptions {
    pub client_name: String,
    pub client_title: Option<String>,
    pub client_version: String,
    pub experimental_api: bool,
    pub opt_out_notification_methods: Vec<String>,
}

impl Default for InitializeOptions {
    fn default() -> Self {
        Self {
            client_name: "hunk-desktop".to_string(),
            client_title: Some("Hunk".to_string()),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
        }
    }
}

impl InitializeOptions {
    pub fn into_params(self) -> InitializeParams {
        let opt_out = if self.opt_out_notification_methods.is_empty() {
            None
        } else {
            Some(self.opt_out_notification_methods)
        };

        InitializeParams {
            client_info: ClientInfo {
                name: self.client_name,
                title: self.client_title,
                version: self.client_version,
            },
            capabilities: Some(InitializeCapabilities {
                experimental_api: self.experimental_api,
                opt_out_notification_methods: opt_out,
            }),
        }
    }
}

pub mod method {
    pub const INITIALIZE: &str = "initialize";
    pub const INITIALIZED: &str = "initialized";

    pub const THREAD_LIST: &str = "thread/list";
    pub const THREAD_START: &str = "thread/start";
    pub const THREAD_RESUME: &str = "thread/resume";
    pub const THREAD_FORK: &str = "thread/fork";
    pub const THREAD_ARCHIVE: &str = "thread/archive";
    pub const THREAD_UNARCHIVE: &str = "thread/unarchive";
    pub const THREAD_UNSUBSCRIBE: &str = "thread/unsubscribe";
    pub const THREAD_COMPACT_START: &str = "thread/compact/start";
    pub const THREAD_ROLLBACK: &str = "thread/rollback";
    pub const THREAD_LOADED_LIST: &str = "thread/loaded/list";
    pub const THREAD_READ: &str = "thread/read";
    pub const SKILLS_LIST: &str = "skills/list";
    pub const SKILLS_CONFIG_WRITE: &str = "skills/config/write";
    pub const APP_LIST: &str = "app/list";

    pub const TURN_START: &str = "turn/start";
    pub const TURN_STEER: &str = "turn/steer";
    pub const TURN_INTERRUPT: &str = "turn/interrupt";
    pub const REVIEW_START: &str = "review/start";

    pub const ACCOUNT_READ: &str = "account/read";
    pub const ACCOUNT_LOGIN_START: &str = "account/login/start";
    pub const ACCOUNT_LOGIN_CANCEL: &str = "account/login/cancel";
    pub const ACCOUNT_LOGOUT: &str = "account/logout";
    pub const ACCOUNT_RATE_LIMITS_READ: &str = "account/rateLimits/read";
    pub const MODEL_LIST: &str = "model/list";
    pub const EXPERIMENTAL_FEATURE_LIST: &str = "experimentalFeature/list";
    pub const COLLABORATION_MODE_LIST: &str = "collaborationMode/list";

    pub const COMMAND_EXEC: &str = "command/exec";
}
