use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;

pub(crate) const DESKTOP_NOTIFICATION_APP_ID: &str = "com.niteshbalusu.hunk";
const MACOS_NOTIFICATION_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const MACOS_NOTIFICATION_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MacOsNotificationPermissionState {
    #[default]
    Unknown,
    Unavailable,
    NotDetermined,
    Denied,
    Authorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiDesktopNotificationKind {
    ApprovalRequired,
    UserInputRequired,
    PlanReady,
    AgentFinished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopNotificationRequest {
    pub identifier: String,
    pub title: String,
    pub body: String,
    pub thread_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AiInProgressTurnKey {
    pub thread_id: String,
    pub turn_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiDesktopNotificationSnapshot {
    pub workspace_label: String,
    pub approval_request_thread_by_id: BTreeMap<String, String>,
    pub user_input_thread_by_id: BTreeMap<String, String>,
    pub plan_prompt_sequence_by_thread: BTreeMap<String, u64>,
    pub in_progress_turns: BTreeSet<AiInProgressTurnKey>,
    pub thread_label_by_id: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiDesktopNotificationState {
    pub initialized: bool,
    pub approval_request_thread_by_id: BTreeMap<String, String>,
    pub user_input_thread_by_id: BTreeMap<String, String>,
    pub plan_prompt_sequence_by_thread: BTreeMap<String, u64>,
    pub in_progress_turns: BTreeSet<AiInProgressTurnKey>,
    pub thread_label_by_id: BTreeMap<String, String>,
    pub workspace_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiDesktopNotificationEvent {
    ApprovalRequired {
        request_id: String,
        thread_id: String,
        label: String,
    },
    UserInputRequired {
        request_id: String,
        thread_id: String,
        label: String,
    },
    PlanReady {
        thread_id: String,
        source_sequence: u64,
        label: String,
    },
    AgentFinished {
        thread_id: String,
        turn_id: String,
        label: String,
    },
}

impl AiDesktopNotificationEvent {
    pub(crate) fn kind(&self) -> AiDesktopNotificationKind {
        match self {
            Self::ApprovalRequired { .. } => AiDesktopNotificationKind::ApprovalRequired,
            Self::UserInputRequired { .. } => AiDesktopNotificationKind::UserInputRequired,
            Self::PlanReady { .. } => AiDesktopNotificationKind::PlanReady,
            Self::AgentFinished { .. } => AiDesktopNotificationKind::AgentFinished,
        }
    }

    pub(crate) fn request(&self) -> DesktopNotificationRequest {
        match self {
            Self::ApprovalRequired {
                request_id,
                thread_id,
                label,
            } => DesktopNotificationRequest {
                identifier: format!("ai-approval-{request_id}"),
                title: "Approval needed".to_string(),
                body: format!("{label} needs approval before the agent can continue."),
                thread_identifier: Some(thread_id.clone()),
            },
            Self::UserInputRequired {
                request_id,
                thread_id,
                label,
            } => DesktopNotificationRequest {
                identifier: format!("ai-input-{request_id}"),
                title: "Agent needs your input".to_string(),
                body: format!("{label} is waiting for your input."),
                thread_identifier: Some(thread_id.clone()),
            },
            Self::PlanReady {
                thread_id,
                source_sequence,
                label,
            } => DesktopNotificationRequest {
                identifier: format!("ai-plan-{thread_id}-{source_sequence}"),
                title: "Plan ready".to_string(),
                body: format!("A plan is ready in {label}."),
                thread_identifier: Some(thread_id.clone()),
            },
            Self::AgentFinished {
                thread_id,
                turn_id,
                label,
            } => DesktopNotificationRequest {
                identifier: format!("ai-finished-{thread_id}-{turn_id}"),
                title: "Agent finished".to_string(),
                body: format!("The agent finished working in {label}."),
                thread_identifier: Some(thread_id.clone()),
            },
        }
    }
}

pub(crate) fn next_ai_desktop_notification_state(
    previous: Option<&AiDesktopNotificationState>,
    snapshot: AiDesktopNotificationSnapshot,
) -> (
    AiDesktopNotificationState,
    Option<AiDesktopNotificationEvent>,
) {
    let next_state = AiDesktopNotificationState {
        initialized: true,
        approval_request_thread_by_id: snapshot.approval_request_thread_by_id.clone(),
        user_input_thread_by_id: snapshot.user_input_thread_by_id.clone(),
        plan_prompt_sequence_by_thread: snapshot.plan_prompt_sequence_by_thread.clone(),
        in_progress_turns: snapshot.in_progress_turns.clone(),
        thread_label_by_id: snapshot.thread_label_by_id.clone(),
        workspace_label: snapshot.workspace_label.clone(),
    };

    let Some(previous) = previous.filter(|previous| previous.initialized) else {
        return (next_state, None);
    };

    let event = if let Some((request_id, thread_id)) = next_state
        .approval_request_thread_by_id
        .iter()
        .find(|(request_id, _)| {
            !previous
                .approval_request_thread_by_id
                .contains_key(*request_id)
        }) {
        Some(AiDesktopNotificationEvent::ApprovalRequired {
            request_id: request_id.clone(),
            thread_id: thread_id.clone(),
            label: thread_label(thread_id, previous, &next_state),
        })
    } else if let Some((request_id, thread_id)) = next_state
        .user_input_thread_by_id
        .iter()
        .find(|(request_id, _)| !previous.user_input_thread_by_id.contains_key(*request_id))
    {
        Some(AiDesktopNotificationEvent::UserInputRequired {
            request_id: request_id.clone(),
            thread_id: thread_id.clone(),
            label: thread_label(thread_id, previous, &next_state),
        })
    } else if let Some((thread_id, source_sequence)) = next_state
        .plan_prompt_sequence_by_thread
        .iter()
        .find(|(thread_id, source_sequence)| {
            previous
                .plan_prompt_sequence_by_thread
                .get(*thread_id)
                .copied()
                .unwrap_or(0)
                != **source_sequence
        })
    {
        Some(AiDesktopNotificationEvent::PlanReady {
            thread_id: thread_id.clone(),
            source_sequence: *source_sequence,
            label: thread_label(thread_id, previous, &next_state),
        })
    } else {
        previous
            .in_progress_turns
            .iter()
            .find(|turn_key| !next_state.in_progress_turns.contains(*turn_key))
            .map(|turn_key| AiDesktopNotificationEvent::AgentFinished {
                thread_id: turn_key.thread_id.clone(),
                turn_id: turn_key.turn_id.clone(),
                label: thread_label(turn_key.thread_id.as_str(), previous, &next_state),
            })
    };

    (next_state, event)
}

fn thread_label(
    thread_id: &str,
    previous: &AiDesktopNotificationState,
    next: &AiDesktopNotificationState,
) -> String {
    next.thread_label_by_id
        .get(thread_id)
        .or_else(|| previous.thread_label_by_id.get(thread_id))
        .cloned()
        .unwrap_or_else(|| next.workspace_label.clone())
}

fn is_macos_app_bundle_executable_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("Contents"))
}

pub(crate) fn current_executable_display_path() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string())
}

pub(crate) const fn desktop_notification_backend_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macOS UNUserNotificationCenter"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "notify-rust"
    }
}

pub(crate) fn show_desktop_notification(request: &DesktopNotificationRequest) -> Result<()> {
    platform::show_desktop_notification(request)
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_notification_permission_status() -> Result<MacOsNotificationPermissionState> {
    if !platform::macos_notification_center_available() {
        return Ok(MacOsNotificationPermissionState::Unavailable);
    }
    platform::macos_notification_permission_status()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn macos_notification_permission_status() -> Result<MacOsNotificationPermissionState> {
    Ok(MacOsNotificationPermissionState::Authorized)
}

#[cfg(target_os = "macos")]
pub(crate) fn request_macos_notification_permission() -> Result<MacOsNotificationPermissionState> {
    if !platform::macos_notification_center_available() {
        return Ok(MacOsNotificationPermissionState::Unavailable);
    }
    platform::request_macos_notification_permission()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn request_macos_notification_permission() -> Result<MacOsNotificationPermissionState> {
    Ok(MacOsNotificationPermissionState::Authorized)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ptr::NonNull;
    use std::sync::mpsc;

    use anyhow::{Context as _, anyhow};
    use block2::RcBlock;
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
        UNNotificationRequest, UNNotificationSettings, UNUserNotificationCenter,
    };

    use super::{
        DesktopNotificationRequest, MACOS_NOTIFICATION_QUERY_TIMEOUT,
        MACOS_NOTIFICATION_REQUEST_TIMEOUT, MacOsNotificationPermissionState,
    };

    pub(super) fn macos_notification_center_available() -> bool {
        let Ok(current_exe) = std::env::current_exe() else {
            return false;
        };

        super::is_macos_app_bundle_executable_path(current_exe.as_path())
    }

    pub(super) fn macos_notification_permission_status()
    -> anyhow::Result<MacOsNotificationPermissionState> {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (tx, rx) = mpsc::channel();
        let completion = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            let settings = unsafe { settings.as_ref() };
            let _ = tx.send(permission_state_from_status(settings.authorizationStatus()));
        });
        center.getNotificationSettingsWithCompletionHandler(&completion);
        rx.recv_timeout(MACOS_NOTIFICATION_QUERY_TIMEOUT)
            .context("timed out waiting for macOS notification settings")
    }

    pub(super) fn request_macos_notification_permission()
    -> anyhow::Result<MacOsNotificationPermissionState> {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (tx, rx) = mpsc::channel();
        let completion = RcBlock::new(move |_granted, error: *mut NSError| {
            let result = if error.is_null() {
                Ok(())
            } else {
                let error = unsafe { error.as_ref() }
                    .map(format_nserror)
                    .unwrap_or_else(|| "unknown NSError".to_string());
                Err(anyhow!(
                    "macOS notification authorization request failed: {error}"
                ))
            };
            let _ = tx.send(result);
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert,
            &completion,
        );
        rx.recv_timeout(MACOS_NOTIFICATION_REQUEST_TIMEOUT)
            .context("timed out waiting for macOS notification authorization prompt")??;
        macos_notification_permission_status()
    }

    pub(super) fn show_desktop_notification(
        request: &DesktopNotificationRequest,
    ) -> anyhow::Result<()> {
        if !macos_notification_center_available() {
            return Ok(());
        }

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(request.title.as_str()));
        content.setBody(&NSString::from_str(request.body.as_str()));
        if let Some(thread_identifier) = request.thread_identifier.as_deref() {
            content.setThreadIdentifier(&NSString::from_str(thread_identifier));
        }

        let identifier = NSString::from_str(request.identifier.as_str());
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (tx, rx) = mpsc::channel();
        let completion = RcBlock::new(move |error: *mut NSError| {
            let result = if error.is_null() {
                Ok(())
            } else {
                let error = unsafe { error.as_ref() }
                    .map(format_nserror)
                    .unwrap_or_else(|| "unknown NSError".to_string());
                Err(anyhow!(
                    "macOS failed to enqueue desktop notification: {error}"
                ))
            };
            let _ = tx.send(result);
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&completion));
        rx.recv_timeout(MACOS_NOTIFICATION_QUERY_TIMEOUT)
            .context("timed out waiting for macOS desktop notification enqueue")?
    }

    fn permission_state_from_status(
        status: UNAuthorizationStatus,
    ) -> MacOsNotificationPermissionState {
        if status == UNAuthorizationStatus::Authorized
            || status == UNAuthorizationStatus::Provisional
            || status == UNAuthorizationStatus::Ephemeral
        {
            return MacOsNotificationPermissionState::Authorized;
        }
        if status == UNAuthorizationStatus::Denied {
            return MacOsNotificationPermissionState::Denied;
        }
        if status == UNAuthorizationStatus::NotDetermined {
            return MacOsNotificationPermissionState::NotDetermined;
        }
        MacOsNotificationPermissionState::Unknown
    }

    fn format_nserror(error: &NSError) -> String {
        let mut parts = vec![
            format!("domain={}", error.domain()),
            format!("code={}", error.code()),
            format!("description={}", error.localizedDescription()),
        ];
        if let Some(reason) = error.localizedFailureReason() {
            parts.push(format!("reason={reason}"));
        }
        if let Some(suggestion) = error.localizedRecoverySuggestion() {
            parts.push(format!("suggestion={suggestion}"));
        }
        parts.join(", ")
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use anyhow::Result;
    use notify_rust::Notification;

    use super::{DESKTOP_NOTIFICATION_APP_ID, DesktopNotificationRequest};

    pub(super) fn show_desktop_notification(request: &DesktopNotificationRequest) -> Result<()> {
        let mut notification = Notification::new();
        notification
            .summary(request.title.as_str())
            .body(request.body.as_str())
            .appname("Hunk");
        #[cfg(target_os = "windows")]
        notification.app_id(DESKTOP_NOTIFICATION_APP_ID);
        notification.show()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AiDesktopNotificationEvent, AiDesktopNotificationSnapshot, AiDesktopNotificationState,
        AiInProgressTurnKey, is_macos_app_bundle_executable_path,
        next_ai_desktop_notification_state,
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn snapshot() -> AiDesktopNotificationSnapshot {
        AiDesktopNotificationSnapshot {
            workspace_label: "Primary Checkout".to_string(),
            approval_request_thread_by_id: BTreeMap::new(),
            user_input_thread_by_id: BTreeMap::new(),
            plan_prompt_sequence_by_thread: BTreeMap::new(),
            in_progress_turns: BTreeSet::new(),
            thread_label_by_id: BTreeMap::new(),
        }
    }

    #[test]
    fn first_snapshot_seeds_state_without_notifying() {
        let (state, event) = next_ai_desktop_notification_state(None, snapshot());
        assert!(state.initialized);
        assert!(event.is_none());
    }

    #[test]
    fn approval_notifications_fire_once_for_new_request_ids() {
        let (state, _) = next_ai_desktop_notification_state(None, snapshot());
        let mut next = snapshot();
        next.approval_request_thread_by_id
            .insert("approval-1".to_string(), "thread-1".to_string());
        next.thread_label_by_id
            .insert("thread-1".to_string(), "Fix auth".to_string());

        let (state, event) = next_ai_desktop_notification_state(Some(&state), next.clone());
        assert_eq!(
            event,
            Some(AiDesktopNotificationEvent::ApprovalRequired {
                request_id: "approval-1".to_string(),
                thread_id: "thread-1".to_string(),
                label: "Fix auth".to_string(),
            })
        );

        let (_, event) = next_ai_desktop_notification_state(Some(&state), next);
        assert!(event.is_none());
    }

    #[test]
    fn user_input_notifications_fire_once_for_new_request_ids() {
        let (state, _) = next_ai_desktop_notification_state(None, snapshot());
        let mut next = snapshot();
        next.user_input_thread_by_id
            .insert("input-1".to_string(), "thread-1".to_string());
        next.thread_label_by_id
            .insert("thread-1".to_string(), "Fix auth".to_string());

        let (state, event) = next_ai_desktop_notification_state(Some(&state), next.clone());
        assert_eq!(
            event,
            Some(AiDesktopNotificationEvent::UserInputRequired {
                request_id: "input-1".to_string(),
                thread_id: "thread-1".to_string(),
                label: "Fix auth".to_string(),
            })
        );

        let (_, event) = next_ai_desktop_notification_state(Some(&state), next);
        assert!(event.is_none());
    }

    #[test]
    fn plan_ready_notifications_fire_when_source_sequence_changes() {
        let (state, _) = next_ai_desktop_notification_state(None, snapshot());
        let mut next = snapshot();
        next.plan_prompt_sequence_by_thread
            .insert("thread-1".to_string(), 7);
        next.thread_label_by_id
            .insert("thread-1".to_string(), "Fix auth".to_string());

        let (state, event) = next_ai_desktop_notification_state(Some(&state), next.clone());
        assert_eq!(
            event,
            Some(AiDesktopNotificationEvent::PlanReady {
                thread_id: "thread-1".to_string(),
                source_sequence: 7,
                label: "Fix auth".to_string(),
            })
        );

        let (_, event) = next_ai_desktop_notification_state(Some(&state), next.clone());
        assert!(event.is_none());

        let mut newer = next;
        newer
            .plan_prompt_sequence_by_thread
            .insert("thread-1".to_string(), 8);
        let (_, event) = next_ai_desktop_notification_state(Some(&state), newer);
        assert_eq!(
            event,
            Some(AiDesktopNotificationEvent::PlanReady {
                thread_id: "thread-1".to_string(),
                source_sequence: 8,
                label: "Fix auth".to_string(),
            })
        );
    }

    #[test]
    fn completed_turn_notifications_fire_when_in_progress_turns_finish() {
        let (state, _) = next_ai_desktop_notification_state(None, snapshot());
        let mut working = snapshot();
        working.in_progress_turns.insert(AiInProgressTurnKey {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        });
        working
            .thread_label_by_id
            .insert("thread-1".to_string(), "Fix auth".to_string());

        let (state, event) = next_ai_desktop_notification_state(Some(&state), working);
        assert!(event.is_none());

        let mut finished = snapshot();
        finished
            .thread_label_by_id
            .insert("thread-1".to_string(), "Fix auth".to_string());
        let (_, event) = next_ai_desktop_notification_state(Some(&state), finished);
        assert_eq!(
            event,
            Some(AiDesktopNotificationEvent::AgentFinished {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                label: "Fix auth".to_string(),
            })
        );
    }

    #[test]
    fn approvals_take_priority_over_completed_turns() {
        let initial = AiDesktopNotificationState {
            initialized: true,
            in_progress_turns: BTreeSet::from([AiInProgressTurnKey {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
            }]),
            thread_label_by_id: BTreeMap::from([("thread-1".to_string(), "Fix auth".to_string())]),
            workspace_label: "Primary Checkout".to_string(),
            ..AiDesktopNotificationState::default()
        };

        let mut next = snapshot();
        next.approval_request_thread_by_id
            .insert("approval-1".to_string(), "thread-1".to_string());
        next.thread_label_by_id
            .insert("thread-1".to_string(), "Fix auth".to_string());

        let (_, event) = next_ai_desktop_notification_state(Some(&initial), next);
        assert!(matches!(
            event,
            Some(AiDesktopNotificationEvent::ApprovalRequired { .. })
        ));
    }

    #[test]
    fn macos_bundle_path_detection_matches_packaged_layout() {
        assert!(is_macos_app_bundle_executable_path(std::path::Path::new(
            "/Applications/Hunk.app/Contents/MacOS/hunk_desktop",
        )));
        assert!(!is_macos_app_bundle_executable_path(std::path::Path::new(
            "/Volumes/hulk/dev/projects/hunk/target/release/hunk_desktop",
        )));
    }
}
