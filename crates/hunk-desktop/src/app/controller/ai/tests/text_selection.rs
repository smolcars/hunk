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
    fn ai_text_selection_spans_multiple_rows_in_one_scope() {
        let surfaces = vec![
            AiTextSelectionSurfaceSpec::new("surface-a", "hello").with_row_id("row-a"),
            AiTextSelectionSurfaceSpec::new("surface-b", "world")
                .with_row_id("row-b")
                .with_separator_before("\n\n"),
        ];
        let mut selection =
            AiTextSelection::new("thread-1".to_string(), surfaces.as_slice(), "surface-a", 2);
        selection.set_head_for_surface("surface-b", 3);

        assert_eq!(selection.selected_text().as_deref(), Some("llo\n\nwor"));
        assert!(selection.intersects_row_ids(&std::collections::BTreeSet::from([
            "row-b".to_string(),
        ])));
    }

    #[test]
    fn ai_text_selection_caret_only_intersects_its_active_row() {
        let surfaces = vec![
            AiTextSelectionSurfaceSpec::new("surface-a", "hello").with_row_id("row-a"),
            AiTextSelectionSurfaceSpec::new("surface-b", "world")
                .with_row_id("row-b")
                .with_separator_before("\n\n"),
        ];
        let selection =
            AiTextSelection::new("thread-1".to_string(), surfaces.as_slice(), "surface-a", 5);

        assert!(selection.intersects_row_ids(&std::collections::BTreeSet::from([
            "row-a".to_string(),
        ])));
        assert!(!selection.intersects_row_ids(&std::collections::BTreeSet::from([
            "row-b".to_string(),
        ])));
    }

    #[test]
    fn ai_text_selection_range_only_intersects_rows_it_covers() {
        let surfaces = vec![
            AiTextSelectionSurfaceSpec::new("surface-a", "hello").with_row_id("row-a"),
            AiTextSelectionSurfaceSpec::new("surface-b", "world")
                .with_row_id("row-b")
                .with_separator_before("\n\n"),
            AiTextSelectionSurfaceSpec::new("surface-c", "later")
                .with_row_id("row-c")
                .with_separator_before("\n\n"),
        ];
        let mut selection =
            AiTextSelection::new("thread-1".to_string(), surfaces.as_slice(), "surface-a", 2);
        selection.set_head_for_surface("surface-b", 3);

        assert!(selection.intersects_row_ids(&std::collections::BTreeSet::from([
            "row-a".to_string(),
        ])));
        assert!(selection.intersects_row_ids(&std::collections::BTreeSet::from([
            "row-b".to_string(),
        ])));
        assert!(!selection.intersects_row_ids(&std::collections::BTreeSet::from([
            "row-c".to_string(),
        ])));
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

    #[test]
    fn ai_workspace_selection_surfaces_join_title_and_preview_with_newline() {
        let block = ai_workspace_session::AiWorkspaceBlock {
            id: "row-1".to_string(),
            source_row_id: "row-1".to_string(),
            role: ai_workspace_session::AiWorkspaceBlockRole::Assistant,
            kind: ai_workspace_session::AiWorkspaceBlockKind::Message,
            nested: false,
            mono_preview: false,
            markdown_preview: true,
            open_review_tab: false,
            expandable: false,
            expanded: true,
            title: "Assistant".to_string(),
            preview: "Hello from the workspace surface.".to_string(),
            action_area: ai_workspace_session::AiWorkspaceBlockActionArea::Header,
            copy_text: None,
            copy_tooltip: None,
            copy_success_message: None,
            run_in_terminal_command: None,
            run_in_terminal_cwd: None,
            status_label: None,
            status_color_role: None,
            last_sequence: 1,
        };

        let surfaces = ai_workspace_selection_surfaces(&block);
        assert_eq!(surfaces.len(), 2);
        assert_eq!(surfaces[0].surface_id, "ai-workspace:row-1:title");
        assert_eq!(surfaces[0].text, "Assistant");
        assert_eq!(surfaces[1].surface_id, "ai-workspace:row-1:preview");
        assert_eq!(surfaces[1].separator_before, "\n");
        assert_eq!(surfaces[1].text, "Hello from the workspace surface.");
    }

    #[test]
    fn ai_workspace_context_menu_target_preserves_existing_selection() {
        let surfaces = ai_selection_surfaces([("surface", "hello world", "")]);
        let mut selection =
            AiTextSelection::new("row".to_string(), surfaces.as_slice(), "surface", 0);
        selection.set_head_for_surface("surface", 5);
        selection.dragging = false;

        let (should_place_caret, target) = ai_workspace_selectable_text_context_menu_target(
            Some(&selection),
            "row",
            "surface",
            2,
            std::sync::Arc::<[AiTextSelectionSurfaceSpec]>::from(surfaces),
            Some("https://example.com".to_string()),
        );

        assert!(!should_place_caret);
        assert_eq!(target.row_id, "row");
        assert!(target.can_copy);
        assert!(target.can_select_all);
        assert_eq!(target.link_target.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn ai_workspace_context_menu_target_places_caret_for_unselected_hit() {
        let surfaces = ai_selection_surfaces([("surface", "hello world", "")]);
        let mut selection =
            AiTextSelection::new("row".to_string(), surfaces.as_slice(), "surface", 0);
        selection.set_head_for_surface("surface", 5);
        selection.dragging = false;

        let (should_place_caret, target) = ai_workspace_selectable_text_context_menu_target(
            Some(&selection),
            "row",
            "surface",
            8,
            std::sync::Arc::<[AiTextSelectionSurfaceSpec]>::from(surfaces),
            None,
        );

        assert!(should_place_caret);
        assert!(!target.can_copy);
        assert!(target.can_select_all);
        assert!(target.link_target.is_none());
    }
