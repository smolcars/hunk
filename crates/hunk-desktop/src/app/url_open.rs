use std::process::Command;

use anyhow::{Context as _, Result, anyhow};

pub(crate) fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        return spawn_background_command({
            let mut command = Command::new("open");
            command.arg(url);
            command
        })
        .with_context(|| format!("failed to open URL '{url}' via macOS browser opener"));
    }

    #[cfg(target_os = "linux")]
    {
        if let Err(portal_error) = open_url_via_linux_portal(url) {
            return spawn_background_command({
                let mut command = Command::new("xdg-open");
                command.arg(url);
                command
            })
            .with_context(|| {
                format!(
                    "failed to open URL '{url}' via Linux opener after portal error: {portal_error:#}"
                )
            });
        }

        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        return spawn_background_command({
            let mut command = Command::new("cmd");
            command.args(["/C", "start", "", url]);
            configure_background_windows_command(&mut command);
            command
        })
        .with_context(|| format!("failed to open URL '{url}' via Windows browser opener"));
    }

    #[allow(unreachable_code)]
    Err(anyhow!("opening URLs is not supported on this platform"))
}

fn spawn_background_command(mut command: Command) -> Result<()> {
    let status = command.status().context("failed to spawn browser opener")?;
    if status.success() {
        return Ok(());
    }

    Err(anyhow!("browser opener exited with status {status}"))
}

#[cfg(target_os = "linux")]
fn open_url_via_linux_portal(url: &str) -> Result<()> {
    let uri = ashpd::Uri::parse(url).with_context(|| format!("failed to parse URL '{url}'"))?;
    pollster::block_on(async {
        ashpd::desktop::open_uri::OpenFileRequest::default()
            .send_uri(&uri)
            .await
            .and_then(|request| request.response())
    })
    .with_context(|| format!("failed to open URL '{url}' via XDG portal"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_background_windows_command(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}
