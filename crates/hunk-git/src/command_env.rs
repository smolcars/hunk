use std::env::{join_paths, split_paths};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const MACOS_GUI_FALLBACK_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/local/MacGPG2/bin",
    "/opt/local/bin",
    "/nix/var/nix/profiles/default/bin",
    "/run/current-system/sw/bin",
];

#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINUX_GUI_FALLBACK_DIRS: &[&str] = &[
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/snap/bin",
    "/home/linuxbrew/.linuxbrew/bin",
    "/nix/var/nix/profiles/default/bin",
    "/run/current-system/sw/bin",
];

pub(crate) fn git_cli_command(program: &str) -> Command {
    #[cfg(target_os = "macos")]
    {
        unix_command_with_search_path(
            program,
            macos_gui_search_path(std::env::var_os("PATH"), std::env::var_os("HOME")),
        )
    }

    #[cfg(target_os = "linux")]
    {
        unix_command_with_search_path(
            program,
            linux_gui_search_path(std::env::var_os("PATH"), std::env::var_os("HOME")),
        )
    }

    #[cfg(target_os = "windows")]
    {
        windows_gui_command(program)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Command::new(program)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn unix_command_with_search_path(program: &str, search_path: Option<OsString>) -> Command {
    let mut command = match search_path
        .as_ref()
        .and_then(|path| resolve_program_in_search_path(program, path.as_os_str()))
    {
        Some(path) => Command::new(path),
        None => Command::new(program),
    };

    if let Some(search_path) = search_path {
        // Finder-launched apps do not inherit the user's shell PATH, which breaks Git commit
        // signing when `gpg` and `pinentry` live in Homebrew, MacPorts, or Nix locations.
        command.env("PATH", search_path);
    }

    command
}

#[cfg(target_os = "windows")]
fn windows_gui_command(program: &str) -> Command {
    let search_path = windows_gui_search_path(
        std::env::var_os("PATH"),
        std::env::var_os("ProgramFiles"),
        std::env::var_os("ProgramFiles(x86)"),
        std::env::var_os("LocalAppData"),
        std::env::var_os("ProgramData"),
        std::env::var_os("USERPROFILE"),
    );
    let pathext = std::env::var_os("PATHEXT");
    let mut command = match search_path.as_ref().and_then(|path| {
        resolve_windows_command_path_from_env(
            Path::new(program),
            Some(path.clone()),
            pathext.clone(),
        )
    }) {
        Some(path) => Command::new(path),
        None => Command::new(program),
    };

    if let Some(search_path) = search_path {
        command.env("PATH", search_path);
    }
    if let Some(pathext) = pathext {
        command.env("PATHEXT", pathext);
    }

    command
}

#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn macos_gui_search_path(
    current_path: Option<OsString>,
    home_dir: Option<OsString>,
) -> Option<OsString> {
    build_search_path(
        current_path,
        MACOS_GUI_FALLBACK_DIRS.iter().map(PathBuf::from),
        home_dir
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .into_iter()
            .flat_map(|home_dir| {
                [
                    home_dir.join(".local/bin"),
                    home_dir.join(".nix-profile/bin"),
                ]
            }),
    )
}

#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_gui_search_path(
    current_path: Option<OsString>,
    home_dir: Option<OsString>,
) -> Option<OsString> {
    build_search_path(
        current_path,
        LINUX_GUI_FALLBACK_DIRS.iter().map(PathBuf::from),
        home_dir
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .into_iter()
            .flat_map(|home_dir| {
                [
                    home_dir.join(".local/bin"),
                    home_dir.join(".linuxbrew/bin"),
                    home_dir.join(".nix-profile/bin"),
                ]
            }),
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn build_search_path<I, J>(
    current_path: Option<OsString>,
    fallback_dirs: I,
    user_dirs: J,
) -> Option<OsString>
where
    I: IntoIterator<Item = PathBuf>,
    J: IntoIterator<Item = PathBuf>,
{
    let mut directories = current_path
        .as_ref()
        .into_iter()
        .flat_map(split_paths)
        .collect::<Vec<_>>();

    append_unique_paths(&mut directories, fallback_dirs);
    append_unique_paths(&mut directories, user_dirs);

    if directories.is_empty() {
        return None;
    }

    join_paths(directories).ok()
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_gui_search_path(
    current_path: Option<OsString>,
    program_files: Option<OsString>,
    program_files_x86: Option<OsString>,
    local_app_data: Option<OsString>,
    program_data: Option<OsString>,
    user_profile: Option<OsString>,
) -> Option<OsString> {
    let mut directories = current_path
        .as_ref()
        .into_iter()
        .flat_map(split_paths)
        .collect::<Vec<_>>();

    for base_dir in [program_files, program_files_x86]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        append_unique_paths(
            &mut directories,
            [
                base_dir.join("Git").join("cmd"),
                base_dir.join("Git").join("bin"),
                base_dir.join("Git").join("usr").join("bin"),
                base_dir.join("GnuPG").join("bin"),
                base_dir.join("Gpg4win").join("bin"),
            ],
        );
    }

    if let Some(local_app_data) = local_app_data.filter(|value| !value.is_empty()) {
        let local_app_data = PathBuf::from(local_app_data);
        append_unique_paths(
            &mut directories,
            [
                local_app_data.join("Programs").join("Git").join("cmd"),
                local_app_data.join("Programs").join("Git").join("bin"),
                local_app_data
                    .join("Programs")
                    .join("Git")
                    .join("usr")
                    .join("bin"),
                local_app_data.join("Programs").join("GnuPG").join("bin"),
                local_app_data.join("Programs").join("Gpg4win").join("bin"),
                local_app_data
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links"),
            ],
        );
    }

    if let Some(program_data) = program_data.filter(|value| !value.is_empty()) {
        append_unique_paths(
            &mut directories,
            [PathBuf::from(program_data).join("chocolatey").join("bin")],
        );
    }

    if let Some(user_profile) = user_profile.filter(|value| !value.is_empty()) {
        let user_profile = PathBuf::from(user_profile);
        append_unique_paths(&mut directories, [user_profile.join("scoop").join("shims")]);
    }

    if directories.is_empty() {
        return None;
    }

    join_paths(directories).ok()
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
pub(crate) fn resolve_program_in_search_path(
    program: &str,
    search_path: &OsStr,
) -> Option<PathBuf> {
    let program_path = Path::new(program);
    if program_path.is_absolute() || program_path.components().count() > 1 {
        return is_spawnable_path(program_path).then(|| program_path.to_path_buf());
    }

    split_paths(search_path)
        .map(|directory| directory.join(program))
        .find(|candidate| is_spawnable_path(candidate.as_path()))
}

#[cfg(target_os = "windows")]
fn resolve_windows_command_path_from_env(
    command_name: &Path,
    path_var: Option<OsString>,
    pathext_var: Option<OsString>,
) -> Option<PathBuf> {
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
fn windows_command_candidate_names(
    command_name: &OsStr,
    pathext_var: Option<&OsStr>,
) -> Vec<OsString> {
    let command_path = Path::new(command_name);
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

fn append_unique_paths<I>(directories: &mut Vec<PathBuf>, candidates: I)
where
    I: IntoIterator<Item = PathBuf>,
{
    for candidate in candidates {
        if candidate.as_os_str().is_empty() {
            continue;
        }

        if directories.iter().any(|existing| existing == &candidate) {
            continue;
        }

        directories.push(candidate);
    }
}

#[cfg(target_os = "windows")]
fn windows_path_is_spawnable(path: &Path) -> bool {
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
fn windows_file_has_mz_header(path: &Path) -> bool {
    use std::io::Read as _;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 2];
    file.read_exact(&mut header).is_ok() && header == *b"MZ"
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
fn is_spawnable_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}
