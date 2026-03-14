use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use gpui::{Context, Task, Window};
use gpui_component::{
    Rope, RopeExt,
    input::{CompletionProvider, InputState},
};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    InsertReplaceEdit, Range as LspRange,
};

use super::repo_file_search::RepoFileSearchProvider;

const AI_COMPOSER_FILE_COMPLETION_LIMIT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActivePrefixedToken {
    pub(crate) query: String,
    pub(crate) replace_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiComposerFileCompletionMenuState {
    pub(crate) query: String,
    pub(crate) replace_range: Range<usize>,
    pub(crate) items: Vec<String>,
}

pub(crate) struct AiComposerFileCompletionProvider {
    repo_file_search: Rc<RepoFileSearchProvider>,
}

impl AiComposerFileCompletionProvider {
    pub(crate) fn new(repo_file_search: Rc<RepoFileSearchProvider>) -> Self {
        Self { repo_file_search }
    }

    pub(crate) fn begin_reload(&self, repo_root: Option<PathBuf>) -> u64 {
        self.repo_file_search.begin_reload(repo_root)
    }

    pub(crate) fn apply_reload(
        &self,
        generation: u64,
        repo_root: &Path,
        paths: Vec<String>,
    ) -> bool {
        self.repo_file_search
            .apply_reload(generation, repo_root, paths)
    }

    pub(crate) fn clear(&self) {
        self.repo_file_search.clear();
    }

    pub(crate) fn menu_state(
        &self,
        text: &str,
        cursor_offset: usize,
    ) -> Option<AiComposerFileCompletionMenuState> {
        let active_token = active_file_completion_token(text, cursor_offset)?;
        if active_token.query.is_empty() {
            return None;
        }

        let items = self.repo_file_search.matched_paths(
            active_token.query.as_str(),
            AI_COMPOSER_FILE_COMPLETION_LIMIT,
        );
        if items.is_empty() {
            return None;
        }

        Some(AiComposerFileCompletionMenuState {
            query: active_token.query,
            replace_range: active_token.replace_range,
            items,
        })
    }
}

impl Default for AiComposerFileCompletionProvider {
    fn default() -> Self {
        Self::new(Rc::new(RepoFileSearchProvider::new()))
    }
}

impl CompletionProvider for AiComposerFileCompletionProvider {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        _: &mut Context<InputState>,
    ) -> Task<Result<CompletionResponse>> {
        let text = text.to_string();
        let Some(active_token) = active_file_completion_token(text.as_str(), offset) else {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        };
        if active_token.query.is_empty() {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }

        let paths = self.repo_file_search.matched_paths(
            active_token.query.as_str(),
            AI_COMPOSER_FILE_COMPLETION_LIMIT,
        );
        if paths.is_empty() {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }

        let replace_range = lsp_replace_range(text.as_str(), &active_token.replace_range);
        let items = paths
            .into_iter()
            .map(|path| completion_item_for_path(replace_range, path.as_str()))
            .collect::<Vec<_>>();

        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, _: &str, _: &mut Context<InputState>) -> bool {
        true
    }
}

fn completion_item_for_path(replace_range: LspRange, path: &str) -> CompletionItem {
    CompletionItem {
        label: path.to_string(),
        kind: Some(CompletionItemKind::FILE),
        text_edit: Some(CompletionTextEdit::InsertAndReplace(InsertReplaceEdit {
            new_text: inserted_path_text(path),
            insert: replace_range,
            replace: replace_range,
        })),
        insert_text: None,
        ..Default::default()
    }
}

fn inserted_path_text(path: &str) -> String {
    if path.chars().any(char::is_whitespace) && !path.contains('"') {
        format!("\"{path}\" ")
    } else {
        format!("{path} ")
    }
}

pub(crate) fn ai_composer_inserted_path_text(path: &str) -> String {
    inserted_path_text(path)
}

fn lsp_replace_range(text: &str, replace_range: &Range<usize>) -> LspRange {
    let rope = Rope::from_str(text);
    LspRange::new(
        rope.offset_to_position(replace_range.start),
        rope.offset_to_position(replace_range.end),
    )
}

pub(crate) fn active_file_completion_token(
    text: &str,
    cursor_offset: usize,
) -> Option<ActivePrefixedToken> {
    current_prefixed_token(text, cursor_offset, '@', false)
}

fn current_prefixed_token(
    text: &str,
    cursor_offset: usize,
    prefix: char,
    allow_empty: bool,
) -> Option<ActivePrefixedToken> {
    let safe_cursor = clamp_to_char_boundary(text, cursor_offset);
    let before_cursor = &text[..safe_cursor];
    let after_cursor = &text[safe_cursor..];
    let at_whitespace = safe_cursor < text.len()
        && text[safe_cursor..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace);

    let start_left = before_cursor
        .char_indices()
        .rfind(|(_, ch)| ch.is_whitespace())
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    let end_left = safe_cursor
        + after_cursor
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(index, _)| index)
            .unwrap_or(after_cursor.len());
    let token_left = token_slice(text, start_left, end_left);

    let ws_len_right = after_cursor
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .map(char::len_utf8)
        .sum::<usize>();
    let start_right = safe_cursor + ws_len_right;
    let end_right = start_right
        + text[start_right..]
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(index, _)| index)
            .unwrap_or(text.len().saturating_sub(start_right));
    let token_right = token_slice(text, start_right, end_right);

    let prefix_str = prefix.to_string();
    let left_match = prefixed_token_candidate(token_left.clone(), prefix);
    let right_match = prefixed_token_candidate(token_right, prefix);

    if at_whitespace {
        if right_match.is_some() {
            return right_match;
        }
        if token_left
            .as_ref()
            .is_some_and(|candidate| candidate.text == prefix_str)
        {
            return allow_empty.then(|| ActivePrefixedToken {
                query: String::new(),
                replace_range: start_left..end_left,
            });
        }
        return left_match;
    }

    if after_cursor.starts_with(prefix) {
        let prefix_starts_token = before_cursor
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        return if prefix_starts_token {
            right_match.or(left_match)
        } else {
            left_match
        };
    }

    left_match.or(right_match)
}

#[derive(Debug, Clone)]
struct TokenSlice<'a> {
    text: &'a str,
    range: Range<usize>,
}

fn token_slice(text: &str, start: usize, end: usize) -> Option<TokenSlice<'_>> {
    if start >= end {
        return None;
    }

    Some(TokenSlice {
        text: &text[start..end],
        range: start..end,
    })
}

fn prefixed_token_candidate(
    candidate: Option<TokenSlice<'_>>,
    prefix: char,
) -> Option<ActivePrefixedToken> {
    let candidate = candidate?;
    if !candidate.text.starts_with(prefix) {
        return None;
    }

    Some(ActivePrefixedToken {
        query: candidate.text[prefix.len_utf8()..].to_string(),
        replace_range: candidate.range,
    })
}

fn clamp_to_char_boundary(text: &str, cursor_offset: usize) -> usize {
    let mut safe_cursor = cursor_offset.min(text.len());
    while safe_cursor > 0 && !text.is_char_boundary(safe_cursor) {
        safe_cursor = safe_cursor.saturating_sub(1);
    }
    safe_cursor
}

#[cfg(test)]
mod tests {
    use super::{ActivePrefixedToken, active_file_completion_token, inserted_path_text};

    fn token(text: &str, cursor_offset: usize) -> Option<ActivePrefixedToken> {
        active_file_completion_token(text, cursor_offset)
    }

    #[test]
    fn active_file_completion_token_handles_basic_cases() {
        assert_eq!(
            token("@src/main.rs", "@src/main.rs".len()),
            Some(ActivePrefixedToken {
                query: "src/main.rs".to_string(),
                replace_range: 0.."@src/main.rs".len(),
            })
        );
        assert_eq!(
            token("read @src/main.rs now", 12),
            Some(ActivePrefixedToken {
                query: "src/main.rs".to_string(),
                replace_range: 5..17,
            })
        );
        assert_eq!(token("read src/main.rs now", 12), None);
    }

    #[test]
    fn active_file_completion_token_tracks_cursor_inside_token() {
        let text = "open @src/main.rs please";

        assert_eq!(
            token(text, 7),
            Some(ActivePrefixedToken {
                query: "src/main.rs".to_string(),
                replace_range: 5..17,
            })
        );
        assert_eq!(
            token(text, 10),
            Some(ActivePrefixedToken {
                query: "src/main.rs".to_string(),
                replace_range: 5..17,
            })
        );
        assert_eq!(
            token(text, 16),
            Some(ActivePrefixedToken {
                query: "src/main.rs".to_string(),
                replace_range: 5..17,
            })
        );
    }

    #[test]
    fn active_file_completion_token_respects_whitespace_boundaries() {
        let text = "alpha @src/main.rs beta @README.md";

        assert_eq!(
            token(text, 18),
            Some(ActivePrefixedToken {
                query: "src/main.rs".to_string(),
                replace_range: 6..18,
            })
        );
        assert_eq!(
            token(text, text.len()),
            Some(ActivePrefixedToken {
                query: "README.md".to_string(),
                replace_range: 24..34,
            })
        );
    }

    #[test]
    fn active_file_completion_token_allows_second_at_inside_token() {
        let text = "@icons/icon@2x.png";

        assert_eq!(
            token(text, 12),
            Some(ActivePrefixedToken {
                query: "icons/icon@2x.png".to_string(),
                replace_range: 0..text.len(),
            })
        );
    }

    #[test]
    fn active_file_completion_token_ignores_mid_word_at() {
        assert_eq!(token("foo@bar", 7), None);
        assert_eq!(token("prefix foo@bar", 14), None);
    }

    #[test]
    fn inserted_path_text_quotes_paths_with_spaces() {
        assert_eq!(inserted_path_text("src/main.rs"), "src/main.rs ");
        assert_eq!(
            inserted_path_text("docs/hello world.md"),
            "\"docs/hello world.md\" "
        );
        assert_eq!(
            inserted_path_text("docs/hello\"world.md"),
            "docs/hello\"world.md "
        );
    }
}
