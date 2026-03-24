#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod terminal_env;

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use hunk_domain::config::{AppConfig, ConfigStore};
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

static SIGNAL_SHUTDOWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    if terminal_env::maybe_handle_terminal_env_helper_mode()? {
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

    install_process_signal_cleanup()?;

    app::run()
}

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
