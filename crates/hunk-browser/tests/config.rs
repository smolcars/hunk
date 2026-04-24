use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use hunk_browser::{
    BrowserRuntime, BrowserRuntimeConfig, BrowserRuntimeStatus, BrowserStoragePaths,
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
    let storage_paths = BrowserStoragePaths::from_app_data_dir("/tmp/hunk-browser-ready");
    let config = BrowserRuntimeConfig::new(
        "/Applications/Hunk.app/Contents/Frameworks/Chromium Embedded Framework.framework",
        "/Applications/Hunk.app/Contents/Frameworks/Hunk Browser Helper.app/Contents/MacOS/Hunk Browser Helper",
        storage_paths.clone(),
    );
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
