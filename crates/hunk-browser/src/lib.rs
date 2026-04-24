mod frame;
mod runtime;
mod safety;
mod session;
mod snapshot;

pub use frame::{BrowserFrame, BrowserFrameError};
pub use runtime::{BrowserRuntime, BrowserRuntimeStatus};
pub use safety::{BrowserSafetyDecision, SensitiveBrowserAction, classify_browser_action};
pub use session::{
    BrowserAction, BrowserError, BrowserFrameMetadata, BrowserSession, BrowserSessionId,
    BrowserSessionState, BrowserToolAction,
};
pub use snapshot::{
    BrowserElement, BrowserElementRect, BrowserPhysicalPoint, BrowserPoint, BrowserSnapshot,
    BrowserViewport,
};
