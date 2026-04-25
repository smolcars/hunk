#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod terminal_env;
mod updater_helper;

use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::{Context, Result};
use hunk_domain::config::{AppConfig, ConfigStore};
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

const SIGNAL_SHUTDOWN_IDLE: u8 = 0;
const SIGNAL_SHUTDOWN_REQUESTED: u8 = 1;
const SIGNAL_SHUTDOWN_FORCED: u8 = 2;

static SIGNAL_SHUTDOWN_STATE: AtomicU8 = AtomicU8::new(SIGNAL_SHUTDOWN_IDLE);

fn main() -> Result<()> {
    codex_utils_rustls_provider::ensure_rustls_crypto_provider();

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
    ensure_valid_process_current_dir();
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

    app::run()?;
    if process_signal_shutdown_requested() {
        std::process::exit(130);
    }
    Ok(())
}

fn ensure_valid_process_current_dir() {
    if std::env::current_dir().is_ok() {
        return;
    }

    let fallback = dirs::home_dir()
        .filter(|path| path.is_dir())
        .or_else(|| {
            let temp_dir = std::env::temp_dir();
            temp_dir.is_dir().then_some(temp_dir)
        })
        .or_else(|| {
            #[cfg(unix)]
            {
                Some(std::path::PathBuf::from("/"))
            }
            #[cfg(not(unix))]
            {
                None
            }
        });

    let Some(fallback) = fallback else {
        eprintln!("failed to recover from invalid process working directory");
        return;
    };

    if let Err(error) = std::env::set_current_dir(fallback.as_path()) {
        eprintln!(
            "failed to recover from invalid process working directory using {}: {error:#}",
            fallback.display()
        );
    }
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

pub(crate) fn install_process_signal_cleanup() -> Result<()> {
    ctrlc::set_handler(|| {
        match SIGNAL_SHUTDOWN_STATE.compare_exchange(
            SIGNAL_SHUTDOWN_IDLE,
            SIGNAL_SHUTDOWN_REQUESTED,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => {
                eprintln!("received interrupt, shutting down Hunk...");
            }
            Err(SIGNAL_SHUTDOWN_REQUESTED) => {
                eprintln!("received second interrupt, forcing Hunk to exit");
                SIGNAL_SHUTDOWN_STATE.store(SIGNAL_SHUTDOWN_FORCED, Ordering::SeqCst);
                std::process::exit(130);
            }
            Err(_) => {}
        }
    })
    .context("failed to install process signal cleanup handler")
}

pub(crate) fn process_signal_shutdown_requested() -> bool {
    SIGNAL_SHUTDOWN_STATE.load(Ordering::SeqCst) >= SIGNAL_SHUTDOWN_REQUESTED
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
