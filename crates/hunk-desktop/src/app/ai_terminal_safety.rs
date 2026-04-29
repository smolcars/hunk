#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalToolSafetyMode {
    Enforce,
    AllowSensitiveOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SensitiveTerminalAction {
    DestructiveCommand,
    MultiCommand,
    SecretInput,
    SystemConfiguration,
    Exfiltration,
    KillProcess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalToolConfirmation {
    pub kind: SensitiveTerminalAction,
    pub summary: String,
}

pub(crate) fn terminal_dynamic_tool_confirmation(
    params: &hunk_codex::protocol::DynamicToolCallParams,
) -> Option<TerminalToolConfirmation> {
    let Ok(request) = hunk_codex::terminal_tools::parse_terminal_dynamic_tool_request(params)
    else {
        return None;
    };
    let kind = classify_terminal_request(&request)?;
    Some(TerminalToolConfirmation {
        kind,
        summary: terminal_action_summary(&request),
    })
}

pub(crate) fn classify_terminal_request(
    request: &hunk_codex::terminal_tools::TerminalDynamicToolRequest,
) -> Option<SensitiveTerminalAction> {
    use hunk_codex::terminal_tools::TerminalDynamicToolRequest;

    match request {
        TerminalDynamicToolRequest::Run { command, .. } => classify_terminal_text(command),
        TerminalDynamicToolRequest::Type { text, .. }
        | TerminalDynamicToolRequest::Paste { text, .. } => {
            if terminal_text_looks_secret(text) {
                Some(SensitiveTerminalAction::SecretInput)
            } else if text.lines().count() > 1 {
                Some(SensitiveTerminalAction::MultiCommand)
            } else {
                classify_terminal_text(text)
            }
        }
        TerminalDynamicToolRequest::Kill { .. } => Some(SensitiveTerminalAction::KillProcess),
        _ => None,
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

fn classify_terminal_text(text: &str) -> Option<SensitiveTerminalAction> {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    if terminal_text_looks_destructive(normalized.as_str()) {
        return Some(SensitiveTerminalAction::DestructiveCommand);
    }
    if terminal_text_looks_exfiltrating(normalized.as_str()) {
        return Some(SensitiveTerminalAction::Exfiltration);
    }
    if terminal_text_looks_secret(text) {
        return Some(SensitiveTerminalAction::SecretInput);
    }
    if terminal_text_looks_system_config(normalized.as_str()) {
        return Some(SensitiveTerminalAction::SystemConfiguration);
    }
    if terminal_text_looks_multi_command(normalized.as_str()) {
        return Some(SensitiveTerminalAction::MultiCommand);
    }
    None
}

fn terminal_text_looks_destructive(text: &str) -> bool {
    text.contains("rm -rf")
        || text.contains("rm -fr")
        || text.contains("git reset --hard")
        || text.contains("git clean")
        || text.contains("mkfs")
        || text.contains("diskutil erase")
        || text.contains("chmod -r")
        || text.contains("chown -r")
        || text.contains("dd if=")
}

fn terminal_text_looks_exfiltrating(text: &str) -> bool {
    (text.contains("curl ") || text.contains("wget ") || text.contains("scp "))
        && (text.contains("$")
            || text.contains(" ~/.")
            || text.contains(" /etc/")
            || text.contains(" id_rsa")
            || text.contains("secret")
            || text.contains("token"))
}

fn terminal_text_looks_system_config(text: &str) -> bool {
    text.contains("sudo ")
        || text.contains("npm install -g")
        || text.contains("pnpm add -g")
        || text.contains("yarn global")
        || text.contains("brew install")
        || text.contains("launchctl ")
        || text.contains("systemctl ")
        || text.contains("defaults write")
}

fn terminal_text_looks_multi_command(text: &str) -> bool {
    text.lines().count() > 1
        || text.contains(" && ")
        || text.contains(" || ")
        || text.contains("; ")
        || text.contains(" | ")
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

fn summarize_terminal_text(text: &str) -> String {
    const MAX_CHARS: usize = 120;
    let mut summary = text.trim().replace(['\r', '\n'], " ");
    if summary.chars().count() > MAX_CHARS {
        summary = summary.chars().take(MAX_CHARS).collect::<String>();
        summary.push_str("...");
    }
    summary
}
