use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::frame::MobileFrame;
use crate::snapshot::{MobileElement, MobileSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MobileSessionId(String);

impl MobileSessionId {
    pub fn new(thread_id: impl Into<String>) -> Self {
        Self(thread_id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MobileDeviceId(String);

impl MobileDeviceId {
    pub fn new(device_id: impl Into<String>) -> Self {
        Self(device_id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone)]
pub struct MobileSession {
    session_id: MobileSessionId,
    selected_device_id: Option<MobileDeviceId>,
    latest_snapshot: MobileSnapshot,
    latest_frame: Option<MobileFrame>,
}

impl MobileSession {
    pub fn new(session_id: MobileSessionId) -> Self {
        Self {
            session_id,
            selected_device_id: None,
            latest_snapshot: MobileSnapshot::empty(0),
            latest_frame: None,
        }
    }

    pub fn session_id(&self) -> &MobileSessionId {
        &self.session_id
    }

    pub fn selected_device_id(&self) -> Option<&MobileDeviceId> {
        self.selected_device_id.as_ref()
    }

    pub fn select_device(&mut self, device_id: MobileDeviceId) {
        self.selected_device_id = Some(device_id);
    }

    pub fn latest_snapshot(&self) -> &MobileSnapshot {
        &self.latest_snapshot
    }

    pub fn latest_frame(&self) -> Option<&MobileFrame> {
        self.latest_frame.as_ref()
    }

    pub fn replace_snapshot(&mut self, snapshot: MobileSnapshot) {
        self.latest_snapshot = snapshot;
    }

    pub fn set_latest_frame(&mut self, frame: MobileFrame) {
        self.latest_frame = Some(frame);
    }

    pub fn validate_snapshot_element(
        &self,
        snapshot_epoch: u64,
        index: u32,
    ) -> Result<&MobileElement, MobileError> {
        if snapshot_epoch != self.latest_snapshot.epoch {
            return Err(MobileError::StaleSnapshot {
                expected: self.latest_snapshot.epoch,
                received: snapshot_epoch,
            });
        }
        self.latest_snapshot
            .element(index)
            .ok_or(MobileError::UnknownElementIndex(index))
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MobileError {
    #[error("Android SDK tool '{tool}' was not found")]
    MissingAndroidTool { tool: String },
    #[error("Android SDK tool '{tool}' failed: {message}")]
    AndroidToolFailed { tool: String, message: String },
    #[error("no running Android emulator was found")]
    NoRunningEmulator,
    #[error("mobile device '{0}' was not found")]
    MissingDevice(String),
    #[error("mobile session '{0}' was not found")]
    MissingSession(String),
    #[error("mobile snapshot is stale; expected epoch {expected}, received {received}")]
    StaleSnapshot { expected: u64, received: u64 },
    #[error("mobile snapshot does not contain element index {0}")]
    UnknownElementIndex(u32),
    #[error("failed to parse Android UI hierarchy: {0}")]
    UiHierarchyParse(String),
    #[error("failed to parse Android bounds '{0}'")]
    InvalidBounds(String),
    #[error("Android text input is unsupported: {0}")]
    UnsupportedTextInput(String),
    #[error("Android screenshot failed: {0}")]
    Screenshot(String),
    #[error("invalid Android key '{0}'")]
    InvalidKey(String),
}
