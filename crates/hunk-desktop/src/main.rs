mod app;

use anyhow::Result;
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

fn main() -> Result<()> {
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
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env_lossy()
        .add_directive("html5ever=warn".parse()?)
        .add_directive("markup5ever=warn".parse()?);

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .without_time()
        .init();

    app::run()
}
