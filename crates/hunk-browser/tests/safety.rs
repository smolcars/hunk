use hunk_browser::{
    BrowserAction, BrowserSafetyDecision, SensitiveBrowserAction, classify_browser_action,
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
