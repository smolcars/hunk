#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod terminal_env;
mod updater_helper;

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use hunk_domain::config::{AppConfig, ConfigStore};
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

static SIGNAL_SHUTDOWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    if terminal_env::maybe_handle_terminal_env_helper_mode()? {
        return Ok(());
    }
    if updater_helper::maybe_handle_updater_helper_mode()? {
        return Ok(());
    }
    run_with_platform_stack_workaround()
}

#[cfg(all(target_os = "windows", debug_assertions))]
fn run_with_platform_stack_workaround() -> Result<()> {
    const WINDOWS_DEBUG_STACK_SIZE: usize = 8 * 1024 * 1024;
    let thread = std::thread::Builder::new()
        .name("hunk-main".to_string())
        .stack_size(WINDOWS_DEBUG_STACK_SIZE)
        .spawn(run_app)
        .context("failed to start hunk main thread with enlarged stack")?;

    match thread.join() {
        Ok(run_result) => run_result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(not(all(target_os = "windows", debug_assertions)))]
fn run_with_platform_stack_workaround() -> Result<()> {
    run_app()
}

fn run_app() -> Result<()> {
    ensure_hidden_windows_console();
    let config = load_startup_config();
    if let Err(error) = terminal_env::maybe_hydrate_app_environment(&config) {
        eprintln!("failed to hydrate terminal environment: {error:#}");
    }

    let default_log_level = if cfg!(debug_assertions) {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let env_filter = EnvFilter::builder()
        .with_default_directive(default_log_level.into())
        .from_env_lossy()
        .add_directive("jj_lib::gpg_signing=warn".parse()?)
        .add_directive("jj_lib::lock::unix=warn".parse()?)
        .add_directive("html5ever=warn".parse()?)
        .add_directive("markup5ever=warn".parse()?);

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .without_time()
        .init();

    log_linux_compositor_selection();
    install_process_signal_cleanup()?;

    app::run()
}

#[cfg(target_os = "windows")]
fn ensure_hidden_windows_console() {
    use windows_sys::Win32::System::Console::{AllocConsole, GetConsoleWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    unsafe {
        if !GetConsoleWindow().is_null() {
            return;
        }

        if AllocConsole() == 0 {
            return;
        }

        let console_window = GetConsoleWindow();
        if !console_window.is_null() {
            ShowWindow(console_window, SW_HIDE);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_hidden_windows_console() {}

fn load_startup_config() -> AppConfig {
    ConfigStore::new()
        .ok()
        .and_then(|store| store.load_or_create_default().ok())
        .unwrap_or_default()
}

fn install_process_signal_cleanup() -> Result<()> {
    ctrlc::set_handler(|| {
        if SIGNAL_SHUTDOWN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
            return;
        }

        hunk_codex::host::begin_host_shutdown();
        hunk_codex::host::cleanup_tracked_hosts_for_shutdown();
        std::process::exit(130);
    })
    .context("failed to install process signal cleanup handler")
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn log_linux_compositor_selection() {
    let has_wayland_display = std::env::var_os("WAYLAND_DISPLAY")
        .as_ref()
        .is_some_and(|display| !display.is_empty());
    let has_x11_display = std::env::var_os("DISPLAY")
        .as_ref()
        .is_some_and(|display| !display.is_empty());

    tracing::info!(
        compositor = gpui::guess_compositor(),
        has_wayland_display,
        has_x11_display,
        "delegating Linux compositor selection to GPUI"
    );
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn log_linux_compositor_selection() {}
