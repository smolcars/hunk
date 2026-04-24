use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::frame::BrowserFrame;
use crate::runtime::{BrowserRuntimeOperation, BrowserRuntimeStatus};
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
    back_history: Vec<String>,
    forward_history: Vec<String>,
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
            back_history: Vec::new(),
            forward_history: Vec::new(),
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
        let url = url.into();
        if self.state.url.as_deref() != Some(url.as_str()) {
            if let Some(current_url) = self.state.url.clone() {
                self.back_history.push(current_url);
            }
            self.forward_history.clear();
        }
        self.start_navigation_to(url);
    }

    pub fn reload(&mut self) -> Result<(), BrowserError> {
        if self.state.url.is_none() {
            return Err(BrowserError::NoPageLoaded);
        }
        self.state.loading = true;
        self.state.load_error = None;
        self.invalidate_snapshot();
        Ok(())
    }

    pub fn stop(&mut self) {
        self.state.loading = false;
    }

    pub fn go_back(&mut self) -> Result<(), BrowserError> {
        let Some(url) = self.back_history.pop() else {
            return Err(BrowserError::HistoryUnavailable(
                BrowserHistoryDirection::Back,
            ));
        };
        if let Some(current_url) = self.state.url.clone() {
            self.forward_history.push(current_url);
        }
        self.start_navigation_to(url);
        Ok(())
    }

    pub fn go_forward(&mut self) -> Result<(), BrowserError> {
        let Some(url) = self.forward_history.pop() else {
            return Err(BrowserError::HistoryUnavailable(
                BrowserHistoryDirection::Forward,
            ));
        };
        if let Some(current_url) = self.state.url.clone() {
            self.back_history.push(current_url);
        }
        self.start_navigation_to(url);
        Ok(())
    }

    fn start_navigation_to(&mut self, url: String) {
        self.state.url = Some(url);
        self.state.title = None;
        self.state.loading = true;
        self.state.load_error = None;
        self.refresh_history_state();
        self.invalidate_snapshot();
    }

    fn invalidate_snapshot(&mut self) {
        self.latest_snapshot = BrowserSnapshot::empty(self.latest_snapshot.epoch + 1);
        self.state.snapshot_epoch = self.latest_snapshot.epoch;
    }

    fn refresh_history_state(&mut self) {
        self.state.can_go_back = !self.back_history.is_empty();
        self.state.can_go_forward = !self.forward_history.is_empty();
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
            | BrowserAction::Reload
            | BrowserAction::Stop
            | BrowserAction::Back
            | BrowserAction::Forward
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
    Reload,
    Stop,
    Back,
    Forward,
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

impl BrowserAction {
    pub fn runtime_operation(&self) -> BrowserRuntimeOperation {
        match self {
            BrowserAction::Navigate { .. } => BrowserRuntimeOperation::Navigate,
            BrowserAction::Reload => BrowserRuntimeOperation::Reload,
            BrowserAction::Stop => BrowserRuntimeOperation::Stop,
            BrowserAction::Back => BrowserRuntimeOperation::Back,
            BrowserAction::Forward => BrowserRuntimeOperation::Forward,
            BrowserAction::Click { .. } => BrowserRuntimeOperation::Click,
            BrowserAction::Type { .. } => BrowserRuntimeOperation::Type,
            BrowserAction::Press { .. } => BrowserRuntimeOperation::Press,
            BrowserAction::Scroll { .. } => BrowserRuntimeOperation::Scroll,
            BrowserAction::Screenshot => BrowserRuntimeOperation::Screenshot,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserToolAction {
    Navigate,
    Reload,
    Stop,
    Back,
    Forward,
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
    #[error("browser runtime is not ready for {operation}; current status is {status}")]
    RuntimeNotReady {
        operation: BrowserRuntimeOperation,
        status: BrowserRuntimeStatus,
    },
    #[error("browser session '{0}' was not found")]
    MissingSession(String),
    #[error("browser has no loaded page")]
    NoPageLoaded,
    #[error("browser cannot go {0}; no history entry is available")]
    HistoryUnavailable(BrowserHistoryDirection),
    #[error("browser snapshot is stale; expected epoch {expected}, received {received}")]
    StaleSnapshot { expected: u64, received: u64 },
    #[error("browser snapshot does not contain element index {0}")]
    UnknownElementIndex(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserHistoryDirection {
    Back,
    Forward,
}

impl std::fmt::Display for BrowserHistoryDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            BrowserHistoryDirection::Back => "back",
            BrowserHistoryDirection::Forward => "forward",
        };
        f.write_str(label)
    }
}
