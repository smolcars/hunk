use serde::{Deserialize, Serialize};

const TERMINAL_AUTO_REVIEW_POLICY: &str =
    include_str!("../../../../docs/AI_TERMINAL_AUTO_REVIEW_POLICY.md");
#[allow(dead_code)]
const MAX_REVIEW_EVIDENCE_ITEMS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalToolSafetyMode {
    Enforce,
    AllowSensitiveOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TerminalActionKind {
    ReadState,
    SessionLifecycle,
    TabManagement,
    InputWrite,
    CommandExecution,
    Paste,
    KeyPress,
    Resize,
    ProcessKill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TerminalRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub(crate) enum TerminalUserAuthorization {
    Unknown,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub(crate) enum TerminalAutoReviewOutcome {
    Allow,
    Confirm,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TerminalShellKind {
    Posix,
    PowerShell,
    Cmd,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TerminalPlatform {
    MacOS,
    Linux,
    Windows,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalSafetyContext {
    pub thread_id: String,
    pub workspace_key: Option<String>,
    pub cwd: Option<String>,
    pub tab_id: Option<usize>,
    pub active_tab_id: Option<usize>,
    pub available_tab_ids: Vec<usize>,
    pub target_tab_title: Option<String>,
    pub target_tab_status: Option<String>,
    pub target_tab_exit_code: Option<i32>,
    pub target_tab_last_command: Option<String>,
    pub shell_session_available: bool,
    pub shell: TerminalShellKind,
    pub platform: TerminalPlatform,
    pub user_intent: Option<String>,
    pub visible_snapshot: Option<String>,
    pub recent_logs: Vec<TerminalAutoReviewLogEntry>,
    pub recent_thread_context: Vec<TerminalAutoReviewLogEntry>,
    pub browser_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TerminalAutoReviewLogEntry {
    pub sequence: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TerminalAutoReviewRequest {
    pub action_kind: TerminalActionKind,
    pub summary: String,
    pub tab_id: Option<usize>,
    pub active_tab_id: Option<usize>,
    pub tab_title: Option<String>,
    pub tab_status: Option<String>,
    pub tab_exit_code: Option<i32>,
    pub tab_last_command: Option<String>,
    pub command: Option<String>,
    pub input: Option<String>,
    pub keys: Option<String>,
    pub cwd: Option<String>,
    pub workspace_root: Option<String>,
    pub shell: TerminalShellKind,
    pub platform: TerminalPlatform,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub user_intent: Option<String>,
    pub visible_snapshot: Option<String>,
    pub recent_logs: Vec<TerminalAutoReviewLogEntry>,
    pub recent_thread_context: Vec<TerminalAutoReviewLogEntry>,
    pub browser_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct TerminalAutoReviewAssessment {
    pub risk_level: TerminalRiskLevel,
    pub user_authorization: TerminalUserAuthorization,
    pub outcome: TerminalAutoReviewOutcome,
    pub rationale: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
struct TerminalAutoReviewAssessmentWire {
    risk_level: Option<TerminalRiskLevel>,
    user_authorization: Option<TerminalUserAuthorization>,
    outcome: TerminalAutoReviewOutcome,
    rationale: Option<String>,
    #[serde(default)]
    evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct TerminalAutoReviewParseError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalPrefilterDecision {
    Allow {
        summary: String,
        evidence: Vec<String>,
    },
    ReviewRequired {
        request: Box<TerminalAutoReviewRequest>,
        fallback_risk_level: TerminalRiskLevel,
        evidence: Vec<String>,
    },
    Reject(TerminalToolRejection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum TerminalAutoReviewPolicyDecision {
    Execute {
        summary: String,
        evidence: Vec<String>,
    },
    Confirm(TerminalToolConfirmation),
    Reject(TerminalToolRejection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalToolConfirmation {
    pub risk_level: TerminalRiskLevel,
    pub user_authorization: TerminalUserAuthorization,
    pub outcome: TerminalAutoReviewOutcome,
    pub summary: String,
    pub rationale: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalToolRejection {
    pub risk_level: TerminalRiskLevel,
    pub user_authorization: TerminalUserAuthorization,
    pub outcome: TerminalAutoReviewOutcome,
    pub summary: String,
    pub rationale: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalToolPreflight {
    Confirm(TerminalToolConfirmation),
    Reject(TerminalToolRejection),
}

#[allow(dead_code)]
pub(crate) struct TerminalAutoReviewPolicy;

#[allow(dead_code)]
impl TerminalAutoReviewPolicy {
    pub(crate) fn decide(
        review_request: &TerminalAutoReviewRequest,
        assessment: TerminalAutoReviewAssessment,
    ) -> TerminalAutoReviewPolicyDecision {
        let summary = redact_terminal_tool_text(review_request.summary.as_str());
        let rationale = redact_terminal_tool_text(assessment.rationale.as_str());
        let evidence = redact_evidence(assessment.evidence);
        match assessment.outcome {
            TerminalAutoReviewOutcome::Allow
                if matches!(
                    assessment.risk_level,
                    TerminalRiskLevel::Low | TerminalRiskLevel::Medium
                ) =>
            {
                TerminalAutoReviewPolicyDecision::Execute { summary, evidence }
            }
            TerminalAutoReviewOutcome::Allow | TerminalAutoReviewOutcome::Confirm => {
                TerminalAutoReviewPolicyDecision::Confirm(TerminalToolConfirmation {
                    risk_level: assessment.risk_level,
                    user_authorization: assessment.user_authorization,
                    outcome: assessment.outcome,
                    summary,
                    rationale,
                    evidence,
                })
            }
            TerminalAutoReviewOutcome::Deny => {
                TerminalAutoReviewPolicyDecision::Reject(TerminalToolRejection {
                    risk_level: assessment.risk_level,
                    user_authorization: assessment.user_authorization,
                    outcome: assessment.outcome,
                    summary,
                    rationale,
                    evidence,
                })
            }
        }
    }
}

pub(crate) fn evaluate_terminal_action_prefilter(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
    context: &TerminalSafetyContext,
    turn_id: Option<&str>,
) -> TerminalPrefilterDecision {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    if let Some(rejection) = terminal_context_rejection(request, context) {
        return TerminalPrefilterDecision::Reject(rejection);
    }

    match request {
        TerminalDynamicToolRequest::Tabs
        | TerminalDynamicToolRequest::Snapshot { .. }
        | TerminalDynamicToolRequest::Logs { .. }
        | TerminalDynamicToolRequest::Scroll { .. } => TerminalPrefilterDecision::Allow {
            summary: terminal_action_summary(request),
            evidence: vec!["read-only terminal inspection".to_string()],
        },
        TerminalDynamicToolRequest::Open { .. }
        | TerminalDynamicToolRequest::NewTab { .. }
        | TerminalDynamicToolRequest::SelectTab { .. }
        | TerminalDynamicToolRequest::CloseTab { .. }
        | TerminalDynamicToolRequest::Resize { .. } => TerminalPrefilterDecision::Allow {
            summary: terminal_action_summary(request),
            evidence: vec!["terminal UI-only action".to_string()],
        },
        TerminalDynamicToolRequest::Run { command, .. } if command.trim().is_empty() => {
            TerminalPrefilterDecision::Reject(TerminalToolRejection {
                risk_level: TerminalRiskLevel::Low,
                user_authorization: TerminalUserAuthorization::Unknown,
                outcome: TerminalAutoReviewOutcome::Deny,
                summary: "Run command".to_string(),
                rationale: "Terminal command is empty.".to_string(),
                evidence: vec!["empty command".to_string()],
            })
        }
        TerminalDynamicToolRequest::Type { text, .. }
        | TerminalDynamicToolRequest::Paste { text, .. }
            if text.is_empty() =>
        {
            TerminalPrefilterDecision::Reject(TerminalToolRejection {
                risk_level: TerminalRiskLevel::Low,
                user_authorization: TerminalUserAuthorization::Unknown,
                outcome: TerminalAutoReviewOutcome::Deny,
                summary: terminal_action_summary(request),
                rationale: "Terminal input is empty.".to_string(),
                evidence: vec!["empty terminal input".to_string()],
            })
        }
        TerminalDynamicToolRequest::Press { keys, .. } if keys.trim().is_empty() => {
            TerminalPrefilterDecision::Reject(TerminalToolRejection {
                risk_level: TerminalRiskLevel::Low,
                user_authorization: TerminalUserAuthorization::Unknown,
                outcome: TerminalAutoReviewOutcome::Deny,
                summary: "Press terminal keys".to_string(),
                rationale: "Terminal key sequence is empty.".to_string(),
                evidence: vec!["empty key sequence".to_string()],
            })
        }
        _ => {
            let action = terminal_auto_review_request(request, context, turn_id);
            let (fallback_risk_level, evidence) = terminal_prefilter_evidence(request);
            if terminal_request_looks_like_secret_exfiltration(request) {
                return TerminalPrefilterDecision::Reject(TerminalToolRejection {
                    risk_level: TerminalRiskLevel::Critical,
                    user_authorization: TerminalUserAuthorization::Unknown,
                    outcome: TerminalAutoReviewOutcome::Deny,
                    summary: redact_terminal_tool_text(action.summary.as_str()),
                    rationale: "Terminal action appears to send credential-looking input to an external sink.".to_string(),
                    evidence: redact_evidence(vec![
                        "credential-looking input present".to_string(),
                        "network sink present".to_string(),
                    ]),
                });
            }

            TerminalPrefilterDecision::ReviewRequired {
                request: Box::new(action),
                fallback_risk_level,
                evidence,
            }
        }
    }
}

fn terminal_context_rejection(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
    context: &TerminalSafetyContext,
) -> Option<TerminalToolRejection> {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    if context.workspace_key.is_none() {
        return Some(TerminalToolRejection {
            risk_level: TerminalRiskLevel::Low,
            user_authorization: TerminalUserAuthorization::Unknown,
            outcome: TerminalAutoReviewOutcome::Deny,
            summary: terminal_action_summary(request),
            rationale: "Terminal action cannot be reviewed without an active workspace."
                .to_string(),
            evidence: vec!["missing workspace context".to_string()],
        });
    }

    if let Some(tab_id) = request_tab_id(request)
        && !context.available_tab_ids.contains(&tab_id)
    {
        return Some(TerminalToolRejection {
            risk_level: TerminalRiskLevel::Low,
            user_authorization: TerminalUserAuthorization::Unknown,
            outcome: TerminalAutoReviewOutcome::Deny,
            summary: terminal_action_summary(request),
            rationale: format!("Terminal tab {tab_id} is not available."),
            evidence: vec!["stale or missing terminal tab".to_string()],
        });
    }

    if matches!(
        request,
        TerminalDynamicToolRequest::Type { .. }
            | TerminalDynamicToolRequest::Paste { .. }
            | TerminalDynamicToolRequest::Press { .. }
            | TerminalDynamicToolRequest::Kill { .. }
    ) && !context.shell_session_available
    {
        return Some(TerminalToolRejection {
            risk_level: TerminalRiskLevel::Low,
            user_authorization: TerminalUserAuthorization::Unknown,
            outcome: TerminalAutoReviewOutcome::Deny,
            summary: terminal_action_summary(request),
            rationale: "No running terminal session is available for this action.".to_string(),
            evidence: vec!["terminal session unavailable".to_string()],
        });
    }

    None
}

pub(crate) fn terminal_dynamic_tool_preflight(
    params: &hunk_codex::protocol::DynamicToolCallParams,
    context: &TerminalSafetyContext,
) -> Option<TerminalToolPreflight> {
    let Ok(request) = hunk_codex::terminal_tools::parse_terminal_dynamic_tool_request(params)
    else {
        return None;
    };
    match evaluate_terminal_action_prefilter(&request, context, Some(params.turn_id.as_str())) {
        TerminalPrefilterDecision::Allow { .. } => None,
        TerminalPrefilterDecision::ReviewRequired {
            request,
            fallback_risk_level,
            mut evidence,
        } => {
            evidence.push(
                "terminal auto-review is not available yet; user confirmation is required"
                    .to_string(),
            );
            Some(TerminalToolPreflight::Confirm(TerminalToolConfirmation {
                risk_level: fallback_risk_level,
                user_authorization: TerminalUserAuthorization::Unknown,
                outcome: TerminalAutoReviewOutcome::Confirm,
                summary: redact_terminal_tool_text(request.summary.as_str()),
                rationale: "Terminal action requires auto-review before unattended execution."
                    .to_string(),
                evidence: redact_evidence(evidence),
            }))
        }
        TerminalPrefilterDecision::Reject(rejection) => {
            Some(TerminalToolPreflight::Reject(rejection))
        }
    }
}

#[allow(dead_code)]
pub(crate) fn parse_terminal_auto_review_assessment(
    output: &str,
) -> Result<TerminalAutoReviewAssessment, TerminalAutoReviewParseError> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Err(TerminalAutoReviewParseError {
            message: "reviewer output was empty".to_string(),
        });
    }

    let wire: TerminalAutoReviewAssessmentWire =
        serde_json::from_str(trimmed).map_err(|error| TerminalAutoReviewParseError {
            message: format!("reviewer output was not valid terminal auto-review JSON: {error}"),
        })?;
    let rationale = wire
        .rationale
        .map(|rationale| redact_terminal_tool_text(rationale.trim()))
        .filter(|rationale| !rationale.is_empty())
        .ok_or_else(|| TerminalAutoReviewParseError {
            message: "reviewer output did not include a rationale".to_string(),
        })?;

    Ok(TerminalAutoReviewAssessment {
        risk_level: wire.risk_level.unwrap_or(TerminalRiskLevel::High),
        user_authorization: wire
            .user_authorization
            .unwrap_or(TerminalUserAuthorization::Unknown),
        outcome: wire.outcome,
        rationale,
        evidence: redact_evidence(
            wire.evidence
                .into_iter()
                .take(MAX_REVIEW_EVIDENCE_ITEMS)
                .collect(),
        ),
    })
}

#[allow(dead_code)]
pub(crate) fn terminal_auto_review_parse_failure_confirmation(
    review_request: &TerminalAutoReviewRequest,
    fallback_risk_level: TerminalRiskLevel,
    error: &TerminalAutoReviewParseError,
) -> TerminalToolConfirmation {
    TerminalToolConfirmation {
        risk_level: fallback_risk_level,
        user_authorization: TerminalUserAuthorization::Unknown,
        outcome: TerminalAutoReviewOutcome::Confirm,
        summary: redact_terminal_tool_text(review_request.summary.as_str()),
        rationale: "Terminal auto-review failed closed to user confirmation.".to_string(),
        evidence: redact_evidence(vec![error.message.clone()]),
    }
}

pub(crate) fn terminal_auto_review_prompt(request: &TerminalAutoReviewRequest) -> String {
    let request_json = serde_json::to_string_pretty(request)
        .unwrap_or_else(|_| "{\"error\":\"failed to serialize request\"}".to_string());
    format!(
        "{}\n\nReview this exact terminal action and return only JSON.\n\nTerminal action request:\n```json\n{}\n```\n",
        TERMINAL_AUTO_REVIEW_POLICY, request_json
    )
}

pub(crate) fn terminal_auto_review_output_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["outcome", "rationale"],
        "properties": {
            "riskLevel": {
                "type": "string",
                "enum": ["low", "medium", "high", "critical"]
            },
            "userAuthorization": {
                "type": "string",
                "enum": ["unknown", "low", "medium", "high"]
            },
            "outcome": {
                "type": "string",
                "enum": ["allow", "confirm", "deny"]
            },
            "rationale": {
                "type": "string"
            },
            "evidence": {
                "type": "array",
                "items": {
                    "type": "string"
                }
            }
        }
    })
}

pub(crate) fn terminal_auto_review_request(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
    context: &TerminalSafetyContext,
    turn_id: Option<&str>,
) -> TerminalAutoReviewRequest {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    let action_kind = terminal_action_kind(request);
    let (command, input, keys) = match request {
        TerminalDynamicToolRequest::Run { command, .. } => {
            (Some(redact_terminal_tool_text(command)), None, None)
        }
        TerminalDynamicToolRequest::Type { text, .. }
        | TerminalDynamicToolRequest::Paste { text, .. } => {
            (None, Some(redact_terminal_tool_text(text)), None)
        }
        TerminalDynamicToolRequest::Press { keys, .. } => (None, None, Some(keys.clone())),
        _ => (None, None, None),
    };

    TerminalAutoReviewRequest {
        action_kind,
        summary: redact_terminal_tool_text(terminal_action_summary(request).as_str()),
        tab_id: request_tab_id(request).or(context.tab_id),
        active_tab_id: context.active_tab_id,
        tab_title: context
            .target_tab_title
            .as_ref()
            .map(|title| redact_terminal_tool_text(title)),
        tab_status: context.target_tab_status.clone(),
        tab_exit_code: context.target_tab_exit_code,
        tab_last_command: context
            .target_tab_last_command
            .as_ref()
            .map(|command| redact_terminal_tool_text(command)),
        command,
        input,
        keys,
        cwd: context.cwd.clone(),
        workspace_root: context.workspace_key.clone(),
        shell: context.shell,
        platform: context.platform,
        thread_id: context.thread_id.clone(),
        turn_id: turn_id.map(str::to_string),
        user_intent: context
            .user_intent
            .as_ref()
            .map(|intent| redact_terminal_tool_text(intent)),
        visible_snapshot: context
            .visible_snapshot
            .as_ref()
            .map(|snapshot| redact_terminal_tool_text(snapshot)),
        recent_logs: context
            .recent_logs
            .iter()
            .map(|entry| TerminalAutoReviewLogEntry {
                sequence: entry.sequence,
                text: redact_terminal_tool_text(entry.text.as_str()),
            })
            .collect(),
        recent_thread_context: context
            .recent_thread_context
            .iter()
            .map(|entry| TerminalAutoReviewLogEntry {
                sequence: entry.sequence,
                text: redact_terminal_tool_text(entry.text.as_str()),
            })
            .collect(),
        browser_context: context
            .browser_context
            .as_ref()
            .map(|context| redact_terminal_tool_text(context)),
    }
}

pub(crate) fn terminal_action_summary(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> String {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    match request {
        TerminalDynamicToolRequest::Open { .. } => "Open terminal".to_string(),
        TerminalDynamicToolRequest::Tabs => "Read terminal tabs".to_string(),
        TerminalDynamicToolRequest::NewTab { .. } => "Create terminal tab".to_string(),
        TerminalDynamicToolRequest::SelectTab { tab_id } => {
            format!("Select terminal tab {}", tab_id.get())
        }
        TerminalDynamicToolRequest::CloseTab { tab_id } => {
            format!("Close terminal tab {}", tab_id.get())
        }
        TerminalDynamicToolRequest::Snapshot { .. } => "Read terminal snapshot".to_string(),
        TerminalDynamicToolRequest::Logs { .. } => "Read terminal logs".to_string(),
        TerminalDynamicToolRequest::Run { command, .. } => {
            format!("Run command: {}", summarize_terminal_text(command))
        }
        TerminalDynamicToolRequest::Type { text, .. } => {
            format!("Type {} character(s)", text.chars().count())
        }
        TerminalDynamicToolRequest::Paste { text, .. } => {
            format!("Paste {} character(s)", text.chars().count())
        }
        TerminalDynamicToolRequest::Press { keys, .. } => format!("Press {keys}"),
        TerminalDynamicToolRequest::Scroll { lines, .. } => format!("Scroll {lines} line(s)"),
        TerminalDynamicToolRequest::Resize { rows, cols, .. } => {
            format!("Resize terminal to {rows}x{cols}")
        }
        TerminalDynamicToolRequest::Kill { .. } => "Stop terminal process".to_string(),
    }
}

pub(crate) fn redact_terminal_tool_text(text: &str) -> String {
    text.split_whitespace()
        .map(|token| {
            if terminal_text_looks_secret(token) {
                "[redacted]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn terminal_action_kind(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> TerminalActionKind {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    match request {
        TerminalDynamicToolRequest::Tabs
        | TerminalDynamicToolRequest::Snapshot { .. }
        | TerminalDynamicToolRequest::Logs { .. }
        | TerminalDynamicToolRequest::Scroll { .. } => TerminalActionKind::ReadState,
        TerminalDynamicToolRequest::Open { .. } => TerminalActionKind::SessionLifecycle,
        TerminalDynamicToolRequest::NewTab { .. }
        | TerminalDynamicToolRequest::SelectTab { .. }
        | TerminalDynamicToolRequest::CloseTab { .. } => TerminalActionKind::TabManagement,
        TerminalDynamicToolRequest::Run { .. } => TerminalActionKind::CommandExecution,
        TerminalDynamicToolRequest::Type { .. } => TerminalActionKind::InputWrite,
        TerminalDynamicToolRequest::Paste { .. } => TerminalActionKind::Paste,
        TerminalDynamicToolRequest::Press { .. } => TerminalActionKind::KeyPress,
        TerminalDynamicToolRequest::Resize { .. } => TerminalActionKind::Resize,
        TerminalDynamicToolRequest::Kill { .. } => TerminalActionKind::ProcessKill,
    }
}

fn terminal_prefilter_evidence(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> (TerminalRiskLevel, Vec<String>) {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    match request {
        TerminalDynamicToolRequest::Kill { .. } => (
            TerminalRiskLevel::Medium,
            vec!["process control action".to_string()],
        ),
        TerminalDynamicToolRequest::Press { keys, .. } if terminal_key_is_process_control(keys) => {
            (
                TerminalRiskLevel::Medium,
                vec!["process-control key sequence".to_string()],
            )
        }
        TerminalDynamicToolRequest::Run { command, .. }
            if terminal_text_looks_secret(command.as_str()) =>
        {
            (
                TerminalRiskLevel::Critical,
                vec!["credential-looking command text".to_string()],
            )
        }
        TerminalDynamicToolRequest::Type { text, .. }
        | TerminalDynamicToolRequest::Paste { text, .. }
            if terminal_text_looks_secret(text.as_str()) =>
        {
            (
                TerminalRiskLevel::Critical,
                vec!["credential-looking terminal input".to_string()],
            )
        }
        TerminalDynamicToolRequest::Run { command, .. }
            if terminal_text_has_multiple_lines_or_segments(command.as_str()) =>
        {
            (
                TerminalRiskLevel::Medium,
                vec!["multi-command or multi-line terminal input".to_string()],
            )
        }
        TerminalDynamicToolRequest::Paste { text, .. }
            if terminal_text_has_multiple_lines_or_segments(text.as_str()) =>
        {
            (
                TerminalRiskLevel::Medium,
                vec!["multi-line terminal paste".to_string()],
            )
        }
        TerminalDynamicToolRequest::Run { command, .. }
            if terminal_text_has_network_sink(command) =>
        {
            (
                TerminalRiskLevel::Medium,
                vec!["network-capable command".to_string()],
            )
        }
        TerminalDynamicToolRequest::Run { .. }
        | TerminalDynamicToolRequest::Type { .. }
        | TerminalDynamicToolRequest::Paste { .. }
        | TerminalDynamicToolRequest::Press { .. } => (
            TerminalRiskLevel::Medium,
            vec!["terminal write requires auto-review".to_string()],
        ),
        _ => (
            TerminalRiskLevel::Low,
            vec!["terminal action requires auto-review".to_string()],
        ),
    }
}

fn terminal_request_looks_like_secret_exfiltration(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> bool {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    match request {
        TerminalDynamicToolRequest::Run { command, .. } => {
            terminal_text_looks_secret(command) && terminal_text_has_network_sink(command)
        }
        TerminalDynamicToolRequest::Type { text, .. }
        | TerminalDynamicToolRequest::Paste { text, .. } => {
            terminal_text_looks_secret(text) && terminal_text_has_network_sink(text)
        }
        _ => false,
    }
}

fn request_tab_id(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> Option<usize> {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;
    match request {
        TerminalDynamicToolRequest::Open { tab_id }
        | TerminalDynamicToolRequest::Snapshot { tab_id, .. }
        | TerminalDynamicToolRequest::Logs { tab_id, .. }
        | TerminalDynamicToolRequest::Run { tab_id, .. }
        | TerminalDynamicToolRequest::Type { tab_id, .. }
        | TerminalDynamicToolRequest::Paste { tab_id, .. }
        | TerminalDynamicToolRequest::Press { tab_id, .. }
        | TerminalDynamicToolRequest::Scroll { tab_id, .. }
        | TerminalDynamicToolRequest::Resize { tab_id, .. }
        | TerminalDynamicToolRequest::Kill { tab_id } => tab_id.map(|id| id.get()),
        TerminalDynamicToolRequest::SelectTab { tab_id }
        | TerminalDynamicToolRequest::CloseTab { tab_id } => Some(tab_id.get()),
        TerminalDynamicToolRequest::Tabs | TerminalDynamicToolRequest::NewTab { .. } => None,
    }
}

fn terminal_text_has_multiple_lines_or_segments(text: &str) -> bool {
    text.lines().filter(|line| !line.trim().is_empty()).count() > 1
        || text.contains("&&")
        || text.contains("||")
        || text.contains(";")
        || text.contains(" | ")
}

fn terminal_text_has_network_sink(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("curl ")
        || normalized.contains("wget ")
        || normalized.contains("scp ")
        || normalized.contains("https://")
        || normalized.contains("http://")
}

fn terminal_key_is_process_control(keys: &str) -> bool {
    matches!(
        keys.trim().replace('+', "-").to_ascii_lowercase().as_str(),
        "ctrl-c" | "control-c" | "ctrl-d" | "control-d"
    )
}

fn terminal_text_looks_secret(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("api_key")
        || normalized.contains("apikey")
        || normalized.contains("token=")
        || normalized.contains("password=")
        || normalized.contains("secret=")
        || normalized.contains("bearer ")
        || normalized.contains("-----begin ")
}

fn redact_evidence(evidence: Vec<String>) -> Vec<String> {
    evidence
        .into_iter()
        .map(|item| redact_terminal_tool_text(item.as_str()))
        .collect()
}

fn summarize_terminal_text(text: &str) -> String {
    const MAX_CHARS: usize = 120;
    let mut summary = text.trim().replace(['\r', '\n'], " ");
    if summary.chars().count() > MAX_CHARS {
        summary = summary.chars().take(MAX_CHARS).collect::<String>();
        summary.push_str("...");
    }
    summary
}
