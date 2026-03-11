    #[test]
    fn ai_text_selection_tracks_forward_ranges() {
        let mut selection = AiTextSelection::new(
            "row".to_string(),
            ai_selection_surfaces([("surface", "hello world", "")]).as_slice(),
            "surface",
            0,
        );
        selection.set_head_for_surface("surface", 5);

        assert_eq!(selection.range(), 0..5);
        assert_eq!(selection.selected_text().as_deref(), Some("hello"));
        assert_eq!(selection.range_for_surface("surface"), Some(0..5));
    }

    #[test]
    fn ai_text_selection_tracks_reverse_ranges() {
        let mut selection = AiTextSelection::new(
            "row".to_string(),
            ai_selection_surfaces([("surface", "hello world", "")]).as_slice(),
            "surface",
            8,
        );
        selection.set_head_for_surface("surface", 2);

        assert_eq!(selection.range(), 2..8);
        assert_eq!(selection.selected_text().as_deref(), Some("llo wo"));
    }

    #[test]
    fn ai_text_selection_select_all_covers_full_surface() {
        let mut selection = AiTextSelection::new(
            "row".to_string(),
            ai_selection_surfaces([("surface", "entire message", "")]).as_slice(),
            "surface",
            4,
        );
        selection.select_all();

        assert_eq!(selection.range(), 0.."entire message".len());
        assert_eq!(
            selection.selected_text().as_deref(),
            Some("entire message")
        );
        assert!(!selection.dragging);
    }

    #[test]
    fn ai_text_selection_spans_multiple_surfaces_in_same_row() {
        let surfaces = ai_selection_surfaces([
            ("surface-a", "hello", ""),
            ("surface-b", "world", "\n\n"),
        ]);
        let mut selection = AiTextSelection::new("row".to_string(), surfaces.as_slice(), "surface-a", 2);
        selection.set_head_for_surface("surface-b", 3);

        assert_eq!(selection.selected_text().as_deref(), Some("llo\n\nwor"));
        assert_eq!(selection.range_for_surface("surface-a"), Some(2..5));
        assert_eq!(selection.range_for_surface("surface-b"), Some(0..3));
    }

    #[test]
    fn ai_text_selection_returns_none_for_non_overlapping_surface() {
        let surfaces = ai_selection_surfaces([
            ("surface-a", "hello", ""),
            ("surface-b", "world", "\n\n"),
        ]);
        let mut selection =
            AiTextSelection::new("row".to_string(), surfaces.as_slice(), "surface-a", 1);
        selection.set_head_for_surface("surface-a", 4);

        assert_eq!(selection.selected_text().as_deref(), Some("ell"));
        assert_eq!(selection.range_for_surface("surface-a"), Some(1..4));
        assert_eq!(selection.range_for_surface("surface-b"), None);
    }

    #[test]
    fn ai_text_selection_clamps_multibyte_indices_to_utf8_boundaries() {
        let mut selection = AiTextSelection::new(
            "row".to_string(),
            ai_selection_surfaces([("surface", "a🙂b", "")]).as_slice(),
            "surface",
            2,
        );
        selection.set_head_for_surface("surface", 5);

        assert_eq!(selection.range(), 1..5);
        assert_eq!(selection.selected_text().as_deref(), Some("🙂"));
        assert_eq!(selection.range_for_surface("surface"), Some(1..5));
    }
