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
    BrowserAction, BrowserError, BrowserFrameMetadata, BrowserHistoryDirection, BrowserSession,
    BrowserSessionId, BrowserSessionState, BrowserToolAction,
};
pub use snapshot::{
    BrowserElement, BrowserElementRect, BrowserPhysicalPoint, BrowserPoint, BrowserSnapshot,
    BrowserViewport,
};
