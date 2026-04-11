use std::collections::BTreeMap;
use std::time::Duration;

use crate::app::ai_paths::resolve_codex_home_path;
use hunk_domain::state::AiCollaborationModeSelection;
use hunk_domain::state::AiServiceTierSelection;
use hunk_domain::state::AiThreadSessionState;
use hunk_codex::state::ThreadLifecycleStatus;
use hunk_codex::state::ThreadSummary;
use hunk_codex::state::TurnStatus;
use hunk_domain::state::AppState;

include!("bookmarks.rs");

include!("followup_prompts.rs");

include!("core_actions.rs");

include!("core_timeline.rs");

include!("core_workspace.rs");
