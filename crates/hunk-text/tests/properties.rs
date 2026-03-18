use hunk_text::{Anchor, AnchorBias, BufferId, TextBuffer, TextPosition, Transaction};
use proptest::prelude::*;

proptest! {
    #[test]
    fn position_and_byte_round_trip_for_ascii_offsets(
        segments in prop::collection::vec("[a-z]{1,8}", 1..8)
    ) {
        let text = segments.join("\n");
        let buffer = TextBuffer::new(BufferId::new(17), &text);
        let snapshot = buffer.snapshot();

        for line_index in 0..snapshot.line_count() {
            let line_start = snapshot.line_to_byte(line_index).expect("line start");
            let line_text = snapshot
                .slice(line_start..if line_index + 1 < snapshot.line_count() {
                    snapshot.line_to_byte(line_index + 1).expect("next line start")
                } else {
                    snapshot.byte_len()
                })
                .expect("slice");
            let trimmed = line_text.strip_suffix('\n').unwrap_or(&line_text);

            for column in 0..=trimmed.len() {
                let position = TextPosition::new(line_index, column);
                let byte = snapshot.position_to_byte(position).expect("position to byte");
                let round_trip = snapshot.byte_to_position(byte).expect("byte to position");
                prop_assert_eq!(round_trip, position);
            }
        }
    }

    #[test]
    fn inverse_transaction_restores_original_text(
        prefix in "[a-z]{0,10}",
        replaced in "[a-z]{0,10}",
        suffix in "[a-z]{0,10}",
        inserted in "[A-Z]{0,10}"
    ) {
        let original = format!("{prefix}{replaced}{suffix}");
        let mut buffer = TextBuffer::new(BufferId::new(21), &original);
        let start = prefix.len();
        let end = start + replaced.len();

        buffer
            .apply_transaction(Transaction::new().replace(start..end, inserted.clone()))
            .expect("apply transaction");
        buffer.undo().expect("undo");

        prop_assert_eq!(buffer.text(), original);
    }

    #[test]
    fn right_biased_anchor_moves_past_inserted_text(
        prefix in "[a-z]{0,12}",
        inserted in "[A-Z]{0,12}",
        suffix in "[a-z]{0,12}"
    ) {
        let text = format!("{prefix}{suffix}");
        let buffer = TextBuffer::new(BufferId::new(25), &text);
        let snapshot = buffer.snapshot();
        let insertion_byte = prefix.len();
        let anchor = snapshot.anchor_after(insertion_byte).expect("anchor");
        let mapped = anchor.apply_transaction(&Transaction::new().replace(insertion_byte..insertion_byte, inserted.clone()));

        prop_assert_eq!(mapped, Anchor::new(insertion_byte + inserted.len(), AnchorBias::Right));
    }
}
