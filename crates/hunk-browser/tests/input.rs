use hunk_browser::{
    BrowserElement, BrowserElementRect, BrowserPhysicalPoint, BrowserSession, BrowserSessionId,
    BrowserSnapshot, BrowserViewport,
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
fn element_click_target_uses_rect_center_and_device_scale() {
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

    assert_eq!(target, BrowserPhysicalPoint { x: 101, y: 71 });
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
