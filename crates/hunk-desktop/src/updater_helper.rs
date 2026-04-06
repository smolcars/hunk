use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use hunk_updater::AssetFormat;

const APPLY_STAGED_UPDATE_ARG: &str = "--apply-staged-update";
const WAIT_PID_ARG: &str = "--wait-pid";
const PACKAGE_ARG: &str = "--package";
const FORMAT_ARG: &str = "--asset-format";
const UPDATE_HELPER_WAIT_TIMEOUT: Duration = Duration::from_secs(90);

pub(crate) fn maybe_handle_updater_helper_mode() -> Result<bool> {
    let mut args = std::env::args_os();
    let _ = args.next();

    match args.next() {
        Some(flag) if flag == OsStr::new(APPLY_STAGED_UPDATE_ARG) => {
            handle_apply_staged_update_helper(args)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn handle_apply_staged_update_helper(args: impl Iterator<Item = OsString>) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let _ = args;
        bail!("updater helper mode is not supported on Windows")
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut wait_pid = None;
        let mut package_path = None;
        let mut asset_format = None;
        let mut pending_flag: Option<String> = None;

        for arg in args {
            if let Some(flag) = pending_flag.take() {
                match flag.as_str() {
                    WAIT_PID_ARG => {
                        let value = arg
                            .to_str()
                            .ok_or_else(|| anyhow!("wait pid must be valid utf-8"))?;
                        wait_pid = Some(
                            value
                                .parse::<u32>()
                                .with_context(|| format!("invalid wait pid `{value}`"))?,
                        );
                    }
                    PACKAGE_ARG => package_path = Some(PathBuf::from(arg)),
                    FORMAT_ARG => {
                        let value = arg
                            .to_str()
                            .ok_or_else(|| anyhow!("asset format must be valid utf-8"))?;
                        asset_format = Some(value.parse::<AssetFormat>()?);
                    }
                    _ => bail!("unsupported updater helper flag `{flag}`"),
                }
                continue;
            }

            let flag = arg
                .to_str()
                .ok_or_else(|| anyhow!("updater helper flag must be valid utf-8"))?;
            match flag {
                WAIT_PID_ARG | PACKAGE_ARG | FORMAT_ARG => pending_flag = Some(flag.to_string()),
                other => bail!("unsupported updater helper argument `{other}`"),
            }
        }

        if let Some(flag) = pending_flag {
            bail!("missing value for updater helper flag `{flag}`");
        }

        let wait_pid = wait_pid
            .ok_or_else(|| anyhow!("missing required updater helper flag `{WAIT_PID_ARG}`"))?;
        let package_path = package_path
            .ok_or_else(|| anyhow!("missing required updater helper flag `{PACKAGE_ARG}`"))?;
        let asset_format = asset_format
            .ok_or_else(|| anyhow!("missing required updater helper flag `{FORMAT_ARG}`"))?;

        hunk_updater::wait_for_process_to_exit(wait_pid, UPDATE_HELPER_WAIT_TIMEOUT)?;
        let current_executable =
            std::env::current_exe().context("resolve updater helper executable path")?;
        let applied_update = hunk_updater::apply_staged_update_from_current_executable(
            current_executable.as_path(),
            package_path.as_path(),
            asset_format,
        )?;
        launch_updated_app(applied_update.relaunch_executable.as_path())?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn launch_updated_app(relaunch_executable: &Path) -> Result<()> {
    if let Some(app_path) = relaunch_executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .filter(|candidate| candidate.extension().is_some_and(|value| value == "app"))
    {
        Command::new("open")
            .arg("-n")
            .arg("-a")
            .arg(app_path)
            .spawn()
            .with_context(|| format!("launch updated app bundle {}", app_path.display()))?;
        return Ok(());
    }

    Command::new(relaunch_executable).spawn().with_context(|| {
        format!(
            "launch updated executable {}",
            relaunch_executable.display()
        )
    })?;
    Ok(())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn launch_updated_app(relaunch_executable: &Path) -> Result<()> {
    Command::new(relaunch_executable).spawn().with_context(|| {
        format!(
            "launch updated executable {}",
            relaunch_executable.display()
        )
    })?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn launch_updated_app(relaunch_executable: &Path) -> Result<()> {
    let _ = relaunch_executable;
    bail!("launch_updated_app should not be called on Windows")
}
