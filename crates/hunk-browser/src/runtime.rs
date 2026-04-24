use std::collections::BTreeMap;

use crate::config::BrowserRuntimeConfig;
use crate::session::{BrowserAction, BrowserError, BrowserSession, BrowserSessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserRuntimeStatus {
    Disabled,
    Configured,
    Ready,
}

#[derive(Debug, Clone, Default)]
pub struct BrowserRuntime {
    config: Option<BrowserRuntimeConfig>,
    sessions: BTreeMap<BrowserSessionId, BrowserSession>,
    visible_session_id: Option<BrowserSessionId>,
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
        if self.config.is_some() {
            BrowserRuntimeStatus::Configured
        } else {
            BrowserRuntimeStatus::Disabled
        }
    }

    pub fn config(&self) -> Option<&BrowserRuntimeConfig> {
        self.config.as_ref()
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
            BrowserAction::Click { .. }
            | BrowserAction::Type { .. }
            | BrowserAction::Press { .. }
            | BrowserAction::Scroll { .. }
            | BrowserAction::Screenshot => {}
        }

        Ok(())
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}
