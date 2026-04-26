use std::collections::BTreeMap;

#[cfg(feature = "cef")]
use crate::cef_backend::CefBrowserBackend;
use crate::config::BrowserRuntimeConfig;
use crate::session::{
    BrowserAction, BrowserError, BrowserMouseButton, BrowserMouseInput, BrowserSession,
    BrowserSessionId, BrowserTabId, BrowserTabSummary, BrowserViewportSize,
};
use crate::snapshot::BrowserPhysicalPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserRuntimeStatus {
    Disabled,
    Configured,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserRuntimeOperation {
    Initialize,
    CreateSession,
    Pump,
    Navigate,
    Reload,
    Stop,
    Back,
    Forward,
    Click,
    Type,
    Press,
    Scroll,
    Resize,
    Focus,
    Snapshot,
    Screenshot,
    DevTools,
}

impl BrowserRuntimeOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            BrowserRuntimeOperation::Initialize => "initialize",
            BrowserRuntimeOperation::CreateSession => "create session",
            BrowserRuntimeOperation::Pump => "pump",
            BrowserRuntimeOperation::Navigate => "navigate",
            BrowserRuntimeOperation::Reload => "reload",
            BrowserRuntimeOperation::Stop => "stop",
            BrowserRuntimeOperation::Back => "back",
            BrowserRuntimeOperation::Forward => "forward",
            BrowserRuntimeOperation::Click => "click",
            BrowserRuntimeOperation::Type => "type",
            BrowserRuntimeOperation::Press => "press",
            BrowserRuntimeOperation::Scroll => "scroll",
            BrowserRuntimeOperation::Resize => "resize",
            BrowserRuntimeOperation::Focus => "focus",
            BrowserRuntimeOperation::Snapshot => "snapshot",
            BrowserRuntimeOperation::Screenshot => "screenshot",
            BrowserRuntimeOperation::DevTools => "devtools",
        }
    }
}

impl std::fmt::Display for BrowserRuntimeOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for BrowserRuntimeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            BrowserRuntimeStatus::Disabled => "disabled",
            BrowserRuntimeStatus::Configured => "configured",
            BrowserRuntimeStatus::Ready => "ready",
        };
        f.write_str(label)
    }
}

#[derive(Default)]
pub struct BrowserRuntime {
    config: Option<BrowserRuntimeConfig>,
    backend_ready: bool,
    sessions: BTreeMap<BrowserSessionId, BrowserSession>,
    visible_session_id: Option<BrowserSessionId>,
    #[cfg(feature = "cef")]
    cef_backend: Option<CefBrowserBackend>,
}

impl Clone for BrowserRuntime {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            backend_ready: false,
            sessions: self.sessions.clone(),
            visible_session_id: self.visible_session_id.clone(),
            #[cfg(feature = "cef")]
            cef_backend: None,
        }
    }
}

impl std::fmt::Debug for BrowserRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserRuntime")
            .field("config", &self.config)
            .field("backend_ready", &self.backend_ready)
            .field("sessions", &self.sessions)
            .field("visible_session_id", &self.visible_session_id)
            .finish_non_exhaustive()
    }
}

impl BrowserRuntime {
    pub fn new_disabled() -> Self {
        Self::default()
    }

    pub fn new_configured(config: BrowserRuntimeConfig) -> Self {
        Self {
            config: Some(config),
            ..Default::default()
        }
    }

    pub fn status(&self) -> BrowserRuntimeStatus {
        if self.config.is_some() && self.backend_ready {
            BrowserRuntimeStatus::Ready
        } else if self.config.is_some() {
            BrowserRuntimeStatus::Configured
        } else {
            BrowserRuntimeStatus::Disabled
        }
    }

    pub fn config(&self) -> Option<&BrowserRuntimeConfig> {
        self.config.as_ref()
    }

    pub fn mark_backend_ready(&mut self) -> Result<(), BrowserError> {
        if self.config.is_none() {
            return Err(BrowserError::RuntimeNotReady {
                operation: BrowserRuntimeOperation::Initialize,
                status: self.status(),
            });
        }
        self.backend_ready = true;
        Ok(())
    }

    pub fn mark_backend_stopped(&mut self) {
        self.backend_ready = false;
    }

    pub fn require_ready_for_operation(
        &self,
        operation: BrowserRuntimeOperation,
    ) -> Result<(), BrowserError> {
        let status = self.status();
        if status == BrowserRuntimeStatus::Ready {
            Ok(())
        } else {
            Err(BrowserError::RuntimeNotReady { operation, status })
        }
    }

    pub fn ensure_session(&mut self, thread_id: impl Into<String>) -> &mut BrowserSession {
        let session_id = BrowserSessionId::new(thread_id);
        self.visible_session_id = Some(session_id.clone());
        self.sessions
            .entry(session_id.clone())
            .or_insert_with(|| BrowserSession::new(session_id))
    }

    pub fn set_visible_session(
        &mut self,
        thread_id: impl Into<String>,
    ) -> Result<(), BrowserError> {
        let session_id = BrowserSessionId::new(thread_id);
        if !self.sessions.contains_key(&session_id) {
            return Err(BrowserError::MissingSession(
                session_id.as_str().to_string(),
            ));
        }
        self.visible_session_id = Some(session_id);
        Ok(())
    }

    pub fn visible_session(&self) -> Option<&BrowserSession> {
        self.visible_session_id
            .as_ref()
            .and_then(|session_id| self.sessions.get(session_id))
    }

    pub fn session(&self, thread_id: &str) -> Option<&BrowserSession> {
        self.sessions.get(&BrowserSessionId::new(thread_id))
    }

    pub fn session_mut(&mut self, thread_id: &str) -> Option<&mut BrowserSession> {
        self.sessions.get_mut(&BrowserSessionId::new(thread_id))
    }

    pub fn close_session(&mut self, thread_id: &str) -> Option<BrowserSession> {
        let session_id = BrowserSessionId::new(thread_id);
        if self.visible_session_id.as_ref() == Some(&session_id) {
            self.visible_session_id = None;
        }
        self.sessions.remove(&session_id)
    }

    pub fn browser_tabs(&mut self, thread_id: &str) -> &[BrowserTabSummary] {
        self.ensure_session(thread_id.to_string()).tab_summaries()
    }

    pub fn active_tab_id(&mut self, thread_id: &str) -> BrowserTabId {
        self.ensure_session(thread_id.to_string())
            .active_tab_id()
            .clone()
    }

    pub fn create_tab(
        &mut self,
        thread_id: &str,
        url: Option<String>,
        activate: bool,
    ) -> BrowserTabId {
        self.ensure_session(thread_id.to_string())
            .create_tab(url, activate)
    }

    pub fn select_tab(
        &mut self,
        thread_id: &str,
        tab_id: &BrowserTabId,
    ) -> Result<(), BrowserError> {
        self.ensure_session(thread_id.to_string())
            .select_tab(tab_id)
    }

    pub fn close_tab(
        &mut self,
        thread_id: &str,
        tab_id: &BrowserTabId,
    ) -> Result<(), BrowserError> {
        #[cfg(feature = "cef")]
        if let Some(backend) = self.cef_backend.as_mut() {
            backend.close_tab(&BrowserSessionId::new(thread_id), tab_id);
        }
        self.ensure_session(thread_id.to_string()).close_tab(tab_id)
    }

    pub fn take_context_menu_target(
        &mut self,
        thread_id: &str,
    ) -> Option<crate::session::BrowserContextMenuTarget> {
        let session_id = BrowserSessionId::new(thread_id);
        let tab_id = self.sessions.get(&session_id)?.active_tab_id().clone();
        #[cfg(feature = "cef")]
        {
            self.cef_backend
                .as_mut()
                .and_then(|backend| backend.take_context_menu_target(&session_id, &tab_id))
        }
        #[cfg(not(feature = "cef"))]
        {
            let _ = (session_id, tab_id);
            None
        }
    }

    pub fn show_devtools(
        &mut self,
        thread_id: &str,
        inspect_element_at: Option<BrowserPhysicalPoint>,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::DevTools)?;
        self.ensure_backend_session(thread_id.to_string())?;

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.show_devtools(&session_id, &tab_id, inspect_element_at)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = inspect_element_at;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn close_devtools(&mut self, thread_id: &str) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::DevTools)?;

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.close_devtools(&session_id, &tab_id)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = thread_id;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn has_devtools(&mut self, thread_id: &str) -> Result<bool, BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::DevTools)?;

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.has_devtools(&session_id, &tab_id)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = thread_id;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn apply_state_only_action(
        &mut self,
        thread_id: &str,
        action: &BrowserAction,
    ) -> Result<(), BrowserError> {
        let session = self.ensure_session(thread_id.to_string());
        session.preflight_action(action)?;

        match action {
            BrowserAction::Navigate { url } => {
                session.navigate(url.clone());
            }
            BrowserAction::Reload => {
                session.reload()?;
            }
            BrowserAction::Stop => {
                session.stop();
            }
            BrowserAction::Back => {
                session.go_back()?;
            }
            BrowserAction::Forward => {
                session.go_forward()?;
            }
            BrowserAction::Click { .. }
            | BrowserAction::Type { .. }
            | BrowserAction::Press { .. }
            | BrowserAction::Scroll { .. }
            | BrowserAction::Screenshot => {}
        }

        Ok(())
    }

    pub fn resize_backend_session(
        &mut self,
        thread_id: &str,
        viewport: BrowserViewportSize,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Resize)?;
        let session = self.ensure_session(thread_id.to_string());
        session.set_viewport(viewport);

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            backend.resize_session(&session_id, &tab_id, viewport)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = viewport;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn focus_backend_session(
        &mut self,
        thread_id: &str,
        focused: bool,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Focus)?;
        self.ensure_session(thread_id.to_string());

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            backend.focus_session(&session_id, &tab_id, focused)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = focused;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn send_backend_mouse_move(
        &mut self,
        thread_id: &str,
        input: BrowserMouseInput,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Click)?;
        self.ensure_session(thread_id.to_string());

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            backend.send_mouse_move(&session_id, &tab_id, input)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = input;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn send_backend_mouse_click(
        &mut self,
        thread_id: &str,
        input: BrowserMouseInput,
        button: BrowserMouseButton,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Click)?;
        self.ensure_session(thread_id.to_string());

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            backend.send_mouse_click(&session_id, &tab_id, input, button)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = (input, button);
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn send_backend_mouse_wheel(
        &mut self,
        thread_id: &str,
        input: BrowserMouseInput,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Scroll)?;
        self.ensure_session(thread_id.to_string());

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            backend.send_mouse_wheel(&session_id, &tab_id, input, delta_x, delta_y)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = (input, delta_x, delta_y);
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn send_backend_key_press(
        &mut self,
        thread_id: &str,
        keys: &str,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Press)?;
        self.ensure_session(thread_id.to_string());

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            backend.send_key_press(&session_id, &tab_id, keys)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = keys;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn send_backend_text(&mut self, thread_id: &str, text: &str) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Type)?;
        self.ensure_session(thread_id.to_string());

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            backend.send_text(&session_id, &tab_id, text)
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = text;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn initialize_backend(&mut self) -> Result<(), BrowserError> {
        let Some(config) = self.config.clone() else {
            return Err(BrowserError::RuntimeNotReady {
                operation: BrowserRuntimeOperation::Initialize,
                status: self.status(),
            });
        };

        #[cfg(feature = "cef")]
        {
            if self.cef_backend.is_none() {
                self.cef_backend = Some(CefBrowserBackend::initialize(&config)?);
            }
            self.mark_backend_ready()?;
            Ok(())
        }

        #[cfg(not(feature = "cef"))]
        {
            let _ = config;
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn shutdown_backend(&mut self) {
        #[cfg(feature = "cef")]
        if let Some(mut backend) = self.cef_backend.take() {
            backend.shutdown();
        }

        self.mark_backend_stopped();
    }

    pub fn pump_backend(&mut self) -> Result<bool, BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Pump)?;

        #[cfg(feature = "cef")]
        {
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.pump(&mut self.sessions)
        }

        #[cfg(not(feature = "cef"))]
        {
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn capture_backend_snapshot(&mut self, thread_id: &str) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::Snapshot)?;
        self.ensure_backend_session(thread_id.to_string())?;
        #[cfg(feature = "cef")]
        let session_id = BrowserSessionId::new(thread_id);
        #[cfg(feature = "cef")]
        let tab_id = self
            .ensure_session(thread_id.to_string())
            .active_tab_id()
            .clone();
        #[cfg(feature = "cef")]
        let epoch = self
            .sessions
            .get(&session_id)
            .map(|session| session.latest_snapshot().epoch.saturating_add(1))
            .unwrap_or(1);

        #[cfg(feature = "cef")]
        {
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            let snapshot = backend.capture_snapshot(&session_id, &tab_id, epoch)?;
            let session = self
                .sessions
                .get_mut(&session_id)
                .ok_or_else(|| BrowserError::MissingSession(thread_id.to_string()))?;
            session.replace_snapshot(snapshot);
            Ok(())
        }

        #[cfg(not(feature = "cef"))]
        {
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn ensure_backend_session(
        &mut self,
        thread_id: impl Into<String>,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(BrowserRuntimeOperation::CreateSession)?;
        let session_id = BrowserSessionId::new(thread_id);
        self.visible_session_id = Some(session_id.clone());
        self.sessions
            .entry(session_id.clone())
            .or_insert_with(|| BrowserSession::new(session_id.clone()));
        #[cfg(feature = "cef")]
        let tab_id = self
            .sessions
            .get(&session_id)
            .map(|session| session.active_tab_id().clone())
            .unwrap_or_else(|| BrowserTabId::new("tab-1"));

        #[cfg(feature = "cef")]
        {
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id, tab_id)
        }

        #[cfg(not(feature = "cef"))]
        {
            Err(BrowserError::BackendUnavailable(
                "hunk-browser was built without the optional CEF backend".to_string(),
            ))
        }
    }

    pub fn apply_backend_action(
        &mut self,
        thread_id: &str,
        action: &BrowserAction,
    ) -> Result<(), BrowserError> {
        self.require_ready_for_operation(action.runtime_operation())?;
        self.ensure_session(thread_id.to_string())
            .preflight_action(action)?;

        #[cfg(feature = "cef")]
        {
            let session_id = BrowserSessionId::new(thread_id);
            let tab_id = self
                .ensure_session(thread_id.to_string())
                .active_tab_id()
                .clone();
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_tab(session_id.clone(), tab_id.clone())?;
            match action {
                BrowserAction::Click {
                    snapshot_epoch,
                    index,
                } => {
                    let point = self
                        .sessions
                        .get(&session_id)
                        .ok_or_else(|| BrowserError::MissingSession(thread_id.to_string()))?
                        .element_click_target(*snapshot_epoch, *index)?;
                    backend.send_mouse_click(
                        &session_id,
                        &tab_id,
                        BrowserMouseInput::new(point),
                        BrowserMouseButton::Left,
                    )?;
                }
                BrowserAction::Type {
                    snapshot_epoch,
                    index,
                    text,
                    clear,
                } => {
                    let point = self
                        .sessions
                        .get(&session_id)
                        .ok_or_else(|| BrowserError::MissingSession(thread_id.to_string()))?
                        .element_click_target(*snapshot_epoch, *index)?;
                    backend.send_mouse_click(
                        &session_id,
                        &tab_id,
                        BrowserMouseInput::new(point),
                        BrowserMouseButton::Left,
                    )?;
                    if *clear {
                        backend.send_key_press(&session_id, &tab_id, platform_select_all_keys())?;
                        backend.send_key_press(&session_id, &tab_id, "Backspace")?;
                    }
                    backend.send_text(&session_id, &tab_id, text)?;
                }
                BrowserAction::Press { keys } => {
                    backend.send_key_press(&session_id, &tab_id, keys)?;
                }
                BrowserAction::Scroll { down, pages, index } => {
                    let delta_y = scroll_pages_to_wheel_delta(*down, *pages);
                    let point = self
                        .sessions
                        .get(&session_id)
                        .ok_or_else(|| BrowserError::MissingSession(thread_id.to_string()))?
                        .scroll_target(*index)?;
                    backend.send_mouse_wheel(
                        &session_id,
                        &tab_id,
                        BrowserMouseInput::new(point),
                        0,
                        delta_y,
                    )?;
                }
                BrowserAction::Navigate { .. }
                | BrowserAction::Reload
                | BrowserAction::Stop
                | BrowserAction::Screenshot => {
                    backend.apply_action(&session_id, &tab_id, action)?;
                }
                BrowserAction::Back | BrowserAction::Forward => {
                    backend.apply_action(&session_id, &tab_id, action)?;
                    self.sessions
                        .get_mut(&session_id)
                        .ok_or_else(|| BrowserError::MissingSession(thread_id.to_string()))?
                        .start_backend_history_navigation();
                    return Ok(());
                }
            }
        }

        self.apply_state_only_action(thread_id, action)
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

#[cfg(feature = "cef")]
fn scroll_pages_to_wheel_delta(down: bool, pages: f64) -> i32 {
    let magnitude = (pages.abs().max(0.25) * 800.0).round().min(i32::MAX as f64) as i32;
    if down { -magnitude } else { magnitude }
}

#[cfg(feature = "cef")]
fn platform_select_all_keys() -> &'static str {
    if cfg!(target_os = "macos") {
        "Meta+A"
    } else {
        "Control+A"
    }
}
