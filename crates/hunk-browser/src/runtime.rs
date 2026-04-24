use std::collections::BTreeMap;

#[cfg(feature = "cef")]
use crate::cef_backend::CefBrowserBackend;
use crate::config::BrowserRuntimeConfig;
use crate::session::{BrowserAction, BrowserError, BrowserSession, BrowserSessionId};

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
        {
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_session(session_id)
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
            let Some(backend) = self.cef_backend.as_mut() else {
                return Err(BrowserError::BackendUnavailable(
                    "CEF backend is marked ready but is not connected".to_string(),
                ));
            };
            backend.ensure_session(session_id.clone())?;
            backend.apply_action(&session_id, action)?;
        }

        self.apply_state_only_action(thread_id, action)
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}
