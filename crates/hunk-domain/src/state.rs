use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};

const APP_DATA_DIR_NAME: &str = "hunk";
const STATE_FILE_NAME: &str = "state.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub last_project_path: Option<PathBuf>,
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
