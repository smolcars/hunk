use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::frame::BrowserFrame;
use crate::runtime::{BrowserRuntimeOperation, BrowserRuntimeStatus};
use crate::snapshot::{BrowserElement, BrowserPhysicalPoint, BrowserSnapshot};

const MAX_CONSOLE_ENTRIES: usize = 500;
const INITIAL_BROWSER_TAB_ID: &str = "tab-1";

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BrowserTabId(String);

impl BrowserTabId {
    pub fn new(tab_id: impl Into<String>) -> Self {
        Self(tab_id.into())
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
pub struct BrowserTabSummary {
    pub tab_id: BrowserTabId,
    pub url: Option<String>,
    pub title: Option<String>,
    pub loading: bool,
    pub load_error: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub snapshot_epoch: u64,
    pub latest_frame: Option<BrowserFrameMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionState {
    pub session_id: BrowserSessionId,
    pub active_tab_id: BrowserTabId,
    pub tabs: Vec<BrowserTabSummary>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub loading: bool,
    pub load_error: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub snapshot_epoch: u64,
    pub latest_frame: Option<BrowserFrameMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConsoleEntry {
    pub sequence: u64,
    pub level: BrowserConsoleLevel,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<u32>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserConsoleLevel {
    Verbose,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct BrowserSession {
    state: BrowserSessionState,
    latest_snapshot: BrowserSnapshot,
    latest_frame: Option<BrowserFrame>,
    console_entries: Vec<BrowserConsoleEntry>,
    next_console_sequence: u64,
    back_history: Vec<String>,
    forward_history: Vec<String>,
    next_tab_ordinal: u64,
}

impl BrowserSession {
    pub fn new(session_id: BrowserSessionId) -> Self {
        let latest_snapshot = BrowserSnapshot::empty(0);
        let active_tab_id = BrowserTabId::new(INITIAL_BROWSER_TAB_ID);
        let initial_tab = BrowserTabSummary {
            tab_id: active_tab_id.clone(),
            url: None,
            title: None,
            loading: false,
            load_error: None,
            can_go_back: false,
            can_go_forward: false,
            snapshot_epoch: latest_snapshot.epoch,
            latest_frame: None,
        };
        Self {
            state: BrowserSessionState {
                session_id,
                active_tab_id,
                tabs: vec![initial_tab],
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
            console_entries: Vec::new(),
            next_console_sequence: 0,
            back_history: Vec::new(),
            forward_history: Vec::new(),
            next_tab_ordinal: 2,
        }
    }

    pub fn state(&self) -> &BrowserSessionState {
        &self.state
    }

    pub fn active_tab_id(&self) -> &BrowserTabId {
        &self.state.active_tab_id
    }

    pub fn tab_summaries(&self) -> &[BrowserTabSummary] {
        &self.state.tabs
    }

    pub fn latest_snapshot(&self) -> &BrowserSnapshot {
        &self.latest_snapshot
    }

    pub fn latest_frame(&self) -> Option<&BrowserFrame> {
        self.latest_frame.as_ref()
    }

    pub fn console_entries(&self) -> &[BrowserConsoleEntry] {
        &self.console_entries
    }

    pub fn recent_console_entries(
        &self,
        level: Option<BrowserConsoleLevel>,
        since_sequence: Option<u64>,
        limit: usize,
    ) -> Vec<BrowserConsoleEntry> {
        let mut entries = self
            .console_entries
            .iter()
            .filter(|entry| level.is_none_or(|level| entry.level == level))
            .filter(|entry| since_sequence.is_none_or(|since| entry.sequence > since))
            .cloned()
            .collect::<Vec<_>>();
        let limit = limit.max(1);
        if entries.len() > limit {
            entries.drain(0..entries.len() - limit);
        }
        entries
    }

    pub fn push_console_entry(
        &mut self,
        level: BrowserConsoleLevel,
        message: impl Into<String>,
        source: Option<String>,
        line: Option<u32>,
        timestamp_ms: u64,
    ) {
        self.next_console_sequence = self.next_console_sequence.saturating_add(1);
        self.console_entries.push(BrowserConsoleEntry {
            sequence: self.next_console_sequence,
            level,
            message: message.into(),
            source,
            line,
            timestamp_ms,
        });
        if self.console_entries.len() > MAX_CONSOLE_ENTRIES {
            let overflow = self.console_entries.len() - MAX_CONSOLE_ENTRIES;
            self.console_entries.drain(0..overflow);
        }
    }

    pub fn clear_console_entries(&mut self) {
        self.console_entries.clear();
    }

    pub fn create_tab(&mut self, url: Option<String>, activate: bool) -> BrowserTabId {
        let tab_id = BrowserTabId::new(format!("tab-{}", self.next_tab_ordinal));
        self.next_tab_ordinal = self.next_tab_ordinal.saturating_add(1);
        let tab = BrowserTabSummary {
            tab_id: tab_id.clone(),
            url: url.clone(),
            title: None,
            loading: url.is_some(),
            load_error: None,
            can_go_back: false,
            can_go_forward: false,
            snapshot_epoch: self.latest_snapshot.epoch,
            latest_frame: None,
        };
        self.state.tabs.push(tab);
        if activate {
            self.activate_tab_state(&tab_id);
        }
        tab_id
    }

    pub fn select_tab(&mut self, tab_id: &BrowserTabId) -> Result<(), BrowserError> {
        if !self.state.tabs.iter().any(|tab| &tab.tab_id == tab_id) {
            return Err(BrowserError::MissingTab(tab_id.as_str().to_string()));
        }
        self.activate_tab_state(tab_id);
        Ok(())
    }

    pub fn close_tab(&mut self, tab_id: &BrowserTabId) -> Result<(), BrowserError> {
        let Some(index) = self.state.tabs.iter().position(|tab| &tab.tab_id == tab_id) else {
            return Err(BrowserError::MissingTab(tab_id.as_str().to_string()));
        };

        if self.state.tabs.len() == 1 {
            self.state.tabs[0] = BrowserTabSummary {
                tab_id: self.state.active_tab_id.clone(),
                url: None,
                title: None,
                loading: false,
                load_error: None,
                can_go_back: false,
                can_go_forward: false,
                snapshot_epoch: self.latest_snapshot.epoch.saturating_add(1),
                latest_frame: None,
            };
            self.state.url = None;
            self.state.title = None;
            self.state.loading = false;
            self.state.load_error = None;
            self.state.can_go_back = false;
            self.state.can_go_forward = false;
            self.latest_frame = None;
            self.back_history.clear();
            self.forward_history.clear();
            self.invalidate_snapshot();
            self.sync_active_tab_summary();
            return Ok(());
        }

        let closing_active = &self.state.active_tab_id == tab_id;
        self.state.tabs.remove(index);
        if closing_active {
            let replacement_index = index.saturating_sub(1).min(self.state.tabs.len() - 1);
            let replacement_tab_id = self.state.tabs[replacement_index].tab_id.clone();
            self.activate_tab_state(&replacement_tab_id);
        }
        Ok(())
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
        self.sync_active_tab_summary();
        Ok(())
    }

    pub fn stop(&mut self) {
        self.state.loading = false;
        self.sync_active_tab_summary();
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
        self.sync_active_tab_summary();
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
        self.sync_active_tab_summary();
    }

    pub fn apply_backend_loading_state(
        &mut self,
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
        url: Option<String>,
    ) {
        self.set_history_state(can_go_back, can_go_forward);
        if let Some(url) = url {
            self.state.url = Some(url);
        }
        if loading && !self.state.loading {
            self.state.load_error = None;
            self.invalidate_snapshot();
        }
        self.state.loading = loading;
        self.sync_active_tab_summary();
    }

    pub fn set_url(&mut self, url: impl Into<String>) {
        self.state.url = Some(url.into());
        self.sync_active_tab_summary();
    }

    pub fn start_backend_history_navigation(&mut self) {
        if !self.state.loading {
            self.invalidate_snapshot();
        }
        self.state.loading = true;
        self.state.load_error = None;
        self.sync_active_tab_summary();
    }

    pub fn set_load_error(&mut self, error: impl Into<String>) {
        self.state.loading = false;
        self.state.load_error = Some(error.into());
        self.sync_active_tab_summary();
    }

    pub fn clear_load_error(&mut self) {
        self.state.load_error = None;
        self.sync_active_tab_summary();
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.state.title = Some(title.into());
        self.sync_active_tab_summary();
    }

    pub fn set_history_state(&mut self, can_go_back: bool, can_go_forward: bool) {
        self.state.can_go_back = can_go_back;
        self.state.can_go_forward = can_go_forward;
        self.sync_active_tab_summary();
    }

    pub fn set_viewport(&mut self, viewport: BrowserViewportSize) {
        self.latest_snapshot.viewport.width = viewport.width;
        self.latest_snapshot.viewport.height = viewport.height;
        self.latest_snapshot.viewport.device_scale_factor = viewport.device_scale_factor;
    }

    pub fn replace_snapshot(&mut self, snapshot: BrowserSnapshot) {
        self.state.url = snapshot.url.clone();
        self.state.title = snapshot.title.clone();
        self.state.snapshot_epoch = snapshot.epoch;
        self.latest_snapshot = snapshot;
        self.sync_active_tab_summary();
    }

    pub fn set_latest_frame(&mut self, frame: BrowserFrame) {
        self.state.latest_frame = Some(frame.metadata().clone());
        self.latest_frame = Some(frame);
        self.sync_active_tab_summary();
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
            .logical_to_view_point(element.rect.center()))
    }

    pub fn scroll_target(&self, index: Option<u32>) -> Result<BrowserPhysicalPoint, BrowserError> {
        let point = if let Some(index) = index {
            self.latest_snapshot
                .element(index)
                .ok_or(BrowserError::UnknownElementIndex(index))?
                .rect
                .center()
        } else {
            crate::snapshot::BrowserPoint {
                x: self.latest_snapshot.viewport.width as f64 / 2.0,
                y: self.latest_snapshot.viewport.height as f64 / 2.0,
            }
        };
        Ok(self.latest_snapshot.viewport.logical_to_view_point(point))
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

    fn activate_tab_state(&mut self, tab_id: &BrowserTabId) {
        self.sync_active_tab_summary();
        let Some(tab) = self
            .state
            .tabs
            .iter()
            .find(|tab| &tab.tab_id == tab_id)
            .cloned()
        else {
            return;
        };
        self.state.active_tab_id = tab.tab_id;
        self.state.url = tab.url;
        self.state.title = tab.title;
        self.state.loading = tab.loading;
        self.state.load_error = tab.load_error;
        self.state.can_go_back = tab.can_go_back;
        self.state.can_go_forward = tab.can_go_forward;
        self.state.snapshot_epoch = tab.snapshot_epoch;
        self.state.latest_frame = tab.latest_frame;
    }

    fn sync_active_tab_summary(&mut self) {
        let active_tab_id = self.state.active_tab_id.clone();
        if let Some(tab) = self
            .state
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == active_tab_id)
        {
            tab.url = self.state.url.clone();
            tab.title = self.state.title.clone();
            tab.loading = self.state.loading;
            tab.load_error = self.state.load_error.clone();
            tab.can_go_back = self.state.can_go_back;
            tab.can_go_forward = self.state.can_go_forward;
            tab.snapshot_epoch = self.state.snapshot_epoch;
            tab.latest_frame = self.state.latest_frame.clone();
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserViewportSize {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: f32,
}

impl BrowserViewportSize {
    pub fn new(width: u32, height: u32, device_scale_factor: f32) -> Result<Self, BrowserError> {
        if width == 0 || height == 0 {
            return Err(BrowserError::InvalidViewportSize { width, height });
        }

        let device_scale_factor = if device_scale_factor.is_finite() {
            device_scale_factor.max(f32::EPSILON)
        } else {
            1.0
        };

        Ok(Self {
            width,
            height,
            device_scale_factor,
        })
    }
}

impl Default for BrowserViewportSize {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 768,
            device_scale_factor: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserMouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInputModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserMouseInput {
    pub point: BrowserPhysicalPoint,
    pub modifiers: BrowserInputModifiers,
}

impl BrowserMouseInput {
    pub fn new(point: BrowserPhysicalPoint) -> Self {
        Self {
            point,
            modifiers: BrowserInputModifiers::default(),
        }
    }
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
    #[error("browser tab '{0}' was not found")]
    MissingTab(String),
    #[error("browser has no loaded page")]
    NoPageLoaded,
    #[error("browser cannot go {0}; no history entry is available")]
    HistoryUnavailable(BrowserHistoryDirection),
    #[error("browser snapshot is stale; expected epoch {expected}, received {received}")]
    StaleSnapshot { expected: u64, received: u64 },
    #[error("browser snapshot does not contain element index {0}")]
    UnknownElementIndex(u32),
    #[error("browser viewport dimensions must be non-zero, received {width}x{height}")]
    InvalidViewportSize { width: u32, height: u32 },
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
