use hunk_browser::{
    BrowserAction, BrowserElement, BrowserElementRect, BrowserRuntime, BrowserSessionId,
    BrowserSnapshot, BrowserViewport,
};

#[test]
fn runtime_keeps_sessions_separate_by_thread() {
    let mut runtime = BrowserRuntime::new_disabled();

    runtime
        .ensure_session("thread-a")
        .replace_snapshot(snapshot(1, "https://example.com/a", 1));
    runtime
        .ensure_session("thread-b")
        .replace_snapshot(snapshot(1, "https://example.com/b", 2));

    assert_eq!(runtime.session_count(), 2);
    assert_eq!(
        runtime
            .session("thread-a")
            .and_then(|session| session.state().url.as_deref()),
        Some("https://example.com/a")
    );
    assert_eq!(
        runtime
            .session("thread-b")
            .and_then(|session| session.state().url.as_deref()),
        Some("https://example.com/b")
    );
}

#[test]
fn snapshot_element_validation_rejects_stale_epoch() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(snapshot(7, "https://example.com", 4));

    let error = session
        .validate_snapshot_element(6, 4)
        .expect_err("stale snapshot should be rejected");

    assert!(error.to_string().contains("snapshot is stale"));
}

#[test]
fn snapshot_element_validation_rejects_missing_index() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(snapshot(7, "https://example.com", 4));

    let error = session
        .validate_snapshot_element(7, 99)
        .expect_err("missing element index should be rejected");

    assert!(error.to_string().contains("element index 99"));
}

#[test]
fn preflight_action_accepts_current_indexed_action() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(snapshot(7, "https://example.com", 4));

    session
        .preflight_action(&BrowserAction::Click {
            snapshot_epoch: 7,
            index: 4,
        })
        .expect("current indexed action should pass preflight");
}

#[test]
fn preflight_action_rejects_stale_indexed_action() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(snapshot(7, "https://example.com", 4));

    let error = session
        .preflight_action(&BrowserAction::Type {
            snapshot_epoch: 6,
            index: 4,
            text: "hello".to_string(),
            clear: true,
        })
        .expect_err("stale indexed action should fail");

    assert!(error.to_string().contains("snapshot is stale"));
}

#[test]
fn preflight_action_accepts_navigation_without_snapshot() {
    let session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    session
        .preflight_action(&BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        })
        .expect("navigation should not require a snapshot");
}

fn snapshot(epoch: u64, url: &str, element_index: u32) -> BrowserSnapshot {
    BrowserSnapshot {
        epoch,
        url: Some(url.to_string()),
        title: Some("Example".to_string()),
        viewport: BrowserViewport {
            width: 1024,
            height: 768,
            device_scale_factor: 2.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        },
        elements: vec![BrowserElement {
            index: element_index,
            role: "button".to_string(),
            label: "Continue".to_string(),
            text: "Continue".to_string(),
            rect: BrowserElementRect {
                x: 10.0,
                y: 20.0,
                width: 80.0,
                height: 30.0,
            },
            selector: Some("#continue".to_string()),
        }],
    }
}
