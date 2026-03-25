use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::{LanguageDefinition, LanguageRegistry, PreviewSyntaxToken, SyntaxSession};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewHighlightSpan {
    pub byte_range: Range<usize>,
    pub token: PreviewSyntaxToken,
}

pub fn preview_highlight_spans_for_path(
    file_path: Option<&str>,
    source: &str,
) -> Vec<PreviewHighlightSpan> {
    let Some(file_path) = file_path else {
        return Vec::new();
    };
    preview_highlight_spans(
        source,
        registry()
            .language_for_path(Path::new(file_path))
            .map(|language| language.as_ref()),
    )
}

pub fn preview_highlight_spans_for_language_hint(
    language_hint: Option<&str>,
    source: &str,
) -> Vec<PreviewHighlightSpan> {
    let Some(language_hint) = language_hint else {
        return Vec::new();
    };
    preview_highlight_spans(
        source,
        registry()
            .language_for_hint(language_hint)
            .map(|language| language.as_ref()),
    )
}

pub fn warm_preview_highlight_registry() {
    let _ = registry();
}

fn preview_highlight_spans(
    source: &str,
    language: Option<&LanguageDefinition>,
) -> Vec<PreviewHighlightSpan> {
    let Some(language) = language else {
        return Vec::new();
    };

    let mut syntax = SyntaxSession::new();
    if syntax
        .parse_with_language(registry(), Some(language), source)
        .is_err()
    {
        return Vec::new();
    }

    let Ok(captures) = syntax.highlight_visible_range(registry(), source, 0..source.len()) else {
        return Vec::new();
    };

    captures
        .into_iter()
        .map(|capture| PreviewHighlightSpan {
            byte_range: capture.byte_range,
            token: PreviewSyntaxToken::from_capture_name(capture.style_key.as_str()),
        })
        .collect()
}

fn registry() -> &'static LanguageRegistry {
    static REGISTRY: OnceLock<LanguageRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LanguageRegistry::builtin)
}

pub(crate) fn language_hint_path(hint: &str) -> Option<PathBuf> {
    let hint = hint.trim();
    if hint.is_empty() {
        return None;
    }

    if hint.eq_ignore_ascii_case("dockerfile") {
        return Some(PathBuf::from("Dockerfile"));
    }

    let lower = hint.to_ascii_lowercase();
    if lower.contains('.') {
        return Some(PathBuf::from(lower));
    }

    Some(PathBuf::from(format!("sample.{lower}")))
}
