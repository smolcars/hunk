use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
#[cfg(not(target_os = "windows"))]
use std::io::IsTerminal as _;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(not(target_os = "windows"))]
use std::process::Command;

use anyhow::{Context, Result, bail};
use hunk_domain::config::{AppConfig, TerminalConfig, TerminalShell};

pub(crate) const PRINT_TERMINAL_ENV_ARG: &str = "--print-terminal-env-json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalShellFamily {
    Posix,
    PowerShell,
    Cmd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTerminalShell {
    program: OsString,
    args: Vec<OsString>,
    family: TerminalShellFamily,
    label: String,
    custom_args: bool,
}

impl ResolvedTerminalShell {
    pub(crate) fn program(&self) -> &OsStr {
        self.program.as_os_str()
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn family(&self) -> TerminalShellFamily {
        self.family
    }

    pub(crate) fn label(&self) -> &str {
        self.label.as_str()
    }

    pub(crate) fn interactive_shell_args(&self, inherit_login_environment: bool) -> Vec<OsString> {
        if self.custom_args {
            return self.args.clone();
        }

        match self.family {
            TerminalShellFamily::Posix => {
                let mut args = Vec::new();
                if inherit_login_environment {
                    args.push(OsString::from("-l"));
                }
                args.push(OsString::from("-i"));
                args
            }
            TerminalShellFamily::PowerShell => {
                let mut args = vec![OsString::from("-NoLogo")];
                if !inherit_login_environment {
                    args.push(OsString::from("-NoProfile"));
                }
                args
            }
            TerminalShellFamily::Cmd => Vec::new(),
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn supports_environment_capture(&self) -> bool {
        !self.custom_args
    }
}

pub(crate) fn maybe_handle_terminal_env_helper_mode() -> Result<bool> {
    let mut args = std::env::args_os();
    let _ = args.next();
    match args.next() {
        Some(flag) if flag == OsStr::new(PRINT_TERMINAL_ENV_ARG) && args.next().is_none() => {
            print_current_process_environment_json()?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(crate) fn maybe_hydrate_app_environment(config: &AppConfig) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let _ = config;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        if !config.terminal.hydrate_app_environment_on_launch || std::io::stdout().is_terminal() {
            return Ok(());
        }

        let cwd = dirs::home_dir()
            .or_else(|| std::env::current_dir().ok())
            .context("resolve startup directory for terminal environment hydration")?;
        let shell = resolve_terminal_shell(&config.terminal);
        if !shell.supports_environment_capture() {
            return Ok(());
        }

        let environment = capture_shell_environment(
            &shell,
            cwd.as_path(),
            config.terminal.inherit_login_environment,
        )
        .context("capture login shell environment for app launch")?;
        for (key, value) in environment {
            if key == "SHLVL" {
                continue;
            }
            // SAFETY: Hunk performs this once during startup before launching background work.
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Ok(())
    }
}

pub(crate) fn resolve_terminal_shell(config: &TerminalConfig) -> ResolvedTerminalShell {
    match &config.shell {
        TerminalShell::System => {
            let program = system_shell_program();
            build_resolved_terminal_shell(program, Vec::new(), false)
        }
        TerminalShell::Program(program) if !program.trim().is_empty() => {
            build_resolved_terminal_shell(OsString::from(program), Vec::new(), false)
        }
        TerminalShell::WithArguments { program, args } if !program.trim().is_empty() => {
            build_resolved_terminal_shell(
                OsString::from(program),
                args.iter().cloned().map(OsString::from).collect(),
                true,
            )
        }
        _ => {
            let program = system_shell_program();
            build_resolved_terminal_shell(program, Vec::new(), false)
        }
    }
}

pub(crate) fn terminal_shell_label(config: &TerminalConfig) -> String {
    resolve_terminal_shell(config).label().to_string()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn capture_shell_environment(
    shell: &ResolvedTerminalShell,
    cwd: &Path,
    inherit_login_environment: bool,
) -> Result<BTreeMap<String, String>> {
    if !shell.supports_environment_capture() {
        bail!("terminal environment capture does not support custom shell arguments yet");
    }

    let helper = std::env::current_exe().context("resolve hunk executable for env helper mode")?;
    let mut command = Command::new(shell.program());
    command.current_dir(cwd);

    match shell.family() {
        TerminalShellFamily::Posix => {
            if inherit_login_environment {
                command.arg("-l");
            }
            command.arg("-i");
            command.arg("-c");
            let helper_path = quote_posix(helper.as_os_str());
            command.arg(format!("{helper_path} {}", PRINT_TERMINAL_ENV_ARG));
        }
        TerminalShellFamily::PowerShell => {
            command.arg("-NoLogo");
            if !inherit_login_environment {
                command.arg("-NoProfile");
            }
            command.arg("-Command");
            let helper_path = quote_powershell(helper.as_os_str());
            command.arg(format!("& {helper_path} {}", PRINT_TERMINAL_ENV_ARG));
        }
        TerminalShellFamily::Cmd => {
            command.arg("/D");
            command.arg("/C");
            command.arg(format!(
                "\"{}\" {}",
                helper.display(),
                PRINT_TERMINAL_ENV_ARG
            ));
        }
    }

    let output = command
        .output()
        .with_context(|| format!("run terminal environment helper with {:?}", shell.program()))?;
    if !output.status.success() {
        bail!(
            "terminal environment helper exited with {}. stdout: {} stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut environment: BTreeMap<String, String> =
        serde_json::from_slice(output.stdout.as_slice())
            .context("parse terminal environment helper output")?;
    if let Some(path) = environment.remove("Path") {
        environment.insert("PATH".to_string(), path);
    }
    Ok(environment)
}

fn print_current_process_environment_json() -> Result<()> {
    let environment: BTreeMap<String, String> = std::env::vars().collect();
    println!(
        "{}",
        serde_json::to_string(&environment).context("serialize environment to json")?
    );
    Ok(())
}

fn build_resolved_terminal_shell(
    program: OsString,
    args: Vec<OsString>,
    custom_args: bool,
) -> ResolvedTerminalShell {
    let label = Path::new(program.as_os_str())
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("shell")
        .to_string();
    let family = shell_family_from_program(program.as_os_str());

    ResolvedTerminalShell {
        program,
        args,
        family,
        label,
        custom_args,
    }
}

fn shell_family_from_program(program: &OsStr) -> TerminalShellFamily {
    let program_name = Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match program_name.as_str() {
        "cmd" | "cmd.exe" => TerminalShellFamily::Cmd,
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => TerminalShellFamily::PowerShell,
        _ => TerminalShellFamily::Posix,
    }
}

#[cfg(target_os = "windows")]
fn system_shell_program() -> OsString {
    windows_preferred_shells()
        .into_iter()
        .next()
        .map(|candidate| candidate.into_os_string())
        .or_else(|| find_program_on_windows_path("pwsh.exe"))
        .or_else(|| find_program_on_windows_path("powershell.exe"))
        .unwrap_or_else(|| OsString::from("cmd.exe"))
}

#[cfg(target_os = "windows")]
fn windows_preferred_shells() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    for base in [
        std::env::var_os("ProgramFiles"),
        std::env::var_os("ProgramFiles(x86)"),
    ]
    .into_iter()
    .flatten()
    {
        let base = PathBuf::from(base);
        if let Ok(entries) = std::fs::read_dir(base.join("PowerShell")) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("pwsh.exe");
                if candidate.exists() {
                    candidates.push(candidate);
                }
            }
        }
    }

    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let windows_apps = PathBuf::from(local_app_data)
            .join("Microsoft")
            .join("WindowsApps");
        if let Ok(entries) = std::fs::read_dir(&windows_apps) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("pwsh.exe");
                if candidate.exists() {
                    candidates.push(candidate);
                }
            }
        }
    }

    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let scoop = PathBuf::from(user_profile)
            .join("scoop")
            .join("shims")
            .join("pwsh.exe");
        if scoop.exists() {
            candidates.push(scoop);
        }
    }

    candidates
}

#[cfg(target_os = "windows")]
fn find_program_on_windows_path(program: &str) -> Option<OsString> {
    let path_var = std::env::var_os("PATH")?;
    let pathext = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter(|entry| !entry.is_empty())
                .map(|entry| entry.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![".exe".to_string(), ".cmd".to_string(), ".bat".to_string()]);

    for directory in std::env::split_paths(&path_var) {
        let candidate = directory.join(program);
        if candidate.exists() {
            return Some(candidate.into_os_string());
        }

        let lower = program.to_ascii_lowercase();
        if !lower.ends_with(".exe") && !lower.ends_with(".cmd") && !lower.ends_with(".bat") {
            for extension in &pathext {
                let candidate = directory.join(format!("{program}{extension}"));
                if candidate.exists() {
                    return Some(candidate.into_os_string());
                }
            }
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
fn system_shell_program() -> OsString {
    if let Some(shell) = std::env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .filter(|shell| Path::new(shell).exists())
    {
        return shell;
    }

    unix_fallback_shells()
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("/bin/sh"))
}

#[cfg(not(target_os = "windows"))]
fn unix_fallback_shells() -> [&'static str; 3] {
    ["/bin/bash", "/bin/zsh", "/bin/sh"]
}

#[cfg(not(target_os = "windows"))]
fn quote_posix(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(not(target_os = "windows"))]
fn quote_powershell(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::{
        TerminalShellFamily, build_resolved_terminal_shell, resolve_terminal_shell,
        shell_family_from_program,
    };
    use hunk_domain::config::{TerminalConfig, TerminalShell};
    use std::ffi::OsString;

    #[test]
    fn explicit_program_shell_resolution_preserves_program_and_defaults_args() {
        let config = TerminalConfig {
            shell: TerminalShell::Program("/bin/zsh".to_string()),
            ..TerminalConfig::default()
        };

        let resolved = resolve_terminal_shell(&config);

        assert_eq!(resolved.program(), "/bin/zsh");
        assert!(resolved.args.is_empty());
        assert_eq!(resolved.label(), "zsh");
    }

    #[test]
    fn explicit_shell_args_are_preserved() {
        let resolved = build_resolved_terminal_shell(
            OsString::from("pwsh.exe"),
            vec![OsString::from("-NoLogo")],
            true,
        );

        assert_eq!(resolved.args, vec![OsString::from("-NoLogo")]);
        assert_eq!(
            resolved.interactive_shell_args(true),
            vec![OsString::from("-NoLogo")]
        );
    }

    #[test]
    fn powershell_interactive_args_honor_profile_opt_out() {
        let resolved = build_resolved_terminal_shell(OsString::from("pwsh.exe"), Vec::new(), false);

        assert_eq!(
            resolved.interactive_shell_args(true),
            vec![OsString::from("-NoLogo")]
        );
        assert_eq!(
            resolved.interactive_shell_args(false),
            vec![OsString::from("-NoLogo"), OsString::from("-NoProfile")]
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn unix_fallback_shells_keep_bash_ahead_of_zsh() {
        assert_eq!(super::unix_fallback_shells()[0], "/bin/bash");
        assert_eq!(super::unix_fallback_shells()[1], "/bin/zsh");
    }

    #[test]
    fn shell_family_detection_handles_windows_shells() {
        assert_eq!(
            shell_family_from_program(OsString::from("pwsh.exe").as_os_str()),
            TerminalShellFamily::PowerShell
        );
        assert_eq!(
            shell_family_from_program(OsString::from("cmd.exe").as_os_str()),
            TerminalShellFamily::Cmd
        );
    }
}
