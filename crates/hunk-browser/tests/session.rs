use hunk_browser::{
    BrowserAction, BrowserConsoleLevel, BrowserElement, BrowserElementRect, BrowserError,
    BrowserFrame, BrowserHistoryDirection, BrowserRuntime, BrowserSessionId, BrowserSnapshot,
    BrowserTabId, BrowserViewport,
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
    assert!(session.state().can_go_back);
    assert!(!session.state().can_go_forward);
    assert_eq!(session.state().snapshot_epoch, 8);
    assert!(session.latest_snapshot().elements.is_empty());
    assert_eq!(session.state().tabs.len(), 1);
    assert_eq!(
        session.state().tabs[0].url.as_deref(),
        Some("https://example.com")
    );
    assert_eq!(session.state().tabs[0].snapshot_epoch, 8);
}

#[test]
fn new_session_starts_with_one_active_tab() {
    let session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    assert_eq!(session.active_tab_id().as_str(), "tab-1");
    assert_eq!(session.tab_summaries().len(), 1);
    assert_eq!(session.tab_summaries()[0].tab_id, *session.active_tab_id());
    assert_eq!(session.state().url.as_deref(), Some("about:blank"));
    assert_eq!(
        session.tab_summaries()[0].url.as_deref(),
        Some("about:blank")
    );
    assert_eq!(session.state().active_tab_id, *session.active_tab_id());
}

#[test]
fn create_blank_tab_uses_about_blank_without_loading() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.navigate("https://example.com/one");
    session.set_loading(false);

    let tab_id = session.create_tab(None, true);

    assert_eq!(session.active_tab_id(), &tab_id);
    assert_eq!(session.state().url.as_deref(), Some("about:blank"));
    assert!(!session.state().loading);
    assert_eq!(
        session
            .tab_summaries()
            .iter()
            .find(|tab| tab.tab_id == tab_id)
            .and_then(|tab| tab.url.as_deref()),
        Some("about:blank")
    );
}

#[test]
fn create_tab_can_activate_initial_url() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.navigate("https://example.com/one");
    session.set_loading(false);

    let tab_id = session.create_tab(Some("https://example.com/two".to_string()), true);

    assert_eq!(session.active_tab_id(), &tab_id);
    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/two")
    );
    assert!(session.state().loading);
    assert_eq!(session.tab_summaries().len(), 2);
    assert_eq!(
        session.tab_summaries()[0].url.as_deref(),
        Some("https://example.com/one")
    );
    assert_eq!(session.tab_summaries()[1].tab_id, tab_id);
}

#[test]
fn select_tab_restores_tab_summary_state() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.navigate("https://example.com/one");
    session.set_loading(false);
    let first_tab_id = session.active_tab_id().clone();
    let second_tab_id = session.create_tab(Some("https://example.com/two".to_string()), true);

    session
        .select_tab(&first_tab_id)
        .expect("existing tab should be selectable");

    assert_eq!(session.active_tab_id(), &first_tab_id);
    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/one")
    );
    assert!(!session.state().loading);
    assert_eq!(session.tab_summaries()[1].tab_id, second_tab_id);
}

#[test]
fn close_active_tab_selects_neighbor() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.navigate("https://example.com/one");
    let first_tab_id = session.active_tab_id().clone();
    let second_tab_id = session.create_tab(Some("https://example.com/two".to_string()), true);

    session
        .close_tab(&second_tab_id)
        .expect("existing tab should close");

    assert_eq!(session.active_tab_id(), &first_tab_id);
    assert_eq!(session.tab_summaries().len(), 1);
    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/one")
    );
}

#[test]
fn runtime_exposes_tab_lifecycle_by_thread() {
    let mut runtime = BrowserRuntime::new_disabled();

    runtime
        .apply_state_only_action(
            "thread-a",
            &BrowserAction::Navigate {
                url: "https://example.com/one".to_string(),
            },
        )
        .expect("navigation should create a session");
    let first_tab_id = runtime.active_tab_id("thread-a");
    let second_tab_id = runtime.create_tab(
        "thread-a",
        Some("https://example.com/two".to_string()),
        true,
    );

    assert_eq!(runtime.active_tab_id("thread-a"), second_tab_id);
    assert_eq!(runtime.browser_tabs("thread-a").len(), 2);

    runtime
        .select_tab("thread-a", &first_tab_id)
        .expect("existing tab should be selectable");
    assert_eq!(runtime.active_tab_id("thread-a"), first_tab_id);
    assert_eq!(
        runtime
            .session("thread-a")
            .and_then(|session| session.state().url.as_deref()),
        Some("https://example.com/one")
    );

    runtime
        .close_tab("thread-a", &second_tab_id)
        .expect("existing tab should close");
    assert_eq!(runtime.browser_tabs("thread-a").len(), 1);
}

#[test]
fn runtime_rejects_missing_tab_selection() {
    let mut runtime = BrowserRuntime::new_disabled();

    let error = runtime
        .select_tab("thread-a", &BrowserTabId::new("missing-tab"))
        .expect_err("missing tab should be rejected");

    assert_eq!(error, BrowserError::MissingTab("missing-tab".to_string()));
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
fn backend_loading_state_updates_history_flags_and_invalidates_snapshot() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(snapshot(7, "https://example.com", 4));

    session.apply_backend_loading_state(
        true,
        true,
        false,
        Some("https://example.com/next".to_string()),
    );

    assert!(session.state().loading);
    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/next")
    );
    assert!(session.state().can_go_back);
    assert!(!session.state().can_go_forward);
    assert_eq!(session.state().snapshot_epoch, 8);
    assert!(session.latest_snapshot().elements.is_empty());

    session.apply_backend_loading_state(false, true, true, None);

    assert!(!session.state().loading);
    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/next")
    );
    assert!(session.state().can_go_back);
    assert!(session.state().can_go_forward);
    assert_eq!(session.state().snapshot_epoch, 8);
}

#[test]
fn backend_url_and_title_events_update_session_state() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    session.set_url("https://example.com/changed");
    session.set_title("Changed");

    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/changed")
    );
    assert_eq!(session.state().title.as_deref(), Some("Changed"));
}

#[test]
fn backend_history_navigation_does_not_require_state_only_history_stack() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(snapshot(7, "https://example.com/b", 4));
    session.set_history_state(true, false);

    session.start_backend_history_navigation();

    assert!(session.state().loading);
    assert_eq!(session.state().load_error, None);
    assert_eq!(session.state().snapshot_epoch, 8);
    assert!(session.latest_snapshot().elements.is_empty());
}

#[test]
fn console_entries_are_sequenced_filtered_and_bounded() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    session.push_console_entry(
        BrowserConsoleLevel::Info,
        "first",
        Some("https://example.com/app.js".to_string()),
        Some(10),
        1000,
    );
    session.push_console_entry(BrowserConsoleLevel::Error, "second", None, None, 1001);

    let errors = session.recent_console_entries(Some(BrowserConsoleLevel::Error), None, 10);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].sequence, 2);
    assert_eq!(errors[0].message, "second");

    let since_first = session.recent_console_entries(None, Some(1), 10);
    assert_eq!(since_first.len(), 1);
    assert_eq!(since_first[0].sequence, 2);

    for index in 0..510 {
        session.push_console_entry(
            BrowserConsoleLevel::Info,
            format!("entry {index}"),
            None,
            None,
            2000 + index,
        );
    }

    assert_eq!(session.console_entries().len(), 500);
    assert_eq!(session.console_entries()[0].sequence, 13);

    let limited = session.recent_console_entries(None, None, 3);
    assert_eq!(limited.len(), 3);
    assert_eq!(limited[0].sequence, 510);
    assert_eq!(limited[2].sequence, 512);
}

#[test]
fn console_entries_can_be_filtered_by_tab() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    let first_tab_id = session.active_tab_id().clone();
    let second_tab_id = session.create_tab(Some("https://example.com/two".to_string()), true);

    session.push_console_entry_for_tab(
        first_tab_id.clone(),
        BrowserConsoleLevel::Info,
        "first tab",
        None,
        None,
        1000,
    );
    session.push_console_entry_for_tab(
        second_tab_id.clone(),
        BrowserConsoleLevel::Info,
        "second tab",
        None,
        None,
        1001,
    );

    let first_entries = session.recent_console_entries_for_tab(&first_tab_id, None, None, 10);
    let second_entries = session.recent_console_entries_for_tab(&second_tab_id, None, None, 10);

    assert_eq!(first_entries.len(), 1);
    assert_eq!(first_entries[0].message, "first tab");
    assert_eq!(first_entries[0].tab_id, first_tab_id);
    assert_eq!(second_entries.len(), 1);
    assert_eq!(second_entries[0].message, "second tab");
    assert_eq!(
        session.latest_console_sequence_for_tab(&second_tab_id),
        Some(second_entries[0].sequence)
    );
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
fn reload_requires_loaded_page_and_invalidates_snapshot() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    assert_eq!(session.reload(), Err(BrowserError::NoPageLoaded));

    session.replace_snapshot(snapshot(7, "https://example.com", 4));
    session.set_load_error("network failed");

    session.reload().expect("loaded page should reload");

    assert_eq!(session.state().url.as_deref(), Some("https://example.com"));
    assert!(session.state().loading);
    assert_eq!(session.state().load_error, None);
    assert_eq!(session.state().snapshot_epoch, 8);
    assert!(session.latest_snapshot().elements.is_empty());
}

#[test]
fn stop_clears_loading_without_changing_page() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    session.navigate("https://example.com");
    session.stop();

    assert_eq!(session.state().url.as_deref(), Some("https://example.com"));
    assert!(!session.state().loading);
}

#[test]
fn state_only_history_supports_back_and_forward() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    session.navigate("https://example.com/a");
    session.set_loading(false);
    session.navigate("https://example.com/b");

    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/b")
    );
    assert!(session.state().can_go_back);
    assert!(!session.state().can_go_forward);

    session.go_back().expect("back history should exist");

    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/a")
    );
    assert!(!session.state().can_go_back);
    assert!(session.state().can_go_forward);
    assert!(session.state().loading);

    session.go_forward().expect("forward history should exist");

    assert_eq!(
        session.state().url.as_deref(),
        Some("https://example.com/b")
    );
    assert!(session.state().can_go_back);
    assert!(!session.state().can_go_forward);
}

#[test]
fn state_only_history_reports_missing_entries() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));

    assert_eq!(
        session.go_back(),
        Err(BrowserError::HistoryUnavailable(
            BrowserHistoryDirection::Back
        ))
    );
    assert_eq!(
        session.go_forward(),
        Err(BrowserError::HistoryUnavailable(
            BrowserHistoryDirection::Forward
        ))
    );
}

#[test]
fn runtime_applies_state_only_navigation_controls() {
    let mut runtime = BrowserRuntime::new_disabled();

    runtime
        .apply_state_only_action(
            "thread-a",
            &BrowserAction::Navigate {
                url: "https://example.com/a".to_string(),
            },
        )
        .expect("first navigation should update state");
    runtime
        .session_mut("thread-a")
        .expect("session should exist")
        .set_loading(false);
    runtime
        .apply_state_only_action(
            "thread-a",
            &BrowserAction::Navigate {
                url: "https://example.com/b".to_string(),
            },
        )
        .expect("second navigation should update state");

    runtime
        .apply_state_only_action("thread-a", &BrowserAction::Back)
        .expect("back navigation should update state");
    assert_eq!(
        runtime
            .session("thread-a")
            .and_then(|session| session.state().url.as_deref()),
        Some("https://example.com/a")
    );

    runtime
        .apply_state_only_action("thread-a", &BrowserAction::Forward)
        .expect("forward navigation should update state");
    runtime
        .apply_state_only_action("thread-a", &BrowserAction::Reload)
        .expect("reload should update state");
    assert!(
        runtime
            .session("thread-a")
            .expect("session should exist")
            .state()
            .loading
    );

    runtime
        .apply_state_only_action("thread-a", &BrowserAction::Stop)
        .expect("stop should update state");
    assert!(
        !runtime
            .session("thread-a")
            .expect("session should exist")
            .state()
            .loading
    );
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

#[test]
fn selecting_tab_restores_that_tabs_cached_frame() {
    let mut session = hunk_browser::BrowserSession::new(BrowserSessionId::new("thread-a"));
    let first_tab_id = session.active_tab_id().clone();
    let first_frame =
        BrowserFrame::from_bgra(1, 1, 11, vec![1, 2, 3, 255]).expect("valid first frame");
    session.set_latest_frame_for_tab(&first_tab_id, first_frame);

    let second_tab_id = session.create_tab(Some("https://example.com/two".to_string()), true);
    let second_frame =
        BrowserFrame::from_bgra(1, 1, 12, vec![4, 5, 6, 255]).expect("valid second frame");
    session.set_latest_frame_for_tab(&second_tab_id, second_frame);

    session
        .select_tab(&first_tab_id)
        .expect("first tab should still be selectable");

    assert_eq!(
        session
            .latest_frame()
            .expect("first tab frame should be restored")
            .metadata()
            .frame_epoch,
        11
    );
    assert_eq!(
        session
            .latest_frame()
            .expect("first tab frame should be restored")
            .bgra(),
        &[1, 2, 3, 255]
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
