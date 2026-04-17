//! Hunk-owned protocol seam for embedded Codex integration.
//!
//! The rest of the workspace should import Codex app-server and model/config
//! types from this module instead of reaching into upstream crate paths
//! directly. That keeps future Codex bumps localized to `hunk-codex`.

pub use codex_app_server_protocol::*;
pub use codex_protocol::config_types::{CollaborationMode, ModeKind, ServiceTier, Settings};
pub use codex_protocol::openai_models::{InputModality, ReasoningEffort};
pub use codex_protocol::protocol::SessionSource;
