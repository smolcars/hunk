use hunk_browser::{
    BrowserElement, BrowserRuntime, BrowserSessionId, BrowserSnapshot, BrowserViewport,
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
            selector: Some("#continue".to_string()),
        }],
    }
}
