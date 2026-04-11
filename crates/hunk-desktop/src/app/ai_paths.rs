use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn resolve_codex_home_path() -> Option<PathBuf> {
    resolve_codex_home_path_from(
        env::var_os("CODEX_HOME").map(PathBuf::from),
        user_home_dir(),
    )
}

pub(super) fn default_codex_home_path() -> Option<PathBuf> {
    user_home_dir().map(|home_dir| home_dir.join(".codex"))
}

pub(super) fn resolve_ai_chats_root_path() -> Option<PathBuf> {
    hunk_domain::paths::hunk_home_dir()
        .ok()
        .map(|home_dir| home_dir.join("chats"))
}

pub(super) fn ensure_ai_chats_root_path() -> Option<PathBuf> {
    let chats_root = resolve_ai_chats_root_path()?;
    fs::create_dir_all(&chats_root).ok()?;
    Some(chats_root)
}

fn user_home_dir() -> Option<PathBuf> {
    dirs::home_dir()
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

fn resolve_codex_home_path_from(
    configured_path: Option<PathBuf>,
    home_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    match configured_path {
        Some(path) => expand_home_prefixed_path(path, home_dir.as_deref()),
        None => home_dir.map(|home_dir| home_dir.join(".codex")),
    }
}

fn expand_home_prefixed_path(path: PathBuf, home_dir: Option<&Path>) -> Option<PathBuf> {
    let Some(relative_suffix) = home_relative_suffix(path.as_path()) else {
        return Some(path);
    };

    let mut resolved = home_dir?.to_path_buf();
    if !relative_suffix.as_os_str().is_empty() {
        resolved.push(relative_suffix);
    }
    Some(resolved)
}

fn home_relative_suffix(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    match components.next()? {
        Component::Normal(component) if component == OsStr::new("~") => {
            let mut suffix = PathBuf::new();
            for component in components {
                suffix.push(component.as_os_str());
            }
            Some(suffix)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::expand_home_prefixed_path;
    use super::resolve_ai_chats_root_path;
    use super::resolve_codex_home_path_from;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn default_codex_home_uses_resolved_home_directory() {
        let home_dir = PathBuf::from("users").join("coco");

        let resolved = resolve_codex_home_path_from(None, Some(home_dir.clone()));

        assert_eq!(resolved, Some(home_dir.join(".codex")));
    }

    #[test]
    fn configured_tilde_path_expands_from_home_directory() {
        let home_dir = PathBuf::from("users").join("coco");

        let resolved =
            resolve_codex_home_path_from(Some(PathBuf::from("~/.codex")), Some(home_dir.clone()));

        assert_eq!(resolved, Some(home_dir.join(".codex")));
    }

    #[test]
    fn configured_non_tilde_path_is_left_unchanged() {
        let configured = PathBuf::from("custom").join("codex-home");

        let resolved = resolve_codex_home_path_from(Some(configured.clone()), None);

        assert_eq!(resolved, Some(configured));
    }

    #[test]
    fn configured_tilde_path_requires_a_home_directory() {
        let resolved = resolve_codex_home_path_from(Some(PathBuf::from("~/.codex")), None);

        assert_eq!(resolved, None);
    }

    #[test]
    fn bare_tilde_expands_to_the_home_directory() {
        let home_dir = PathBuf::from("users").join("coco");

        let resolved = expand_home_prefixed_path(PathBuf::from("~"), Some(home_dir.as_path()));

        assert_eq!(resolved, Some(home_dir));
    }

    #[test]
    fn chats_root_uses_hunk_home_dir_override() {
        let _guard = env_lock().lock().expect("env lock should be available");
        let previous = std::env::var_os(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR);
        unsafe {
            std::env::set_var(
                hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR,
                PathBuf::from("/tmp").join("custom-hunk-home"),
            );
        }

        let resolved = resolve_ai_chats_root_path();

        match previous {
            Some(value) => unsafe {
                std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, value)
            },
            None => unsafe { std::env::remove_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR) },
        }

        assert_eq!(
            resolved,
            Some(PathBuf::from("/tmp").join("custom-hunk-home").join("chats")),
        );
    }
}
