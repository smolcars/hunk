use hunk_browser::{
    BrowserAction, BrowserSafetyDecision, REDACTED_BROWSER_SECRET, SensitiveBrowserAction,
    classify_browser_action, redact_browser_tool_text,
};

#[test]
fn normal_navigation_is_allowed() {
    let action = BrowserAction::Navigate {
        url: "https://example.com".to_string(),
    };

    assert_eq!(
        classify_browser_action(&action),
        BrowserSafetyDecision::Allow
    );
}

#[test]
fn external_protocol_navigation_requires_confirmation() {
    let action = BrowserAction::Navigate {
        url: "mailto:test@example.com".to_string(),
    };

    assert_eq!(
        classify_browser_action(&action),
        BrowserSafetyDecision::Prompt(SensitiveBrowserAction::ExternalProtocol)
    );
}

#[test]
fn file_transfer_navigation_requires_confirmation() {
    let action = BrowserAction::Navigate {
        url: "https://example.com/download/report.zip".to_string(),
    };

    assert_eq!(
        classify_browser_action(&action),
        BrowserSafetyDecision::Prompt(SensitiveBrowserAction::FileTransfer)
    );
}

#[test]
fn payment_navigation_requires_confirmation() {
    let action = BrowserAction::Navigate {
        url: "https://example.com/checkout".to_string(),
    };

    assert_eq!(
        classify_browser_action(&action),
        BrowserSafetyDecision::Prompt(SensitiveBrowserAction::PaymentOrPurchase)
    );
}

#[test]
fn likely_secret_entry_requires_confirmation() {
    let action = BrowserAction::Type {
        snapshot_epoch: 1,
        index: 4,
        text: "verylongsecretvalue".to_string(),
        clear: true,
    };

    assert_eq!(
        classify_browser_action(&action),
        BrowserSafetyDecision::Prompt(SensitiveBrowserAction::CredentialEntry)
    );
}

#[test]
fn submit_key_requires_confirmation() {
    let action = BrowserAction::Press {
        keys: "Enter".to_string(),
    };

    assert_eq!(
        classify_browser_action(&action),
        BrowserSafetyDecision::Prompt(SensitiveBrowserAction::HighRiskFormSubmit)
    );
}

#[test]
fn browser_tool_text_redacts_likely_secret_tokens() {
    let redacted = redact_browser_tool_text("token=abc123def456 keep");

    assert_eq!(redacted, format!("{REDACTED_BROWSER_SECRET} keep"));
}

#[test]
fn browser_tool_text_keeps_normal_visible_text() {
    let redacted = redact_browser_tool_text("Checkout button");

    assert_eq!(redacted, "Checkout button");
}
