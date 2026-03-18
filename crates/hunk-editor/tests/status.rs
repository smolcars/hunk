use std::path::PathBuf;

use hunk_editor::{EditorCommand, EditorState, OverlayDescriptor, OverlayKind, Viewport};
use hunk_language::{
    CompletionContext, CompletionRequest, CompletionTriggerKind, DefinitionRequest, Diagnostic,
    DiagnosticSeverity, DocumentContext, HoverRequest, LanguageId, ParseStatus, SemanticToken,
    SemanticTokenKind, SymbolKind, SymbolOccurrence,
};
use hunk_text::{BufferId, TextBuffer, TextPosition, TextRange};

#[test]
fn editor_state_tracks_dirty_language_parse_status_and_overlays() {
    let buffer = TextBuffer::new(BufferId::new(3), "fn main() {}\n");
    let mut editor = EditorState::new(buffer);

    let output = editor.apply(EditorCommand::SetViewport(Viewport {
        first_visible_row: 4,
        visible_row_count: 20,
        horizontal_offset: 2,
    }));
    assert!(output.viewport_changed);

    editor.apply(EditorCommand::SetLanguage(Some(LanguageId::new(9))));
    editor.apply(EditorCommand::SetParseStatus(ParseStatus::Parsing));
    editor.apply(EditorCommand::SetOverlays(vec![OverlayDescriptor {
        line: 0,
        kind: OverlayKind::DiagnosticWarning,
        message: Some("warn".to_string()),
    }]));
    editor.apply(EditorCommand::ReplaceAll(
        "fn answer() -> i32 { 42 }\n".to_string(),
    ));

    let display = editor.display_snapshot();
    assert_eq!(display.viewport.first_visible_row, 0);
    assert!(display.dirty);
    assert_eq!(display.language_id, Some(LanguageId::new(9)));
    assert_eq!(display.parse_status, ParseStatus::Parsing);
    assert_eq!(display.visible_rows[0].overlays.len(), 1);

    editor.apply(EditorCommand::MarkSaved);
    assert!(!editor.is_dirty());
}

#[test]
fn editor_state_tracks_language_intelligence_state_and_pending_requests() {
    let buffer = TextBuffer::new(BufferId::new(7), "let answer = compute();\n");
    let mut editor = EditorState::new(buffer);
    let document = DocumentContext {
        path: PathBuf::from("main.rs"),
        language_id: Some(LanguageId::new(11)),
        version: 3,
    };
    let target = SymbolOccurrence {
        range: TextRange::new(TextPosition::new(0, 4), TextPosition::new(0, 10)),
        text: "answer".to_string(),
        kind: SymbolKind::Symbol,
        node_kind: "identifier".to_string(),
    };

    editor.apply(EditorCommand::SetDiagnostics(vec![Diagnostic {
        range: TextRange::new(TextPosition::new(0, 0), TextPosition::new(0, 3)),
        severity: DiagnosticSeverity::Warning,
        message: "shadowed binding".to_string(),
        source: Some("test".to_string()),
        code: Some("W1".to_string()),
    }]));
    editor.apply(EditorCommand::SetSemanticTokens(vec![SemanticToken {
        range: TextRange::new(TextPosition::new(0, 4), TextPosition::new(0, 10)),
        kind: SemanticTokenKind::Parameter,
        modifiers: Vec::new(),
    }]));

    let hover = editor.apply(EditorCommand::RequestHover(HoverRequest {
        document: document.clone(),
        position: TextPosition::new(0, 5),
        target: target.clone(),
    }));
    let definition = editor.apply(EditorCommand::RequestDefinition(DefinitionRequest {
        document: document.clone(),
        target: target.clone(),
    }));
    let completion = editor.apply(EditorCommand::RequestCompletion(CompletionRequest {
        document,
        position: TextPosition::new(0, 10),
        context: CompletionContext {
            replace_range: TextRange::new(TextPosition::new(0, 4), TextPosition::new(0, 10)),
            prefix: "answer".to_string(),
            trigger: CompletionTriggerKind::Invoked,
        },
    }));

    assert_eq!(editor.diagnostics().len(), 1);
    assert_eq!(editor.semantic_tokens().len(), 1);
    assert!(hover.hover_requested);
    assert!(definition.definition_requested);
    assert!(completion.completion_requested);
    assert!(editor.pending_hover_request().is_some());
    assert!(editor.pending_definition_request().is_some());
    assert!(editor.pending_completion_request().is_some());

    editor.apply(EditorCommand::ReplaceAll("replacement\n".to_string()));
    assert!(editor.diagnostics().is_empty());
    assert!(editor.semantic_tokens().is_empty());
    assert!(editor.pending_hover_request().is_none());
    assert!(editor.pending_definition_request().is_none());
    assert!(editor.pending_completion_request().is_none());
}
