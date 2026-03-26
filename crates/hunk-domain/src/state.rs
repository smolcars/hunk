use std::collections::{BTreeMap, BTreeSet};
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
            Self::Default => "Code",
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

impl AiThreadSessionState {
    pub fn preferred_defaults() -> Self {
        Self {
            model: Some("gpt-5.4".to_string()),
            effort: Some("high".to_string()),
            collaboration_mode: AiCollaborationModeSelection::Default,
            service_tier: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CachedChangedFileState {
    pub path: String,
    pub status_tag: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CachedLocalBranchState {
    pub name: String,
    pub is_current: bool,
    pub tip_unix_time: Option<i64>,
    pub attached_workspace_target_id: Option<String>,
    pub attached_workspace_target_root: Option<PathBuf>,
    pub attached_workspace_target_label: Option<String>,
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
pub struct CachedRecentCommitState {
    pub commit_id: String,
    pub subject: String,
    pub committed_unix_time: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CachedRecentCommitsState {
    pub root: Option<PathBuf>,
    pub head_ref_name: Option<String>,
    pub head_commit_id: Option<String>,
    pub base_tip_id: Option<String>,
    pub commits: Vec<CachedRecentCommitState>,
    pub cached_unix_time: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewCompareSelectionState {
    pub left_source_id: Option<String>,
    pub right_source_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    #[serde(alias = "last_project_path", skip_serializing)]
    pub legacy_last_project_path: Option<PathBuf>,
    pub workspace_project_paths: Vec<PathBuf>,
    pub active_workspace_project_path: Option<PathBuf>,
    pub last_workspace_target_by_repo: BTreeMap<String, String>,
    pub review_compare_selection_by_repo: BTreeMap<String, ReviewCompareSelectionState>,
    pub ai_bookmarked_thread_ids: BTreeSet<String>,
    pub ai_workspace_mad_max: BTreeMap<String, bool>,
    pub ai_workspace_include_hidden_models: BTreeMap<String, bool>,
    pub ai_workspace_session_overrides: BTreeMap<String, AiThreadSessionState>,
    pub ai_thread_session_overrides: BTreeMap<String, AiThreadSessionState>,
    pub git_workflow_cache_by_repo: BTreeMap<String, CachedWorkflowState>,
    pub git_recent_commits_cache_by_repo: BTreeMap<String, CachedRecentCommitsState>,
}

impl AppState {
    pub fn contains_workspace_project(&self, project_path: &Path) -> bool {
        self.workspace_project_paths
            .iter()
            .any(|path| path.as_path() == project_path)
    }

    pub fn activate_workspace_project(&mut self, project_path: PathBuf) -> bool {
        let previous_paths = self.workspace_project_paths.clone();
        let previous_active = self.active_workspace_project_path.clone();

        if !self.contains_workspace_project(project_path.as_path()) {
            self.workspace_project_paths.push(project_path.clone());
        }
        self.active_workspace_project_path = Some(project_path);
        self.normalize_workspace_state();

        self.workspace_project_paths != previous_paths
            || self.active_workspace_project_path != previous_active
    }

    pub fn remove_workspace_project(&mut self, project_path: &Path) -> bool {
        let Some(removal_index) = self
            .workspace_project_paths
            .iter()
            .position(|path| path.as_path() == project_path)
        else {
            return false;
        };

        let previous_paths = self.workspace_project_paths.clone();
        let previous_active = self.active_workspace_project_path.clone();
        let was_active = self.active_workspace_project_path.as_deref() == Some(project_path);

        self.workspace_project_paths.remove(removal_index);
        if was_active {
            self.active_workspace_project_path =
                if removal_index < self.workspace_project_paths.len() {
                    Some(self.workspace_project_paths[removal_index].clone())
                } else if removal_index > 0 {
                    Some(self.workspace_project_paths[removal_index - 1].clone())
                } else {
                    None
                };
        }
        self.normalize_workspace_state();

        self.workspace_project_paths != previous_paths
            || self.active_workspace_project_path != previous_active
    }

    pub fn normalize_workspace_state(&mut self) {
        let mut seen_paths = BTreeSet::new();
        let mut normalized_paths = Vec::with_capacity(self.workspace_project_paths.len() + 1);

        for path in &self.workspace_project_paths {
            if seen_paths.insert(path.clone()) {
                normalized_paths.push(path.clone());
            }
        }

        let preferred_active_path = self
            .active_workspace_project_path
            .clone()
            .or_else(|| self.legacy_last_project_path.clone());

        if let Some(active_path) = preferred_active_path.as_ref()
            && seen_paths.insert(active_path.clone())
        {
            normalized_paths.push(active_path.clone());
        }

        let resolved_active_path = preferred_active_path
            .filter(|active_path| normalized_paths.iter().any(|path| path == active_path))
            .or_else(|| normalized_paths.first().cloned());

        self.workspace_project_paths = normalized_paths;
        self.active_workspace_project_path = resolved_active_path.clone();
        self.legacy_last_project_path = None;
    }

    pub fn active_project_path(&self) -> Option<&PathBuf> {
        self.active_workspace_project_path.as_ref()
    }
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
