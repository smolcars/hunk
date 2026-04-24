use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::snapshot::{BrowserElement, BrowserSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BrowserSessionId(String);

impl BrowserSessionId {
    pub fn new(thread_id: impl Into<String>) -> Self {
        Self(thread_id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFrameMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionState {
    pub session_id: BrowserSessionId,
    pub url: Option<String>,
    pub title: Option<String>,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub snapshot_epoch: u64,
    pub latest_frame: Option<BrowserFrameMetadata>,
}

#[derive(Debug, Clone)]
pub struct BrowserSession {
    state: BrowserSessionState,
    latest_snapshot: BrowserSnapshot,
}

impl BrowserSession {
    pub fn new(session_id: BrowserSessionId) -> Self {
        let latest_snapshot = BrowserSnapshot::empty(0);
        Self {
            state: BrowserSessionState {
                session_id,
                url: None,
                title: None,
                loading: false,
                can_go_back: false,
                can_go_forward: false,
                snapshot_epoch: latest_snapshot.epoch,
                latest_frame: None,
            },
            latest_snapshot,
        }
    }

    pub fn state(&self) -> &BrowserSessionState {
        &self.state
    }

    pub fn latest_snapshot(&self) -> &BrowserSnapshot {
        &self.latest_snapshot
    }

    pub fn replace_snapshot(&mut self, snapshot: BrowserSnapshot) {
        self.state.url = snapshot.url.clone();
        self.state.title = snapshot.title.clone();
        self.state.snapshot_epoch = snapshot.epoch;
        self.latest_snapshot = snapshot;
    }

    pub fn set_latest_frame(&mut self, frame: BrowserFrameMetadata) {
        self.state.latest_frame = Some(frame);
    }

    pub fn validate_snapshot_element(
        &self,
        snapshot_epoch: u64,
        index: u32,
    ) -> Result<&BrowserElement, BrowserError> {
        if snapshot_epoch != self.latest_snapshot.epoch {
            return Err(BrowserError::StaleSnapshot {
                expected: self.latest_snapshot.epoch,
                received: snapshot_epoch,
            });
        }
        self.latest_snapshot
            .element(index)
            .ok_or(BrowserError::UnknownElementIndex(index))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum BrowserAction {
    Navigate {
        url: String,
    },
    Click {
        snapshot_epoch: u64,
        index: u32,
    },
    Type {
        snapshot_epoch: u64,
        index: u32,
        text: String,
        clear: bool,
    },
    Press {
        keys: String,
    },
    Scroll {
        down: bool,
        pages: f64,
        index: Option<u32>,
    },
    Screenshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserToolAction {
    Navigate,
    Snapshot,
    Click,
    Type,
    Press,
    Scroll,
    Screenshot,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BrowserError {
    #[error("browser backend is not available: {0}")]
    BackendUnavailable(String),
    #[error("browser session '{0}' was not found")]
    MissingSession(String),
    #[error("browser snapshot is stale; expected epoch {expected}, received {received}")]
    StaleSnapshot { expected: u64, received: u64 },
    #[error("browser snapshot does not contain element index {0}")]
    UnknownElementIndex(u32),
}
