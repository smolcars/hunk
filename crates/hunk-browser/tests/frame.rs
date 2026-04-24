use hunk_browser::{BrowserFrame, BrowserFrameError};

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
