use hunk_browser::{
    BrowserAction, BrowserElement, BrowserElementRect, BrowserFrame, BrowserRuntime,
    BrowserSessionId, BrowserSnapshot, BrowserViewport,
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

#[test]
fn navigate_updates_session_state_and_invalidates_snapshot_epoch() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(snapshot(7, "https://old.example.com", 4));

    session.navigate("https://example.com");

    assert_eq!(session.state().url.as_deref(), Some("https://example.com"));
    assert_eq!(session.state().title, None);
    assert!(session.state().loading);
    assert_eq!(session.state().load_error, None);
    assert_eq!(session.state().snapshot_epoch, 8);
    assert!(session.latest_snapshot().elements.is_empty());
}

#[test]
fn load_error_stops_loading_and_is_cleared_by_navigation() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    session.navigate("https://example.com");
    session.set_load_error("network failed");

    assert!(!session.state().loading);
    assert_eq!(
        session.state().load_error.as_deref(),
        Some("network failed")
    );

    session.navigate("https://example.com/retry");

    assert!(session.state().loading);
    assert_eq!(session.state().load_error, None);
}

#[test]
fn runtime_applies_navigation_state_only_action() {
    let mut runtime = BrowserRuntime::new_disabled();

    runtime
        .apply_state_only_action(
            "thread-a",
            &BrowserAction::Navigate {
                url: "https://example.com".to_string(),
            },
        )
        .expect("navigation should update state");

    let session = runtime
        .session("thread-a")
        .expect("navigation should create a session");
    assert_eq!(session.state().url.as_deref(), Some("https://example.com"));
    assert!(session.state().loading);
}

#[test]
fn setting_latest_frame_updates_state_metadata_and_keeps_pixels() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    let frame = BrowserFrame::from_bgra(2, 1, 11, vec![0, 0, 255, 255, 0, 255, 0, 255])
        .expect("valid frame should be accepted");

    session.set_latest_frame(frame);

    assert_eq!(
        session.state().latest_frame.as_ref().map(|metadata| (
            metadata.width,
            metadata.height,
            metadata.frame_epoch
        )),
        Some((2, 1, 11))
    );
    assert_eq!(
        session
            .latest_frame()
            .expect("frame should be stored")
            .bgra(),
        &[0, 0, 255, 255, 0, 255, 0, 255]
    );
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
