use crate::session::BrowserAction;

pub const REDACTED_BROWSER_SECRET: &str = "[redacted]";

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
        BrowserAction::Navigate { url } if looks_like_payment_or_purchase(url) => {
            BrowserSafetyDecision::Prompt(SensitiveBrowserAction::PaymentOrPurchase)
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

pub fn redact_browser_tool_text(text: &str) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }

    text.split_whitespace()
        .map(|token| {
            if looks_like_secret_token(token) {
                REDACTED_BROWSER_SECRET
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

fn looks_like_payment_or_purchase(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("checkout")
        || lower.contains("purchase")
        || lower.contains("payment")
        || lower.contains("billing")
}

fn looks_like_secret(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.len() >= 12 && !trimmed.contains(char::is_whitespace)
}

fn looks_like_secret_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|value: char| !value.is_ascii_alphanumeric() && value != '_');
    if trimmed.len() < 12 {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("xoxb-")
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("secret=")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
    {
        return true;
    }

    trimmed.chars().any(|value| value.is_ascii_alphabetic())
        && trimmed.chars().any(|value| value.is_ascii_digit())
        && trimmed.chars().any(|value| !value.is_ascii_alphanumeric())
}

fn looks_like_form_submit(keys: &str) -> bool {
    let normalized = keys.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "enter" | "return" | "cmd+enter" | "ctrl+enter"
    )
}
