use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};

const CONFIG_FILE_NAME: &str = "config.toml";
const DEFAULT_AUTO_REFRESH_INTERVAL_MS: u64 = 60_000;

pub const fn default_auto_refresh_interval_ms() -> u64 {
    DEFAULT_AUTO_REFRESH_INTERVAL_MS
}

pub const fn default_terminal_hydrate_app_environment_on_launch() -> bool {
    !cfg!(target_os = "windows")
}

pub const fn default_desktop_notifications_enabled() -> bool {
    true
}

pub const fn default_desktop_notifications_only_when_unfocused() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewProviderKind {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitlab")]
    GitLab,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewProviderMapping {
    pub host: String,
    pub provider: ReviewProviderKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalShell {
    #[default]
    System,
    Program(String),
    WithArguments {
        program: String,
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    pub shell: TerminalShell,
    pub inherit_login_environment: bool,
    #[serde(default = "default_terminal_hydrate_app_environment_on_launch")]
    pub hydrate_app_environment_on_launch: bool,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            shell: TerminalShell::System,
            inherit_login_environment: true,
            hydrate_app_environment_on_launch: default_terminal_hydrate_app_environment_on_launch(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiDesktopNotificationsConfig {
    #[serde(default = "default_desktop_notifications_enabled")]
    pub agent_finished: bool,
    #[serde(default = "default_desktop_notifications_enabled")]
    pub plan_ready: bool,
    #[serde(default = "default_desktop_notifications_enabled")]
    pub user_input_required: bool,
    #[serde(default = "default_desktop_notifications_enabled")]
    pub approval_required: bool,
}

impl Default for AiDesktopNotificationsConfig {
    fn default() -> Self {
        Self {
            agent_finished: true,
            plan_ready: true,
            user_input_required: true,
            approval_required: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopNotificationsConfig {
    #[serde(default = "default_desktop_notifications_enabled")]
    pub enabled: bool,
    #[serde(default = "default_desktop_notifications_only_when_unfocused")]
    pub only_when_unfocused: bool,
    pub ai: AiDesktopNotificationsConfig,
}

impl Default for DesktopNotificationsConfig {
    fn default() -> Self {
        Self {
            enabled: default_desktop_notifications_enabled(),
            only_when_unfocused: default_desktop_notifications_only_when_unfocused(),
            ai: AiDesktopNotificationsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardShortcuts {
    pub select_next_line: Vec<String>,
    pub select_previous_line: Vec<String>,
    pub extend_selection_next_line: Vec<String>,
    pub extend_selection_previous_line: Vec<String>,
    pub copy_selection: Vec<String>,
    pub select_all_diff_rows: Vec<String>,
    pub next_hunk: Vec<String>,
    pub previous_hunk: Vec<String>,
    pub next_file: Vec<String>,
    pub previous_file: Vec<String>,
    pub view_current_review_file: Vec<String>,
    pub toggle_sidebar_tree: Vec<String>,
    pub switch_to_files_view: Vec<String>,
    pub switch_to_review_view: Vec<String>,
    #[serde(alias = "switch_to_graph_view")]
    pub switch_to_git_view: Vec<String>,
    pub switch_to_ai_view: Vec<String>,
    pub toggle_ai_terminal_drawer: Vec<String>,
    pub open_project: Vec<String>,
    pub save_current_file: Vec<String>,
    pub next_editor_tab: Vec<String>,
    pub previous_editor_tab: Vec<String>,
    pub close_editor_tab: Vec<String>,
    pub open_settings: Vec<String>,
    pub quit_app: Vec<String>,
    pub repo_tree_new_file: Vec<String>,
    pub repo_tree_new_folder: Vec<String>,
    pub repo_tree_rename_file: Vec<String>,
}

impl Default for KeyboardShortcuts {
    fn default() -> Self {
        Self {
            select_next_line: vec!["down".into()],
            select_previous_line: vec!["up".into()],
            extend_selection_next_line: vec!["shift-down".into()],
            extend_selection_previous_line: vec!["shift-up".into()],
            copy_selection: vec!["cmd-c".into(), "ctrl-c".into()],
            select_all_diff_rows: vec!["cmd-a".into(), "ctrl-a".into()],
            next_hunk: vec!["f7".into()],
            previous_hunk: vec!["shift-f7".into()],
            next_file: vec!["alt-down".into()],
            previous_file: vec!["alt-up".into()],
            view_current_review_file: vec!["g space".into()],
            toggle_sidebar_tree: vec!["cmd-b".into(), "ctrl-b".into()],
            switch_to_files_view: vec!["cmd-1".into(), "ctrl-1".into()],
            switch_to_review_view: vec!["cmd-2".into(), "ctrl-2".into()],
            switch_to_git_view: vec!["cmd-3".into(), "ctrl-3".into()],
            switch_to_ai_view: vec!["cmd-4".into(), "ctrl-4".into()],
            toggle_ai_terminal_drawer: vec!["cmd-j".into(), "ctrl-j".into()],
            open_project: vec!["cmd-shift-o".into(), "ctrl-shift-o".into()],
            save_current_file: vec!["cmd-s".into(), "ctrl-s".into()],
            next_editor_tab: if cfg!(target_os = "macos") {
                vec!["cmd-}".into()]
            } else {
                vec!["ctrl-shift-]".into()]
            },
            previous_editor_tab: if cfg!(target_os = "macos") {
                vec!["cmd-{".into()]
            } else {
                vec!["ctrl-shift-[".into()]
            },
            close_editor_tab: if cfg!(target_os = "macos") {
                vec!["cmd-w".into()]
            } else {
                vec!["ctrl-w".into()]
            },
            open_settings: vec!["cmd-,".into(), "ctrl-,".into()],
            quit_app: vec!["cmd-q".into()],
            repo_tree_new_file: vec!["%".into()],
            repo_tree_new_folder: vec!["d".into()],
            repo_tree_rename_file: vec!["shift-r".into()],
        }
    }
}

impl KeyboardShortcuts {
    fn normalize_files_tab_shortcuts(&mut self) {
        if cfg!(target_os = "macos") {
            if self.next_editor_tab.len() == 1
                && self.next_editor_tab.first().map(String::as_str) == Some("cmd-shift-]")
            {
                self.next_editor_tab = vec!["cmd-}".into()];
            }
            if self.previous_editor_tab.len() == 1
                && self.previous_editor_tab.first().map(String::as_str) == Some("cmd-shift-[")
            {
                self.previous_editor_tab = vec!["cmd-{".into()];
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub theme: ThemePreference,
    pub reduce_motion: bool,
    pub show_fps_counter: bool,
    pub auto_update_enabled: bool,
    pub desktop_notifications: DesktopNotificationsConfig,
    pub terminal: TerminalConfig,
    pub keyboard_shortcuts: KeyboardShortcuts,
    pub review_provider_mappings: Vec<ReviewProviderMapping>,
    #[serde(default = "default_auto_refresh_interval_ms")]
    pub auto_refresh_interval_ms: u64,
    pub last_update_check_at: Option<i64>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut config = Self {
            theme: ThemePreference::System,
            reduce_motion: false,
            show_fps_counter: true,
            auto_update_enabled: true,
            desktop_notifications: DesktopNotificationsConfig::default(),
            terminal: TerminalConfig::default(),
            keyboard_shortcuts: KeyboardShortcuts::default(),
            review_provider_mappings: Vec::new(),
            auto_refresh_interval_ms: default_auto_refresh_interval_ms(),
            last_update_check_at: None,
        };
        config.keyboard_shortcuts.normalize_files_tab_shortcuts();
        config
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new() -> Result<Self> {
        let path = crate::paths::hunk_home_dir()?.join(CONFIG_FILE_NAME);
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_create_default(&self) -> Result<AppConfig> {
        if !self.path.exists() {
            let config = AppConfig::default();
            self.save(&config)?;
            return Ok(config);
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read config file at {}", self.path.display()))?;
        let mut config = toml::from_str::<AppConfig>(&raw).with_context(|| {
            format!(
                "failed to parse TOML config file at {}",
                self.path.display()
            )
        })?;
        config.keyboard_shortcuts.normalize_files_tab_shortcuts();
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow!("config path has no parent: {}", self.path.display()))?;

        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;

        let contents =
            toml::to_string_pretty(config).context("failed to serialize app config to TOML")?;
        fs::write(&self.path, contents)
            .with_context(|| format!("failed to write config file at {}", self.path.display()))?;
        Ok(())
    }
}
