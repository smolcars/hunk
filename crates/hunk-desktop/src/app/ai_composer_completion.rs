use std::cmp::Ordering;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

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

use super::fuzzy_match::{is_match_boundary, segment_prefix_position, subsequence_match_score};

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

#[derive(Debug, Clone)]
struct SearchableRepoFile {
    path: String,
    normalized_path: String,
    normalized_file_name: String,
}

#[derive(Debug, Default)]
struct AiComposerFileCompletionState {
    repo_root: Option<PathBuf>,
    files: Arc<[SearchableRepoFile]>,
    reload_generation: u64,
}

pub(crate) struct AiComposerFileCompletionProvider {
    state: RwLock<AiComposerFileCompletionState>,
}

impl AiComposerFileCompletionProvider {
    pub(crate) fn new() -> Self {
        Self {
            state: RwLock::new(AiComposerFileCompletionState::default()),
        }
    }

    pub(crate) fn begin_reload(&self, repo_root: Option<PathBuf>) -> u64 {
        let mut state = self.write_state();
        state.reload_generation = state.reload_generation.wrapping_add(1);
        if state.repo_root.as_ref() != repo_root.as_ref() {
            state.files = Arc::from(Vec::<SearchableRepoFile>::new());
        }
        state.repo_root = repo_root;
        state.reload_generation
    }

    pub(crate) fn apply_reload(
        &self,
        generation: u64,
        repo_root: &Path,
        paths: Vec<String>,
    ) -> bool {
        let mut state = self.write_state();
        if state.reload_generation != generation || state.repo_root.as_deref() != Some(repo_root) {
            return false;
        }

        state.files = searchable_repo_files(paths);
        true
    }

    pub(crate) fn clear(&self) {
        let _ = self.begin_reload(None);
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

        let files = self.searchable_files();
        if files.is_empty() {
            return None;
        }

        let items = ranked_file_matches(files.as_ref(), active_token.query.as_str())
            .into_iter()
            .map(|ranked| ranked.file.path.clone())
            .collect::<Vec<_>>();
        if items.is_empty() {
            return None;
        }

        Some(AiComposerFileCompletionMenuState {
            query: active_token.query,
            replace_range: active_token.replace_range,
            items,
        })
    }

    fn searchable_files(&self) -> Arc<[SearchableRepoFile]> {
        self.read_state().files.clone()
    }

    fn read_state(&self) -> RwLockReadGuard<'_, AiComposerFileCompletionState> {
        match self.state.read() {
            Ok(guard) => guard,
            Err(error) => error.into_inner(),
        }
    }

    fn write_state(&self) -> RwLockWriteGuard<'_, AiComposerFileCompletionState> {
        match self.state.write() {
            Ok(guard) => guard,
            Err(error) => error.into_inner(),
        }
    }
}

impl Default for AiComposerFileCompletionProvider {
    fn default() -> Self {
        Self::new()
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

        let files = self.searchable_files();
        if files.is_empty() {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }

        let replace_range = lsp_replace_range(text.as_str(), &active_token.replace_range);
        let items = ranked_file_matches(files.as_ref(), active_token.query.as_str())
            .into_iter()
            .map(|ranked| completion_item_for_path(replace_range, ranked.file.path.as_str()))
            .collect::<Vec<_>>();

        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, _: &str, _: &mut Context<InputState>) -> bool {
        true
    }
}

fn searchable_repo_files(paths: Vec<String>) -> Arc<[SearchableRepoFile]> {
    paths
        .into_iter()
        .map(|path| SearchableRepoFile {
            normalized_path: normalize_file_match_key(path.as_str()),
            normalized_file_name: normalize_file_match_key(file_name_from_path(path.as_str())),
            path,
        })
        .collect::<Vec<_>>()
        .into()
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

#[derive(Debug)]
struct RankedRepoFile<'a> {
    file: &'a SearchableRepoFile,
    score: i32,
}

fn ranked_file_matches<'a>(
    files: &'a [SearchableRepoFile],
    query: &str,
) -> Vec<RankedRepoFile<'a>> {
    let mut ranked = Vec::with_capacity(AI_COMPOSER_FILE_COMPLETION_LIMIT);

    for file in files {
        let Some(score) = file_match_score(query, file) else {
            continue;
        };

        ranked.push(RankedRepoFile { file, score });
        ranked.sort_by(compare_ranked_repo_files);
        if ranked.len() > AI_COMPOSER_FILE_COMPLETION_LIMIT {
            ranked.truncate(AI_COMPOSER_FILE_COMPLETION_LIMIT);
        }
    }

    ranked
}

fn compare_ranked_repo_files(left: &RankedRepoFile<'_>, right: &RankedRepoFile<'_>) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.file.path.len().cmp(&right.file.path.len()))
        .then_with(|| left.file.path.cmp(&right.file.path))
}

fn file_match_score(query: &str, file: &SearchableRepoFile) -> Option<i32> {
    let query = normalize_file_match_key(query);
    if query.is_empty() {
        return Some(0);
    }

    let candidate = file.normalized_path.as_str();
    let file_name = file.normalized_file_name.as_str();
    if candidate.is_empty() {
        return None;
    }

    let mut best_score = None;

    if candidate == query {
        best_score = Some(10_000);
    }

    if file_name == query {
        best_score = Some(best_score.map_or(9_600, |current| current.max(9_600)));
    }

    if file_name.starts_with(query.as_str()) {
        let score = 8_900 - (file_name.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = file_name.find(query.as_str()) {
        let boundary_bonus = if position == 0
            || is_match_boundary(file_name.as_bytes()[position.saturating_sub(1)])
        {
            220
        } else {
            0
        };
        let score = 8_000 + boundary_bonus
            - (position as i32 * 12)
            - (file_name.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if candidate.starts_with(query.as_str()) {
        let score = 8_400 - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = segment_prefix_position(candidate, query.as_str()) {
        let score =
            7_200 - (position as i32 * 8) - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(position) = candidate.find(query.as_str()) {
        let boundary_bonus = if position == 0
            || is_match_boundary(candidate.as_bytes()[position.saturating_sub(1)])
        {
            180
        } else {
            0
        };
        let score = 6_400 + boundary_bonus
            - (position as i32 * 10)
            - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    if let Some(score) = subsequence_match_score(candidate, query.as_str()) {
        best_score = Some(best_score.map_or(score, |current| current.max(score)));
    }

    best_score
}

fn file_name_from_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn normalize_file_match_key(value: &str) -> String {
    value.trim().to_lowercase().replace('\\', "/")
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
    use super::{
        ActivePrefixedToken, SearchableRepoFile, active_file_completion_token, file_match_score,
        inserted_path_text, ranked_file_matches,
    };

    fn token(text: &str, cursor_offset: usize) -> Option<ActivePrefixedToken> {
        active_file_completion_token(text, cursor_offset)
    }

    fn searchable_file(path: &str) -> SearchableRepoFile {
        SearchableRepoFile {
            path: path.to_string(),
            normalized_path: path.to_lowercase(),
            normalized_file_name: path.rsplit('/').next().unwrap_or(path).to_lowercase(),
        }
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

    #[test]
    fn file_match_score_prefers_file_name_matches() {
        let exact_name = searchable_file("src/main.rs");
        let substring = searchable_file("docs/domain-guide.md");

        let exact_name_score = file_match_score("main", &exact_name).expect("match score");
        let substring_score = file_match_score("main", &substring).expect("match score");

        assert!(exact_name_score > substring_score);
    }

    #[test]
    fn ranked_file_matches_sort_by_relevance_then_shorter_paths() {
        let files = vec![
            searchable_file("src/main.rs"),
            searchable_file("crates/hunk-desktop/src/main.rs"),
            searchable_file("README.md"),
        ];

        let ranked = ranked_file_matches(&files, "main");

        assert_eq!(ranked[0].file.path, "src/main.rs");
        assert_eq!(ranked[1].file.path, "crates/hunk-desktop/src/main.rs");
    }
}
