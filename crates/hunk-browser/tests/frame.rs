use std::time::{Duration, Instant};

use hunk_browser::{
    BROWSER_FRAME_TARGET_INTERVAL, BrowserFrame, BrowserFrameError, BrowserFrameRateLimiter,
};

#[test]
fn bgra_frame_keeps_metadata_and_pixels() {
    let frame = BrowserFrame::from_bgra(2, 1, 9, vec![0, 0, 255, 255, 0, 255, 0, 255])
        .expect("valid bgra frame should be accepted");

    assert_eq!(frame.metadata().width, 2);
    assert_eq!(frame.metadata().height, 1);
    assert_eq!(frame.metadata().frame_epoch, 9);
    assert_eq!(frame.bgra(), &[0, 0, 255, 255, 0, 255, 0, 255]);
    assert!(!frame.is_blank());
}

#[test]
fn blank_bgra_frame_is_detected() {
    let frame = BrowserFrame::from_bgra(1, 1, 1, vec![0, 0, 0, 0])
        .expect("valid bgra frame should be accepted");

    assert!(frame.is_blank());
}

#[test]
fn bgra_frame_rejects_wrong_buffer_length() {
    let error = BrowserFrame::from_bgra(2, 2, 1, vec![0; 12])
        .expect_err("short bgra frame should be rejected");

    assert_eq!(
        error,
        BrowserFrameError::InvalidBufferLength {
            expected: 16,
            actual: 12
        }
    );
}

#[test]
fn bgra_frame_rejects_zero_dimensions() {
    let error =
        BrowserFrame::from_bgra(0, 1, 1, Vec::new()).expect_err("zero width should be rejected");

    assert_eq!(
        error,
        BrowserFrameError::InvalidDimensions {
            width: 0,
            height: 1
        }
    );
}

#[test]
fn frame_rate_limiter_allows_first_frame() {
    let mut limiter = BrowserFrameRateLimiter::v1_60fps();

    assert_eq!(limiter.min_interval(), BROWSER_FRAME_TARGET_INTERVAL);
    assert!(limiter.should_notify(Instant::now()));
}

#[test]
fn frame_rate_limiter_suppresses_frames_inside_interval() {
    let mut limiter = BrowserFrameRateLimiter::with_min_interval(Duration::from_millis(16));
    let start = Instant::now();

    assert!(limiter.should_notify(start));
    assert!(!limiter.should_notify(start + Duration::from_millis(15)));
}

#[test]
fn frame_rate_limiter_allows_frames_after_interval() {
    let mut limiter = BrowserFrameRateLimiter::with_min_interval(Duration::from_millis(16));
    let start = Instant::now();

    assert!(limiter.should_notify(start));
    assert!(limiter.should_notify(start + Duration::from_millis(16)));
}
