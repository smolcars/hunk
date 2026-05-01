use crate::android::AndroidAction;

pub const REDACTED_MOBILE_SECRET: &str = "[redacted]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveMobileAction {
    CredentialEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileSafetyDecision {
    Allow,
    Prompt(SensitiveMobileAction),
}

pub fn classify_android_action(action: &AndroidAction) -> MobileSafetyDecision {
    match action {
        AndroidAction::Type { text, .. } if looks_like_secret(text) => {
            MobileSafetyDecision::Prompt(SensitiveMobileAction::CredentialEntry)
        }
        _ => MobileSafetyDecision::Allow,
    }
}

pub fn redact_mobile_tool_text(text: &str) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }

    text.split_whitespace()
        .map(|token| {
            if looks_like_secret_token(token) {
                REDACTED_MOBILE_SECRET
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("xoxb-")
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("secret=")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
}
