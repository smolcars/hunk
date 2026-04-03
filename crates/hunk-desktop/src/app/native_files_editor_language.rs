use super::*;
use hunk_language::{
    CompletionRequest, CompletionTriggerKind, DefinitionRequest, Diagnostic, DiagnosticSeverity,
    HoverRequest, SemanticToken,
};

impl FilesEditor {
    #[allow(dead_code)]
    pub(crate) fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.editor
            .apply(EditorCommand::SetDiagnostics(diagnostics));
        self.sync_overlays();
    }

    #[allow(dead_code)]
    pub(crate) fn set_semantic_tokens(&mut self, tokens: Vec<SemanticToken>) {
        self.editor.apply(EditorCommand::SetSemanticTokens(tokens));
        self.visible_highlight_cache = None;
        self.semantic_highlight_revision = self.semantic_highlight_revision.saturating_add(1);
    }

    #[allow(dead_code)]
    pub(crate) fn request_hover_at_cursor(&mut self) -> Option<HoverRequest> {
        let document = self.document_context()?;
        let position = self.editor.selection().head;
        let source = self.editor.buffer().text();
        let target = self.syntax.hover_target_at(&source, position)?;
        let request = HoverRequest {
            document,
            position,
            target,
        };
        self.editor
            .apply(EditorCommand::RequestHover(request.clone()));
        Some(request)
    }

    #[allow(dead_code)]
    pub(crate) fn take_pending_hover_request(&mut self) -> Option<HoverRequest> {
        let request = self.editor.pending_hover_request().cloned();
        if request.is_some() {
            self.editor.apply(EditorCommand::ClearHoverRequest);
        }
        request
    }

    #[allow(dead_code)]
    pub(crate) fn request_definition_at_cursor(&mut self) -> Option<DefinitionRequest> {
        let document = self.document_context()?;
        let position = self.editor.selection().head;
        let source = self.editor.buffer().text();
        let target = self.syntax.definition_target_at(&source, position)?;
        let request = DefinitionRequest { document, target };
        self.editor
            .apply(EditorCommand::RequestDefinition(request.clone()));
        Some(request)
    }

    #[allow(dead_code)]
    pub(crate) fn take_pending_definition_request(&mut self) -> Option<DefinitionRequest> {
        let request = self.editor.pending_definition_request().cloned();
        if request.is_some() {
            self.editor.apply(EditorCommand::ClearDefinitionRequest);
        }
        request
    }

    #[allow(dead_code)]
    pub(crate) fn trigger_completion(
        &mut self,
        trigger: CompletionTriggerKind,
    ) -> Option<CompletionRequest> {
        let document = self.document_context()?;
        let position = self.editor.selection().head;
        let source = self.editor.buffer().text();
        let context = self
            .syntax
            .completion_context_at(&source, position, trigger)?;
        let request = CompletionRequest {
            document,
            position,
            context,
        };
        self.editor
            .apply(EditorCommand::RequestCompletion(request.clone()));
        Some(request)
    }

    #[allow(dead_code)]
    pub(crate) fn take_pending_completion_request(&mut self) -> Option<CompletionRequest> {
        let request = self.editor.pending_completion_request().cloned();
        if request.is_some() {
            self.editor.apply(EditorCommand::ClearCompletionRequest);
        }
        request
    }

    #[allow(dead_code)]
    fn document_context(&self) -> Option<hunk_language::DocumentContext> {
        let path = self.active_path_buf()?;
        let snapshot = self.editor.buffer().snapshot();
        Some(hunk_language::DocumentContext {
            path,
            language_id: self.editor.status_snapshot().language_id,
            version: snapshot.version,
        })
    }
}

pub(super) fn overlay_kind_for_diagnostic_severity(severity: DiagnosticSeverity) -> OverlayKind {
    match severity {
        DiagnosticSeverity::Error => OverlayKind::DiagnosticError,
        DiagnosticSeverity::Warning => OverlayKind::DiagnosticWarning,
        DiagnosticSeverity::Info | DiagnosticSeverity::Hint => OverlayKind::DiagnosticInfo,
    }
}
