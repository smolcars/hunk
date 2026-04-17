pub mod api;
pub mod app_server_client;
pub mod diff_stats;
pub mod errors;
mod in_process_app_server_client;
pub mod protocol;
pub mod rpc;
pub mod state;
pub mod structured_generation;
pub mod threads;
pub mod tools;

/// Pinned upstream Codex commit selected during implementation kickoff.
pub const CODEX_PINNED_MAIN_COMMIT: &str = "6bee02a346d0aa8dc4d5dcb312545fa37408b6ca";
/// Codex App Server docs reference used for this integration.
pub const CODEX_APP_SERVER_DOCS_URL: &str = "https://developers.openai.com/codex/app-server";
