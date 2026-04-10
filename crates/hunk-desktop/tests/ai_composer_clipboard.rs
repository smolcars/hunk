#[path = "../src/app/ai_composer_clipboard.rs"]
mod ai_composer_clipboard;

use std::fs;
use std::path::PathBuf;

use ai_composer_clipboard::ai_composer_clipboard_attachments;
use gpui::{ClipboardEntry, ClipboardItem, ExternalPaths, Image, ImageFormat};

#[test]
fn clipboard_text_does_not_produce_composer_attachments() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let pasted_image_dir = ai_composer_clipboard::ai_composer_pasted_image_dir();
    let clipboard = ClipboardItem::new_string("plain text".to_string());

    let attachments =
        ai_composer_clipboard_attachments(&clipboard, temp_dir.path()).expect("attachments");

    assert_eq!(attachments, None);
    assert!(pasted_image_dir.ends_with(std::process::id().to_string()));
}

#[test]
fn absolute_image_paths_in_clipboard_text_are_attachment_candidates() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let image_path = PathBuf::from("/tmp/test-screenshot.png");
    let clipboard = ClipboardItem::new_string(image_path.display().to_string());

    let attachments =
        ai_composer_clipboard_attachments(&clipboard, temp_dir.path()).expect("attachments");
    let attachments = attachments.expect("text path attachments");

    assert_eq!(attachments.item_count, 1);
    assert_eq!(attachments.paths, vec![image_path]);
}

#[test]
fn file_urls_in_clipboard_text_are_attachment_candidates() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let image_path = PathBuf::from("/tmp/test screenshot.png");
    let clipboard = ClipboardItem::new_string("file:///tmp/test%20screenshot.png".to_string());

    let attachments =
        ai_composer_clipboard_attachments(&clipboard, temp_dir.path()).expect("attachments");
    let attachments = attachments.expect("file url attachments");

    assert_eq!(attachments.item_count, 1);
    assert_eq!(attachments.paths, vec![image_path]);
}

#[test]
fn clipboard_external_paths_are_returned_as_attachment_candidates() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let image_path = PathBuf::from("/tmp/test-screenshot.png");
    let clipboard = ClipboardItem {
        entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
            vec![image_path.clone()].into(),
        ))],
    };

    let attachments =
        ai_composer_clipboard_attachments(&clipboard, temp_dir.path()).expect("attachments");
    let attachments = attachments.expect("external path attachments");

    assert_eq!(attachments.item_count, 1);
    assert_eq!(attachments.paths, vec![image_path]);
}

#[test]
fn clipboard_images_are_persisted_to_temp_files_for_attachment() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let image = Image::from_bytes(ImageFormat::Png, vec![0x89, b'P', b'N', b'G']);
    let clipboard = ClipboardItem {
        entries: vec![ClipboardEntry::Image(image.clone())],
    };

    let attachments =
        ai_composer_clipboard_attachments(&clipboard, temp_dir.path()).expect("attachments");
    let attachments = attachments.expect("image attachments");

    assert_eq!(attachments.item_count, 1);
    assert_eq!(attachments.paths.len(), 1);

    let saved_path = attachments.paths[0].as_path();
    assert_eq!(
        saved_path.extension().and_then(|value| value.to_str()),
        Some("png")
    );
    assert_eq!(
        fs::read(saved_path).expect("saved image bytes"),
        image.bytes()
    );
}

#[test]
fn unsupported_clipboard_image_formats_are_counted_but_not_saved() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let clipboard = ClipboardItem {
        entries: vec![ClipboardEntry::Image(Image::from_bytes(
            ImageFormat::Svg,
            b"<svg />".to_vec(),
        ))],
    };

    let attachments =
        ai_composer_clipboard_attachments(&clipboard, temp_dir.path()).expect("attachments");
    let attachments = attachments.expect("svg clipboard payload");

    assert_eq!(attachments.item_count, 1);
    assert!(attachments.paths.is_empty());
}
