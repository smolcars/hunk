use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::frame::BrowserFrame;
use crate::snapshot::{BrowserElement, BrowserPhysicalPoint, BrowserSnapshot};

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
    pub load_error: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub snapshot_epoch: u64,
    pub latest_frame: Option<BrowserFrameMetadata>,
}

#[derive(Debug, Clone)]
pub struct BrowserSession {
    state: BrowserSessionState,
    latest_snapshot: BrowserSnapshot,
    latest_frame: Option<BrowserFrame>,
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
                load_error: None,
                can_go_back: false,
                can_go_forward: false,
                snapshot_epoch: latest_snapshot.epoch,
                latest_frame: None,
            },
            latest_snapshot,
            latest_frame: None,
        }
    }

    pub fn state(&self) -> &BrowserSessionState {
        &self.state
    }

    pub fn latest_snapshot(&self) -> &BrowserSnapshot {
        &self.latest_snapshot
    }

    pub fn latest_frame(&self) -> Option<&BrowserFrame> {
        self.latest_frame.as_ref()
    }

    pub fn navigate(&mut self, url: impl Into<String>) {
        self.state.url = Some(url.into());
        self.state.title = None;
        self.state.loading = true;
        self.state.load_error = None;
        self.state.can_go_back = false;
        self.state.can_go_forward = false;
        self.latest_snapshot = BrowserSnapshot::empty(self.latest_snapshot.epoch + 1);
        self.state.snapshot_epoch = self.latest_snapshot.epoch;
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.state.loading = loading;
    }

    pub fn set_load_error(&mut self, error: impl Into<String>) {
        self.state.loading = false;
        self.state.load_error = Some(error.into());
    }

    pub fn clear_load_error(&mut self) {
        self.state.load_error = None;
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.state.title = Some(title.into());
    }

    pub fn set_history_state(&mut self, can_go_back: bool, can_go_forward: bool) {
        self.state.can_go_back = can_go_back;
        self.state.can_go_forward = can_go_forward;
    }

    pub fn replace_snapshot(&mut self, snapshot: BrowserSnapshot) {
        self.state.url = snapshot.url.clone();
        self.state.title = snapshot.title.clone();
        self.state.snapshot_epoch = snapshot.epoch;
        self.latest_snapshot = snapshot;
    }

    pub fn set_latest_frame(&mut self, frame: BrowserFrame) {
        self.state.latest_frame = Some(frame.metadata().clone());
        self.latest_frame = Some(frame);
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

    pub fn element_click_target(
        &self,
        snapshot_epoch: u64,
        index: u32,
    ) -> Result<BrowserPhysicalPoint, BrowserError> {
        let element = self.validate_snapshot_element(snapshot_epoch, index)?;
        Ok(self
            .latest_snapshot
            .viewport
            .logical_to_physical_point(element.rect.center()))
    }

    pub fn preflight_action(&self, action: &BrowserAction) -> Result<(), BrowserError> {
        match action {
            BrowserAction::Click {
                snapshot_epoch,
                index,
            }
            | BrowserAction::Type {
                snapshot_epoch,
                index,
                ..
            } => {
                self.validate_snapshot_element(*snapshot_epoch, *index)?;
                Ok(())
            }
            BrowserAction::Navigate { .. }
            | BrowserAction::Press { .. }
            | BrowserAction::Scroll { .. }
            | BrowserAction::Screenshot => Ok(()),
        }
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
