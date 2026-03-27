use std::env;
#[cfg(target_os = "windows")]
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, anyhow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProjectOpenTargetId {
    VsCode,
    Cursor,
    Zed,
    Xcode,
    AndroidStudio,
    FileManager,
}

const PROJECT_OPEN_TARGET_ORDER: [ProjectOpenTargetId; 6] = [
    ProjectOpenTargetId::VsCode,
    ProjectOpenTargetId::Cursor,
    ProjectOpenTargetId::Zed,
    ProjectOpenTargetId::Xcode,
    ProjectOpenTargetId::AndroidStudio,
    ProjectOpenTargetId::FileManager,
];

impl ProjectOpenTargetId {
    pub(crate) const fn storage_key(self) -> &'static str {
        match self {
            Self::VsCode => "vscode",
            Self::Cursor => "cursor",
            Self::Zed => "zed",
            Self::Xcode => "xcode",
            Self::AndroidStudio => "android-studio",
            Self::FileManager => "file-manager",
        }
    }

    pub(crate) fn display_label(self) -> &'static str {
        match self {
            Self::VsCode => "VS Code",
            Self::Cursor => "Cursor",
            Self::Zed => "Zed",
            Self::Xcode => "Xcode",
            Self::AndroidStudio => "Android Studio",
            Self::FileManager => {
                if cfg!(target_os = "macos") {
                    "Finder"
                } else if cfg!(target_os = "windows") {
                    "Explorer"
                } else {
                    "Files"
                }
            }
        }
    }

    pub(crate) fn from_storage_key(value: &str) -> Option<Self> {
        PROJECT_OPEN_TARGET_ORDER
            .iter()
            .copied()
            .find(|target| target.storage_key() == value)
    }
}

pub(crate) fn resolve_available_project_open_targets() -> Vec<ProjectOpenTargetId> {
    PROJECT_OPEN_TARGET_ORDER
        .iter()
        .copied()
        .filter(|target| is_project_open_target_available(*target))
        .collect()
}

pub(crate) fn resolve_preferred_project_open_target(
    available: &[ProjectOpenTargetId],
    stored: Option<&str>,
) -> Option<ProjectOpenTargetId> {
    let stored_target = stored.and_then(ProjectOpenTargetId::from_storage_key);
    stored_target
        .filter(|target| available.contains(target))
        .or_else(|| available.first().copied())
}

pub(crate) fn open_path_in_project_target(path: &Path, target: ProjectOpenTargetId) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("path does not exist: {}", path.display()));
    }

    #[cfg(target_os = "macos")]
    {
        open_path_in_target_macos(path, target)
    }

    #[cfg(target_os = "windows")]
    {
        return open_path_in_target_windows(path, target);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        open_path_in_target_linux(path, target)
    }
}

fn is_project_open_target_available(target: ProjectOpenTargetId) -> bool {
    #[cfg(target_os = "macos")]
    {
        is_project_open_target_available_macos(target)
    }

    #[cfg(target_os = "windows")]
    {
        return resolve_windows_launch_command(target).is_some();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        resolve_linux_launch_command(target).is_some()
    }
}

#[cfg(target_os = "macos")]
fn is_project_open_target_available_macos(target: ProjectOpenTargetId) -> bool {
    if target == ProjectOpenTargetId::FileManager {
        return true;
    }

    if resolve_first_command_path(macos_cli_commands(target)).is_some() {
        return true;
    }

    macos_app_aliases(target)
        .iter()
        .any(|alias| macos_can_open_application(alias))
}

#[cfg(target_os = "macos")]
fn open_path_in_target_macos(path: &Path, target: ProjectOpenTargetId) -> Result<()> {
    if target == ProjectOpenTargetId::FileManager {
        let mut command = Command::new("open");
        command.args(["-a", "Finder"]).arg(path);
        return spawn_background_command(command)
            .with_context(|| format!("failed to open {} on macOS", target.display_label()));
    }

    if let Some(command_path) = resolve_first_command_path(macos_cli_commands(target)) {
        let mut command = Command::new(command_path);
        command.arg(path);
        return spawn_background_command(command)
            .with_context(|| format!("failed to open {} on macOS", target.display_label()));
    }

    let mut command = Command::new("open");
    let app_alias = macos_app_aliases(target)
        .iter()
        .copied()
        .find(|alias| macos_can_open_application(alias))
        .ok_or_else(|| anyhow!("{} is not available on this system", target.display_label()))?;
    command.args(["-a", app_alias]);
    command.arg(path);

    spawn_background_command(command)
        .with_context(|| format!("failed to open {} on macOS", target.display_label()))
}

#[cfg(target_os = "macos")]
fn macos_cli_commands(target: ProjectOpenTargetId) -> &'static [&'static str] {
    match target {
        ProjectOpenTargetId::VsCode => &["code"],
        ProjectOpenTargetId::Cursor => &["cursor"],
        ProjectOpenTargetId::Zed => &["zed"],
        ProjectOpenTargetId::Xcode => &[],
        ProjectOpenTargetId::AndroidStudio => &["studio", "studio.sh"],
        ProjectOpenTargetId::FileManager => &[],
    }
}

#[cfg(target_os = "macos")]
fn macos_app_aliases(target: ProjectOpenTargetId) -> &'static [&'static str] {
    match target {
        ProjectOpenTargetId::VsCode => &["Visual Studio Code"],
        ProjectOpenTargetId::Cursor => &["Cursor"],
        ProjectOpenTargetId::Zed => &["Zed", "Zed Preview"],
        ProjectOpenTargetId::Xcode => &["Xcode"],
        ProjectOpenTargetId::AndroidStudio => &["Android Studio"],
        ProjectOpenTargetId::FileManager => &["Finder"],
    }
}

#[cfg(target_os = "macos")]
fn macos_can_open_application(alias: &str) -> bool {
    let mut command = Command::new("open");
    command
        .args(["-Ra", alias])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.status().is_ok_and(|status| status.success())
}

#[cfg(target_os = "windows")]
fn open_path_in_target_windows(path: &Path, target: ProjectOpenTargetId) -> Result<()> {
    let Some(command_path) = resolve_windows_launch_command(target) else {
        return Err(anyhow!(
            "{} is not available on this system",
            target.display_label()
        ));
    };

    if target == ProjectOpenTargetId::FileManager {
        let mut command = Command::new(command_path);
        command.arg(path);
        configure_background_windows_command(&mut command);
        return spawn_background_command(command)
            .with_context(|| format!("failed to open {}", target.display_label()));
    }

    let extension = command_path
        .extension()
        .and_then(OsStr::to_str)
        .map(|value| value.to_ascii_lowercase());
    if matches!(extension.as_deref(), Some("cmd" | "bat")) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(command_path).arg(path);
        configure_background_windows_command(&mut command);
        return spawn_background_command(command)
            .with_context(|| format!("failed to open {}", target.display_label()));
    }

    let mut command = Command::new(command_path);
    command.arg(path);
    configure_background_windows_command(&mut command);
    spawn_background_command(command)
        .with_context(|| format!("failed to open {}", target.display_label()))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_path_in_target_linux(path: &Path, target: ProjectOpenTargetId) -> Result<()> {
    let Some(command_path) = resolve_linux_launch_command(target) else {
        return Err(anyhow!(
            "{} is not available on this system",
            target.display_label()
        ));
    };

    let mut command = Command::new(command_path);
    command.arg(path);
    spawn_background_command(command)
        .with_context(|| format!("failed to open {}", target.display_label()))
}

#[cfg(target_os = "windows")]
fn resolve_windows_launch_command(target: ProjectOpenTargetId) -> Option<PathBuf> {
    match target {
        ProjectOpenTargetId::VsCode => resolve_windows_command(
            &["code", "code.cmd", "Code.exe"],
            &[
                windows_env_path(
                    "LOCALAPPDATA",
                    &["Programs", "Microsoft VS Code", "Code.exe"],
                ),
                windows_env_path("PROGRAMFILES", &["Microsoft VS Code", "Code.exe"]),
                windows_env_path("PROGRAMFILES(X86)", &["Microsoft VS Code", "Code.exe"]),
            ],
        ),
        ProjectOpenTargetId::Cursor => resolve_windows_command(
            &["cursor", "cursor.cmd", "Cursor.exe"],
            &[windows_env_path(
                "LOCALAPPDATA",
                &["Programs", "Cursor", "Cursor.exe"],
            )],
        ),
        ProjectOpenTargetId::Zed => resolve_windows_command(
            &["zed", "zed.cmd", "Zed.exe"],
            &[windows_env_path(
                "LOCALAPPDATA",
                &["Programs", "Zed", "Zed.exe"],
            )],
        ),
        ProjectOpenTargetId::Xcode => None,
        ProjectOpenTargetId::AndroidStudio => resolve_windows_command(
            &["studio64.exe", "studio.exe", "studio.cmd"],
            &[
                windows_env_path(
                    "PROGRAMFILES",
                    &["Android", "Android Studio", "bin", "studio64.exe"],
                ),
                windows_env_path(
                    "PROGRAMFILES",
                    &["Android", "Android Studio", "bin", "studio.exe"],
                ),
                windows_env_path(
                    "PROGRAMFILES(X86)",
                    &["Android", "Android Studio", "bin", "studio64.exe"],
                ),
                windows_env_path(
                    "LOCALAPPDATA",
                    &["Programs", "Android Studio", "bin", "studio64.exe"],
                ),
            ],
        ),
        ProjectOpenTargetId::FileManager => Some(windows_explorer_command()),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn resolve_linux_launch_command(target: ProjectOpenTargetId) -> Option<PathBuf> {
    match target {
        ProjectOpenTargetId::VsCode => resolve_first_command_path(&["code"]),
        ProjectOpenTargetId::Cursor => resolve_first_command_path(&["cursor"]),
        ProjectOpenTargetId::Zed => resolve_first_command_path(&["zed"]),
        ProjectOpenTargetId::Xcode => None,
        ProjectOpenTargetId::AndroidStudio => resolve_linux_android_studio_command(),
        ProjectOpenTargetId::FileManager => resolve_first_command_path(&["xdg-open"]),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn resolve_linux_android_studio_command() -> Option<PathBuf> {
    resolve_first_command_path(&["studio.sh", "studio", "android-studio"]).or_else(|| {
        let home = env::var_os("HOME").map(PathBuf::from);
        [
            Some(PathBuf::from("/opt/android-studio/bin/studio.sh")),
            home.as_ref()
                .map(|base| base.join("android-studio").join("bin").join("studio.sh")),
        ]
        .into_iter()
        .flatten()
        .find(|path| is_executable_file(path.as_path()))
    })
}

#[cfg(target_os = "windows")]
fn resolve_windows_command(
    commands: &[&str],
    known_locations: &[Option<PathBuf>],
) -> Option<PathBuf> {
    resolve_first_command_path(commands).or_else(|| {
        known_locations
            .iter()
            .flatten()
            .find(|path| path.is_file())
            .cloned()
    })
}

#[cfg(target_os = "windows")]
fn windows_env_path(name: &str, segments: &[&str]) -> Option<PathBuf> {
    let mut path = PathBuf::from(env::var_os(name)?);
    for segment in segments {
        path.push(segment);
    }
    Some(path)
}

#[cfg(target_os = "windows")]
fn windows_explorer_command() -> PathBuf {
    windows_env_path("WINDIR", &["explorer.exe"]).unwrap_or_else(|| PathBuf::from("explorer.exe"))
}

fn resolve_first_command_path(commands: &[&str]) -> Option<PathBuf> {
    commands
        .iter()
        .find_map(|command| resolve_command_path(command))
}

fn resolve_command_path(command: &str) -> Option<PathBuf> {
    if command.contains('/') || command.contains('\\') {
        return executable_candidates_for_command(command)
            .into_iter()
            .find(|candidate| is_executable_file(candidate));
    }

    let path_value = env::var_os("PATH")?;
    for entry in env::split_paths(&path_value) {
        if entry.as_os_str().is_empty() {
            continue;
        }
        for candidate in executable_candidates_for_command(command) {
            let candidate_path = entry.join(candidate);
            if is_executable_file(candidate_path.as_path()) {
                return Some(candidate_path);
            }
        }
    }

    None
}

fn executable_candidates_for_command(command: &str) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let path = PathBuf::from(command);
        let extension = path
            .extension()
            .and_then(OsStr::to_str)
            .map(|value| value.to_ascii_uppercase());
        let path_exts = env::var("PATHEXT")
            .ok()
            .map(|value| {
                value
                    .split(';')
                    .filter_map(|segment| {
                        let trimmed = segment.trim();
                        (!trimmed.is_empty()).then(|| trimmed.to_ascii_uppercase())
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|items| !items.is_empty())
            .unwrap_or_else(|| {
                vec![
                    ".COM".to_string(),
                    ".EXE".to_string(),
                    ".BAT".to_string(),
                    ".CMD".to_string(),
                ]
            });

        if extension
            .as_deref()
            .is_some_and(|extension| path_exts.iter().any(|item| item == extension))
        {
            return vec![path];
        }

        return path_exts
            .into_iter()
            .map(|extension| {
                let normalized = extension.trim_start_matches('.');
                PathBuf::from(format!("{command}.{normalized}"))
            })
            .collect();
    }

    #[cfg(not(target_os = "windows"))]
    {
        vec![PathBuf::from(command)]
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(target_os = "windows")]
    {
        path.extension()
            .and_then(OsStr::to_str)
            .map(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "exe" | "cmd" | "bat" | "com"
                )
            })
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & 0o111 != 0
    }
}

fn spawn_background_command(mut command: Command) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .context("failed to spawn background process")?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_background_windows_command(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(test)]
mod tests {
    use super::{ProjectOpenTargetId, resolve_preferred_project_open_target};

    #[test]
    fn preferred_project_open_target_uses_stored_value_when_available() {
        let available = [
            ProjectOpenTargetId::VsCode,
            ProjectOpenTargetId::Zed,
            ProjectOpenTargetId::FileManager,
        ];

        let target = resolve_preferred_project_open_target(available.as_slice(), Some("zed"));

        assert_eq!(target, Some(ProjectOpenTargetId::Zed));
    }

    #[test]
    fn preferred_project_open_target_falls_back_to_first_available_value() {
        let available = [
            ProjectOpenTargetId::VsCode,
            ProjectOpenTargetId::FileManager,
        ];

        let target =
            resolve_preferred_project_open_target(available.as_slice(), Some("android-studio"));

        assert_eq!(target, Some(ProjectOpenTargetId::VsCode));
    }

    #[test]
    fn storage_keys_round_trip() {
        for target in [
            ProjectOpenTargetId::VsCode,
            ProjectOpenTargetId::Cursor,
            ProjectOpenTargetId::Zed,
            ProjectOpenTargetId::Xcode,
            ProjectOpenTargetId::AndroidStudio,
            ProjectOpenTargetId::FileManager,
        ] {
            assert_eq!(
                ProjectOpenTargetId::from_storage_key(target.storage_key()),
                Some(target)
            );
        }
    }
}
