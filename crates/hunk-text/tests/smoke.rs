use hunk_text::{
    Anchor, AnchorBias, BufferId, Selection, TextBuffer, TextPosition, TextRange, Transaction,
};

#[test]
fn text_buffer_snapshot_tracks_version_and_shape() {
    let mut buffer = TextBuffer::new(BufferId::new(7), "alpha\nbeta\n");
    let initial = buffer.snapshot();

    assert_eq!(initial.buffer_id, BufferId::new(7));
    assert_eq!(initial.version, 0);
    assert_eq!(initial.line_count(), 3);
    assert_eq!(initial.byte_len(), "alpha\nbeta\n".len());

    buffer.set_text("gamma\n");

    let updated = buffer.snapshot();
    assert_eq!(updated.version, 1);
    assert_eq!(updated.line_count(), 2);
    assert_eq!(updated.text(), "gamma\n");
}

#[test]
fn selection_range_normalizes_backward_selection() {
    let selection = Selection::new(TextPosition::new(8, 3), TextPosition::new(2, 5));
    let range = selection.range();

    assert_eq!(
        range,
        TextRange::new(TextPosition::new(2, 5), TextPosition::new(8, 3))
    );
    assert!(!selection.is_caret());
}

#[test]
fn transaction_application_and_undo_round_trip() {
    let mut buffer = TextBuffer::new(BufferId::new(1), "hello world");
    let transaction = Transaction::new().replace(6..11, "there");

    buffer
        .apply_transaction(transaction)
        .expect("apply transaction");
    assert_eq!(buffer.text(), "hello there");

    assert!(buffer.undo().expect("undo"));
    assert_eq!(buffer.text(), "hello world");
    assert!(buffer.redo().expect("redo"));
    assert_eq!(buffer.text(), "hello there");
}

#[test]
fn anchors_track_insertions_with_bias() {
    let transaction = Transaction::new().replace(3..3, "XYZ");
    let left = Anchor::new(3, AnchorBias::Left).apply_transaction(&transaction);
    let right = Anchor::new(3, AnchorBias::Right).apply_transaction(&transaction);

    assert_eq!(left.byte, 3);
    assert_eq!(right.byte, 6);
}

#[test]
fn anchor_mapping_is_stable_for_unsorted_non_overlapping_edits() {
    let transaction = Transaction::new()
        .replace(10..10, "tail")
        .replace(2..2, "head");
    let anchor = Anchor::new(12, AnchorBias::Right).apply_transaction(&transaction);

    assert_eq!(anchor.byte, 20);
}

#[test]
fn search_finds_next_and_all_matches() {
    let buffer = TextBuffer::new(BufferId::new(9), "one two one");
    let snapshot = buffer.snapshot();

    let next = snapshot
        .find_next("one", 1)
        .expect("search")
        .expect("match");
    assert_eq!(next.byte_range, 8..11);

    let all = snapshot.find_all("one");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].byte_range, 0..3);
    assert_eq!(all[1].byte_range, 8..11);
}

#[test]
fn large_snapshot_clone_and_edit_smoke() {
    let source = "abcdef0123456789\n".repeat(20_000);
    let mut buffer = TextBuffer::new(BufferId::new(11), &source);
    let snapshot = buffer.snapshot();
    let clone = snapshot.clone();

    assert_eq!(snapshot.byte_len(), clone.byte_len());
    assert_eq!(snapshot.line_count(), clone.line_count());

    buffer
        .apply_transaction(Transaction::new().replace(10..20, "TEN-TO-TWENTY"))
        .expect("edit large buffer");
    assert_ne!(buffer.text(), source);
}

#[test]
fn newline_terminated_buffers_keep_eof_as_terminal_empty_line() {
    let buffer = TextBuffer::new(BufferId::new(12), "alpha\nbeta\n");
    let snapshot = buffer.snapshot();

    let eof = snapshot
        .byte_to_position(snapshot.byte_len())
        .expect("eof position");
    assert_eq!(eof, TextPosition::new(2, 0));
    assert_eq!(
        snapshot
            .position_to_byte(TextPosition::new(2, 0))
            .expect("terminal line byte"),
        snapshot.byte_len()
    );
}
