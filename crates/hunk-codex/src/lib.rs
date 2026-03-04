pub mod api;
pub mod errors;
pub mod host;
pub mod rpc;
pub mod state;
pub mod threads;
pub mod tools;
pub mod ws_client;

/// Pinned upstream Codex commit selected during implementation kickoff.
pub const CODEX_PINNED_MAIN_COMMIT: &str = "6bee02a346d0aa8dc4d5dcb312545fa37408b6ca";
/// Codex App Server docs reference used for this integration.
pub const CODEX_APP_SERVER_DOCS_URL: &str = "https://developers.openai.com/codex/app-server";
