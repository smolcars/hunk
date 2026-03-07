use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};

const APP_DATA_DIR_NAME: &str = "hunk";
const STATE_FILE_NAME: &str = "state.toml";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiServiceTierSelection {
    #[default]
    Standard,
    Fast,
    Flex,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AiCollaborationModeSelection {
    #[default]
    Default,
    Plan,
}

impl AiCollaborationModeSelection {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Plan => "Plan",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiThreadSessionState {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub collaboration_mode: AiCollaborationModeSelection,
    pub service_tier: Option<AiServiceTierSelection>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CachedChangedFileState {
    pub path: String,
    pub status_tag: String,
    pub staged: bool,
    pub untracked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CachedLocalBranchState {
    pub name: String,
    pub is_current: bool,
    pub tip_unix_time: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CachedWorkflowState {
    pub root: Option<PathBuf>,
    pub branch_name: String,
    pub branch_has_upstream: bool,
    pub branch_ahead_count: usize,
    pub branch_behind_count: usize,
    pub branches: Vec<CachedLocalBranchState>,
    pub files: Vec<CachedChangedFileState>,
    pub last_commit_subject: Option<String>,
    pub cached_unix_time: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub last_project_path: Option<PathBuf>,
    pub ai_workspace_mad_max: BTreeMap<String, bool>,
    pub ai_workspace_include_hidden_models: BTreeMap<String, bool>,
    pub ai_workspace_session_overrides: BTreeMap<String, AiThreadSessionState>,
    pub git_workflow_cache: Option<CachedWorkflowState>,
}

#[derive(Debug, Clone)]
pub struct AppStateStore {
    path: PathBuf,
}

impl AppStateStore {
    pub fn new() -> Result<Self> {
        let base_dir = dirs::data_local_dir()
            .or_else(dirs::data_dir)
            .or_else(dirs::home_dir)
            .ok_or_else(|| anyhow!("failed to resolve app data directory"))?;
        Ok(Self {
            path: base_dir.join(APP_DATA_DIR_NAME).join(STATE_FILE_NAME),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_default(&self) -> Result<AppState> {
        if !self.path.exists() {
            return Ok(AppState::default());
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read state file at {}", self.path.display()))?;
        toml::from_str::<AppState>(&raw)
            .with_context(|| format!("failed to parse TOML state file at {}", self.path.display()))
    }

    pub fn save(&self, state: &AppState) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow!("state path has no parent: {}", self.path.display()))?;

        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create state directory {}", parent.display()))?;

        let contents =
            toml::to_string_pretty(state).context("failed to serialize app state to TOML")?;
        fs::write(&self.path, contents)
            .with_context(|| format!("failed to write state file at {}", self.path.display()))?;
        Ok(())
    }
}
