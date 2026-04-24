use crate::session::BrowserAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveBrowserAction {
    CredentialEntry,
    PaymentOrPurchase,
    FileTransfer,
    ExternalProtocol,
    HighRiskFormSubmit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserSafetyDecision {
    Allow,
    Prompt(SensitiveBrowserAction),
}

pub fn classify_browser_action(action: &BrowserAction) -> BrowserSafetyDecision {
    match action {
        BrowserAction::Navigate { url } if has_external_protocol(url) => {
            BrowserSafetyDecision::Prompt(SensitiveBrowserAction::ExternalProtocol)
        }
        BrowserAction::Navigate { url } if looks_like_file_transfer(url) => {
            BrowserSafetyDecision::Prompt(SensitiveBrowserAction::FileTransfer)
        }
        BrowserAction::Type { text, .. } if looks_like_secret(text) => {
            BrowserSafetyDecision::Prompt(SensitiveBrowserAction::CredentialEntry)
        }
        BrowserAction::Press { keys } if looks_like_form_submit(keys) => {
            BrowserSafetyDecision::Prompt(SensitiveBrowserAction::HighRiskFormSubmit)
        }
        _ => BrowserSafetyDecision::Allow,
    }
}

fn has_external_protocol(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "about"
    )
}

fn looks_like_file_transfer(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("download") || lower.contains("upload")
}

fn looks_like_secret(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.len() >= 12 && !trimmed.contains(char::is_whitespace)
}

fn looks_like_form_submit(keys: &str) -> bool {
    let normalized = keys.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "enter" | "return" | "cmd+enter" | "ctrl+enter"
    )
}
