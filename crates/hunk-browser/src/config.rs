use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRuntimeConfig {
    pub cef_runtime_dir: PathBuf,
    pub helper_executable_path: PathBuf,
    pub storage_paths: BrowserStoragePaths,
}

impl BrowserRuntimeConfig {
    pub fn new(
        cef_runtime_dir: impl Into<PathBuf>,
        helper_executable_path: impl Into<PathBuf>,
        storage_paths: BrowserStoragePaths,
    ) -> Self {
        Self {
            cef_runtime_dir: cef_runtime_dir.into(),
            helper_executable_path: helper_executable_path.into(),
            storage_paths,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserStoragePaths {
    pub storage_root: PathBuf,
    pub root_cache_path: PathBuf,
    pub profile_path: PathBuf,
    pub downloads_path: PathBuf,
}

impl BrowserStoragePaths {
    pub fn from_app_data_dir(app_data_dir: impl AsRef<Path>) -> Self {
        Self::from_app_data_dir_with_profile_id(app_data_dir, "default")
    }

    pub fn from_app_data_dir_with_profile_id(
        app_data_dir: impl AsRef<Path>,
        profile_id: impl AsRef<str>,
    ) -> Self {
        let storage_root = app_data_dir.as_ref().join("browser");
        let profile_id = sanitize_profile_id(profile_id.as_ref());
        let root_cache_path = if profile_id == "default" {
            storage_root.join("cef-root")
        } else {
            storage_root.join("cef-roots").join(profile_id)
        };
        let profile_path = root_cache_path.join("profile");
        let downloads_path = storage_root.join("downloads");

        Self {
            storage_root,
            root_cache_path,
            profile_path,
            downloads_path,
        }
    }

    pub fn ensure_directories(&self) -> Result<(), BrowserConfigError> {
        create_dir(&self.storage_root)?;
        create_dir(&self.root_cache_path)?;
        create_dir(&self.profile_path)?;
        create_dir(&self.downloads_path)?;
        Ok(())
    }
}

fn sanitize_profile_id(profile_id: &str) -> String {
    let sanitized: String = profile_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .take(96)
        .collect();
    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Error)]
pub enum BrowserConfigError {
    #[error("failed to create browser directory '{path}': {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

fn create_dir(path: &Path) -> Result<(), BrowserConfigError> {
    std::fs::create_dir_all(path).map_err(|source| BrowserConfigError::CreateDir {
        path: path.to_path_buf(),
        source,
    })
}
