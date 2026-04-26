#[cfg(feature = "cef")]
mod cef_backend;
mod config;
mod frame;
mod runtime;
mod safety;
mod session;
mod snapshot;

pub use config::{BrowserConfigError, BrowserRuntimeConfig, BrowserStoragePaths};
pub use frame::{
    BROWSER_FRAME_TARGET_INTERVAL, BrowserFrame, BrowserFrameError, BrowserFrameRateLimiter,
};
pub use runtime::{BrowserRuntime, BrowserRuntimeOperation, BrowserRuntimeStatus};
pub use safety::{
    BrowserSafetyDecision, REDACTED_BROWSER_SECRET, SensitiveBrowserAction,
    classify_browser_action, redact_browser_tool_text,
};
pub use session::{
    BrowserAction, BrowserConsoleEntry, BrowserConsoleLevel, BrowserError, BrowserFrameMetadata,
    BrowserHistoryDirection, BrowserInputModifiers, BrowserMouseButton, BrowserMouseInput,
    BrowserSession, BrowserSessionId, BrowserSessionState, BrowserTabId, BrowserTabSummary,
    BrowserToolAction, BrowserViewportSize,
};
pub use snapshot::{
    BrowserElement, BrowserElementRect, BrowserPhysicalPoint, BrowserPoint, BrowserSnapshot,
    BrowserViewport,
};
