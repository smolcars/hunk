fn resolve_bundled_codex_executable_from_exe(
    current_exe: &std::path::Path,
) -> Option<std::path::PathBuf> {
    bundled_codex_executable_candidates(current_exe)
        .into_iter()
        .find(|candidate| {
            #[cfg(target_os = "windows")]
            {
                windows_path_is_spawnable(candidate)
            }
            #[cfg(not(target_os = "windows"))]
            {
                candidate.is_file()
            }
        })
}

#[cfg(target_os = "windows")]
fn resolve_windows_command_path(command_name: &std::path::Path) -> Option<std::path::PathBuf> {
    if is_command_name_without_path(command_name) {
        return resolve_windows_command_path_from_env(
            command_name,
            std::env::var_os("PATH"),
            std::env::var_os("PATHEXT"),
        );
    }

    resolve_windows_explicit_command_path(command_name, std::env::var_os("PATHEXT"))
}

#[cfg(target_os = "windows")]
fn resolve_windows_command_path_from_env(
    command_name: &std::path::Path,
    path_var: Option<OsString>,
    pathext_var: Option<OsString>,
) -> Option<std::path::PathBuf> {
    let command_name = command_name.as_os_str();
    let path_var = path_var?;
    let candidate_names = windows_command_candidate_names(command_name, pathext_var.as_deref());
    std::env::split_paths(&path_var).find_map(|directory| {
        candidate_names
            .iter()
            .map(|candidate| directory.join(candidate))
            .find(|candidate| windows_path_is_spawnable(candidate))
    })
}

#[cfg(target_os = "windows")]
fn resolve_windows_explicit_command_path(
    command_path: &std::path::Path,
    pathext_var: Option<OsString>,
) -> Option<std::path::PathBuf> {
    if windows_path_is_spawnable(command_path) {
        return Some(command_path.to_path_buf());
    }

    let parent = command_path.parent()?;
    let file_name = command_path.file_name()?;
    let candidate_names = windows_command_candidate_names(file_name, pathext_var.as_deref());
    candidate_names
        .iter()
        .map(|candidate| parent.join(candidate))
        .find(|candidate| windows_path_is_spawnable(candidate))
}

#[cfg(target_os = "windows")]
fn windows_command_candidate_names(
    command_name: &OsStr,
    pathext_var: Option<&OsStr>,
) -> Vec<OsString> {
    let command_path = std::path::Path::new(command_name);
    if command_path.extension().is_some() {
        return vec![command_name.to_os_string()];
    }

    let mut candidates = Vec::new();
    let pathext_var = pathext_var
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsStr::new(".COM;.EXE;.BAT;.CMD"));
    for extension in pathext_var
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
    {
        let normalized = if extension.starts_with('.') {
            extension.to_string()
        } else {
            format!(".{extension}")
        };
        candidates.push(OsString::from(format!(
            "{}{}",
            command_name.to_string_lossy(),
            normalized
        )));
    }
    candidates.push(command_name.to_os_string());
    candidates
}

#[cfg(target_os = "windows")]
fn windows_path_is_spawnable(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match extension.as_deref() {
        Some("cmd" | "bat" | "com") => true,
        Some("exe") => windows_file_has_mz_header(path),
        Some(_) => false,
        None => windows_file_has_mz_header(path),
    }
}

#[cfg(target_os = "windows")]
fn windows_file_has_mz_header(path: &std::path::Path) -> bool {
    use std::io::Read as _;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 2];
    file.read_exact(&mut header).is_ok() && header == *b"MZ"
}

#[cfg(target_os = "windows")]
const BUNDLED_CODEX_ENTRYPOINT_FILE_NAMES: &[&str] = &["codex.cmd", "codex.exe"];
#[cfg(not(target_os = "windows"))]
const BUNDLED_CODEX_ENTRYPOINT_FILE_NAMES: &[&str] = &["codex"];

fn bundled_codex_executable_candidates(current_exe: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Some(exe_dir) = current_exe.parent() else {
        return Vec::new();
    };

    let platform_dir = codex_runtime_platform_dir();
    let mut candidates = Vec::new();
    let push_candidates = |base_dir: &std::path::Path, candidates: &mut Vec<std::path::PathBuf>| {
        for entrypoint in bundled_codex_entrypoint_file_names() {
            candidates.push(base_dir.join(entrypoint));
        }
    };

    push_candidates(
        exe_dir.join("codex-runtime").join(platform_dir).as_path(),
        &mut candidates,
    );

    if cfg!(target_os = "macos") && let Some(contents_dir) = exe_dir.parent() {
        push_candidates(
            contents_dir
                .join("Resources")
                .join("codex-runtime")
                .join(platform_dir)
                .as_path(),
            &mut candidates,
        );
    } else {
        push_candidates(
            exe_dir
                .join("Resources")
                .join("codex-runtime")
                .join(platform_dir)
                .as_path(),
            &mut candidates,
        );
    }

    #[cfg(target_os = "linux")]
    if let Some(binary_file_name) = current_exe.file_name()
        && let Some(usr_dir) = exe_dir.parent()
    {
        push_candidates(
            usr_dir
                .join("lib")
                .join(binary_file_name)
                .join("codex-runtime")
                .join(platform_dir)
                .as_path(),
            &mut candidates,
        );
    }

    push_candidates(exe_dir, &mut candidates);

    candidates
}

fn codex_runtime_platform_dir() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

#[cfg(test)]
fn codex_runtime_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "codex.exe"
    } else {
        "codex"
    }
}

fn bundled_codex_entrypoint_file_names() -> &'static [&'static str] {
    BUNDLED_CODEX_ENTRYPOINT_FILE_NAMES
}

fn is_command_name_without_path(path: &std::path::Path) -> bool {
    if path.is_absolute() {
        return false;
    }
    let text = path.to_string_lossy();
    !text.contains(std::path::MAIN_SEPARATOR) && !text.contains('/')
}
