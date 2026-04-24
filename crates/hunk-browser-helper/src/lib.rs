pub const HELPER_BINARY_NAME: &str = "hunk-browser-helper";
pub const MACOS_HELPER_BUNDLE_NAME: &str = "Hunk Browser Helper";

pub fn helper_startup_error() -> &'static str {
    "hunk-browser-helper is present, but the CEF subprocess entrypoint is not linked yet"
}
