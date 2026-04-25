use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_browser::{
    BrowserError, BrowserRuntime, BrowserRuntimeConfig, BrowserRuntimeOperation,
    BrowserRuntimeStatus, BrowserStoragePaths,
};

#[test]
fn storage_paths_are_isolated_under_browser_app_data() {
    let app_data_dir = PathBuf::from("/tmp/hunk-test-app-data");
    let paths = BrowserStoragePaths::from_app_data_dir(&app_data_dir);

    assert_eq!(paths.storage_root, app_data_dir.join("browser"));
    assert_eq!(paths.root_cache_path, app_data_dir.join("browser/cef-root"));
    assert_eq!(
        paths.profile_path,
        app_data_dir.join("browser/cef-root/profile")
    );
    assert_eq!(paths.downloads_path, app_data_dir.join("browser/downloads"));
    assert!(paths.profile_path.starts_with(&paths.root_cache_path));
}

#[test]
fn storage_paths_create_required_directories() {
    let app_data_dir = unique_temp_dir("storage-paths");
    let paths = BrowserStoragePaths::from_app_data_dir(&app_data_dir);

    paths.ensure_directories().unwrap();

    assert!(paths.storage_root.is_dir());
    assert!(paths.root_cache_path.is_dir());
    assert!(paths.profile_path.is_dir());
    assert!(paths.downloads_path.is_dir());

    let _ = std::fs::remove_dir_all(app_data_dir);
}

#[test]
fn configured_runtime_reports_configured_without_starting_cef() {
    let config = browser_runtime_config("ready");
    let runtime = BrowserRuntime::new_configured(config.clone());

    assert_eq!(runtime.status(), BrowserRuntimeStatus::Configured);
    assert_eq!(runtime.config(), Some(&config));
    assert_eq!(runtime.session_count(), 0);
}

#[test]
fn disabled_runtime_has_no_config() {
    let runtime = BrowserRuntime::new_disabled();

    assert_eq!(runtime.status(), BrowserRuntimeStatus::Disabled);
    assert_eq!(runtime.config(), None);
}

#[test]
fn disabled_runtime_reports_structured_not_ready_errors() {
    let runtime = BrowserRuntime::new_disabled();

    let error = runtime
        .require_ready_for_operation(BrowserRuntimeOperation::Navigate)
        .expect_err("disabled runtime should reject backend operations");

    assert_eq!(
        error,
        BrowserError::RuntimeNotReady {
            operation: BrowserRuntimeOperation::Navigate,
            status: BrowserRuntimeStatus::Disabled,
        }
    );
    assert_eq!(
        error.to_string(),
        "browser runtime is not ready for navigate; current status is disabled"
    );
}

#[test]
fn configured_runtime_reports_structured_not_ready_errors_until_backend_is_ready() {
    let config = browser_runtime_config("configured");
    let mut runtime = BrowserRuntime::new_configured(config);

    let error = runtime
        .require_ready_for_operation(BrowserRuntimeOperation::Screenshot)
        .expect_err("configured runtime should reject backend operations before CEF is ready");

    assert_eq!(
        error,
        BrowserError::RuntimeNotReady {
            operation: BrowserRuntimeOperation::Screenshot,
            status: BrowserRuntimeStatus::Configured,
        }
    );

    runtime
        .mark_backend_ready()
        .expect("configured runtime can become ready");

    assert_eq!(runtime.status(), BrowserRuntimeStatus::Ready);
    runtime
        .require_ready_for_operation(BrowserRuntimeOperation::Screenshot)
        .expect("ready runtime should accept backend operations");

    runtime.mark_backend_stopped();
    assert_eq!(runtime.status(), BrowserRuntimeStatus::Configured);
}

#[test]
fn disabled_runtime_cannot_be_marked_ready() {
    let mut runtime = BrowserRuntime::new_disabled();

    let error = runtime
        .mark_backend_ready()
        .expect_err("disabled runtime has no backend config to start");

    assert_eq!(
        error,
        BrowserError::RuntimeNotReady {
            operation: BrowserRuntimeOperation::Initialize,
            status: BrowserRuntimeStatus::Disabled,
        }
    );
}

fn browser_runtime_config(name: &str) -> BrowserRuntimeConfig {
    let storage_paths = BrowserStoragePaths::from_app_data_dir(format!("/tmp/hunk-browser-{name}"));
    BrowserRuntimeConfig::new(
        "/Applications/Hunk.app/Contents/Frameworks/Chromium Embedded Framework.framework",
        "/Applications/Hunk.app/Contents/Frameworks/Hunk Browser Helper.app/Contents/MacOS/Hunk Browser Helper",
        storage_paths,
    )
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hunk-browser-{name}-{}-{nanos}",
        std::process::id()
    ))
}
