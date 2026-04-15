use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};

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

pub(super) fn is_ai_chats_workspace_path(path: &Path) -> bool {
    resolve_ai_chats_root_path().is_some_and(|chats_root| path == chats_root)
}

pub(super) fn ai_chats_workspace_paths() -> Vec<PathBuf> {
    let Some(chats_root) = ensure_ai_chats_root_path().or_else(resolve_ai_chats_root_path) else {
        return Vec::new();
    };

    vec![chats_root]
}

#[cfg(test)]
pub(crate) fn lock_hunk_home_test_env() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    }
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
    use super::ai_chats_workspace_paths;
    use super::expand_home_prefixed_path;
    use super::is_ai_chats_workspace_path;
    use super::lock_hunk_home_test_env;
    use super::resolve_ai_chats_root_path;
    use super::resolve_codex_home_path_from;
    use std::path::PathBuf;

    fn canonicalize_if_exists(path: PathBuf) -> PathBuf {
        if !path.exists() {
            return path;
        }

        std::fs::canonicalize(path.as_path()).unwrap_or(path)
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
        let _guard = lock_hunk_home_test_env();
        let hunk_home = std::env::temp_dir().join("custom-hunk-home");
        let previous = std::env::var_os(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR);
        let _ = std::fs::remove_dir_all(&hunk_home);
        std::fs::create_dir_all(&hunk_home).expect("override hunk home should exist");
        unsafe { std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, &hunk_home) };

        let resolved = resolve_ai_chats_root_path();
        let expected = canonicalize_if_exists(hunk_home.clone()).join("chats");

        match previous {
            Some(value) => unsafe {
                std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, value)
            },
            None => unsafe { std::env::remove_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR) },
        }
        let _ = std::fs::remove_dir_all(&hunk_home);

        assert_eq!(resolved, Some(expected),);
    }

    #[test]
    fn chats_workspace_classifies_only_root() {
        let _guard = lock_hunk_home_test_env();
        let hunk_home = std::env::temp_dir().join("hunk-ai-paths-descendants");
        let previous = std::env::var_os(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR);
        let _ = std::fs::remove_dir_all(&hunk_home);
        std::fs::create_dir_all(&hunk_home).expect("override hunk home should exist");
        unsafe { std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, &hunk_home) };

        let chats_root = resolve_ai_chats_root_path().expect("chats root should resolve");
        let thread_root = chats_root.join("chat-1");

        assert!(is_ai_chats_workspace_path(chats_root.as_path()));
        assert!(!is_ai_chats_workspace_path(thread_root.as_path()));
        assert!(!is_ai_chats_workspace_path(
            PathBuf::from("/repo").as_path()
        ));

        match previous {
            Some(value) => unsafe {
                std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, value)
            },
            None => unsafe { std::env::remove_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR) },
        }
        let _ = std::fs::remove_dir_all(&hunk_home);
    }

    #[test]
    fn chat_workspace_paths_only_include_root() {
        let _guard = lock_hunk_home_test_env();
        let hunk_home = std::env::temp_dir().join("hunk-ai-paths-workspaces");
        let chats_root = hunk_home.join("chats");
        let previous = std::env::var_os(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR);
        unsafe { std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, &hunk_home) };
        let _ = std::fs::remove_dir_all(&hunk_home);
        std::fs::create_dir_all(&chats_root).expect("chats root should exist");

        let workspaces = ai_chats_workspace_paths();
        let expected = vec![canonicalize_if_exists(chats_root.clone())];

        match previous {
            Some(value) => unsafe {
                std::env::set_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR, value)
            },
            None => unsafe { std::env::remove_var(hunk_domain::paths::HUNK_HOME_DIR_ENV_VAR) },
        }
        let _ = std::fs::remove_dir_all(&hunk_home);

        assert_eq!(workspaces, expected);
    }
}
