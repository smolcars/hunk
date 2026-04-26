use hunk_browser::{
    BrowserElement, BrowserElementRect, BrowserPhysicalPoint, BrowserSession, BrowserSessionId,
    BrowserSnapshot, BrowserViewport, BrowserViewportSize,
};

#[test]
fn viewport_converts_logical_points_to_physical_pixels() {
    let viewport = viewport(2.0);

    let point =
        viewport.logical_to_physical_point(hunk_browser::BrowserPoint { x: 10.25, y: 20.5 });

    assert_eq!(point, BrowserPhysicalPoint { x: 21, y: 41 });
}

#[test]
fn viewport_converts_physical_pixels_to_logical_points() {
    let viewport = viewport(2.0);

    let point = viewport.physical_to_logical_point(BrowserPhysicalPoint { x: 21, y: 41 });

    assert_eq!(point.x, 10.5);
    assert_eq!(point.y, 20.5);
}

#[test]
fn viewport_converts_logical_points_to_view_coordinates() {
    let viewport = viewport(2.0);

    let point = viewport.logical_to_view_point(hunk_browser::BrowserPoint { x: 10.25, y: 20.5 });

    assert_eq!(point, BrowserPhysicalPoint { x: 10, y: 21 });
}

#[test]
fn element_click_target_uses_rect_center_view_coordinates() {
    let mut session = BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(BrowserSnapshot {
        epoch: 3,
        url: Some("https://example.com".to_string()),
        title: Some("Example".to_string()),
        viewport: viewport(2.0),
        elements: vec![BrowserElement {
            index: 7,
            role: "button".to_string(),
            label: "Continue".to_string(),
            text: "Continue".to_string(),
            rect: BrowserElementRect {
                x: 10.0,
                y: 20.0,
                width: 81.0,
                height: 31.0,
            },
            selector: Some("#continue".to_string()),
        }],
    });

    let target = session
        .element_click_target(3, 7)
        .expect("current element should have a click target");

    assert_eq!(target, BrowserPhysicalPoint { x: 51, y: 36 });
}

#[test]
fn scroll_target_uses_requested_element_center() {
    let mut session = BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.replace_snapshot(BrowserSnapshot {
        epoch: 3,
        url: Some("https://example.com".to_string()),
        title: Some("Example".to_string()),
        viewport: viewport(2.0),
        elements: vec![BrowserElement {
            index: 7,
            role: "listbox".to_string(),
            label: "Results".to_string(),
            text: String::new(),
            rect: BrowserElementRect {
                x: 30.0,
                y: 40.0,
                width: 100.0,
                height: 50.0,
            },
            selector: Some("#results".to_string()),
        }],
    });

    let target = session
        .scroll_target(Some(7))
        .expect("current element should have a scroll target");

    assert_eq!(target, BrowserPhysicalPoint { x: 80, y: 65 });
}

#[test]
fn scroll_target_defaults_to_viewport_center() {
    let mut session = BrowserSession::new(BrowserSessionId::new("thread-a"));
    session.set_viewport(BrowserViewportSize::new(640, 480, 2.0).unwrap());

    let target = session
        .scroll_target(None)
        .expect("viewport should have a scroll target");

    assert_eq!(target, BrowserPhysicalPoint { x: 320, y: 240 });
}

#[test]
fn session_viewport_size_updates_snapshot_viewport() {
    let mut session = BrowserSession::new(BrowserSessionId::new("thread-a"));

    session.set_viewport(BrowserViewportSize::new(1440, 900, 2.0).unwrap());

    let viewport = &session.latest_snapshot().viewport;
    assert_eq!(viewport.width, 1440);
    assert_eq!(viewport.height, 900);
    assert_eq!(viewport.device_scale_factor, 2.0);
}

#[test]
fn viewport_size_rejects_zero_dimensions() {
    let error = BrowserViewportSize::new(0, 900, 1.0).unwrap_err();

    assert_eq!(
        error,
        hunk_browser::BrowserError::InvalidViewportSize {
            width: 0,
            height: 900
        }
    );
}

#[test]
fn viewport_size_sanitizes_invalid_scale() {
    let viewport = BrowserViewportSize::new(800, 600, f32::NAN).unwrap();

    assert_eq!(viewport.device_scale_factor, 1.0);
}

fn viewport(device_scale_factor: f32) -> BrowserViewport {
    BrowserViewport {
        width: 1024,
        height: 768,
        device_scale_factor,
        scroll_x: 0.0,
        scroll_y: 0.0,
    }
}
