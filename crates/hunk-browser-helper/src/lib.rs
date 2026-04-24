pub const HELPER_BINARY_NAME: &str = "hunk-browser-helper";
pub const MACOS_HELPER_BUNDLE_NAME: &str = "Hunk Browser Helper";

pub fn helper_executable_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "hunk-browser-helper.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        HELPER_BINARY_NAME
    }
}

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
fn load_macos_cef_framework() -> Result<MacCefLoader, String> {
    use std::os::unix::ffi::OsStrExt;

    const FRAMEWORK_BINARY: &str =
        "Chromium Embedded Framework.framework/Chromium Embedded Framework";

    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut candidates = Vec::new();
    if let Some(exe_dir) = current_exe.parent() {
        candidates.push(exe_dir.join("../../..").join(FRAMEWORK_BINARY));
    }
    for env_var in ["HUNK_CEF_RUNTIME_DIR", "CEF_PATH"] {
        if let Some(path) = std::env::var_os(env_var) {
            candidates.push(std::path::PathBuf::from(path).join(FRAMEWORK_BINARY));
        }
    }
    candidates.push(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("assets/browser-runtime/cef/macos/runtime")
            .join(FRAMEWORK_BINARY),
    );

    let framework_path = candidates
        .into_iter()
        .find_map(|path| path.canonicalize().ok())
        .ok_or_else(|| "failed to resolve Chromium Embedded Framework".to_string())?;
    let name = std::ffi::CString::new(framework_path.as_os_str().as_bytes())
        .map_err(|error| format!("invalid CEF framework path: {error}"))?;

    let library_path = unsafe { &*name.as_ptr().cast() };
    if cef::load_library(Some(library_path)) == 1 {
        Ok(MacCefLoader {
            path: framework_path,
        })
    } else {
        Err("failed to load Chromium Embedded Framework".to_string())
    }
}

#[cfg(all(feature = "cef-subprocess", target_os = "macos"))]
struct MacCefLoader {
    path: std::path::PathBuf,
}

#[cfg(all(feature = "cef-subprocess", target_os = "macos"))]
impl Drop for MacCefLoader {
    fn drop(&mut self) {
        if cef::unload_library() != 1 {
            eprintln!("cannot unload framework {}", self.path.display());
        }
    }
}
