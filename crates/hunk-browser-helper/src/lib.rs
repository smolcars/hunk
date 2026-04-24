pub const HELPER_BINARY_NAME: &str = "hunk-browser-helper";
pub const MACOS_HELPER_BUNDLE_NAME: &str = "Hunk Browser Helper";

pub fn helper_startup_error() -> &'static str {
    "hunk-browser-helper is present, but the CEF subprocess entrypoint is not linked yet"
}

#[cfg(not(feature = "cef-subprocess"))]
pub fn run() -> i32 {
    eprintln!("{}", helper_startup_error());
    1
}

#[cfg(feature = "cef-subprocess")]
pub fn run() -> i32 {
    match execute_cef_subprocess() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

#[cfg(feature = "cef-subprocess")]
fn execute_cef_subprocess() -> Result<i32, String> {
    use cef::{api_hash, args::Args, execute_process, sys};

    #[cfg(target_os = "macos")]
    let _loader = load_macos_cef_framework()?;

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = Args::new();
    let process_result = execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    if process_result >= 0 {
        Ok(process_result)
    } else {
        Err("CEF subprocess dispatch failed".to_string())
    }
}

#[cfg(all(feature = "cef-subprocess", target_os = "macos"))]
fn load_macos_cef_framework() -> Result<cef::library_loader::LibraryLoader, String> {
    let loader = cef::library_loader::LibraryLoader::new(
        &std::env::current_exe().map_err(|error| error.to_string())?,
        true,
    );
    if loader.load() {
        Ok(loader)
    } else {
        Err("failed to load Chromium Embedded Framework".to_string())
    }
}
