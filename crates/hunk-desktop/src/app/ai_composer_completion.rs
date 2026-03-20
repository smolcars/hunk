use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use codex_app_server_protocol::{SkillMetadata, SkillScope};
use gpui::{Context, Task, Window};
use gpui_component::{
    Rope, RopeExt,
    input::{CompletionProvider, InputState},
};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    InsertReplaceEdit, Range as LspRange,
};

use crate::app::{AiComposerSkillBinding, AiPromptSkillReference};

use super::fuzzy_match::{is_match_boundary, subsequence_match_score};
use super::repo_file_search::RepoFileSearchProvider;

const AI_COMPOSER_FILE_COMPLETION_LIMIT: usize = 5;
const AI_COMPOSER_SKILL_COMPLETION_LIMIT: usize = 3;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiComposerSkillCompletionItem {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) display_name: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiComposerSkillCompletionMenuState {
    pub(crate) query: String,
    pub(crate) replace_range: Range<usize>,
    pub(crate) items: Vec<AiComposerSkillCompletionItem>,
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

pub(crate) fn active_skill_completion_token(
    text: &str,
    cursor_offset: usize,
) -> Option<ActivePrefixedToken> {
    current_prefixed_token(text, cursor_offset, '$', true)
}

pub(crate) fn skill_completion_menu_state(
    skills: &[SkillMetadata],
    text: &str,
    cursor_offset: usize,
) -> Option<AiComposerSkillCompletionMenuState> {
    let active_token = active_skill_completion_token(text, cursor_offset)?;
    let items = matched_skills(
        skills,
        active_token.query.as_str(),
        AI_COMPOSER_SKILL_COMPLETION_LIMIT,
    );
    if items.is_empty() {
        return None;
    }

    Some(AiComposerSkillCompletionMenuState {
        query: active_token.query,
        replace_range: active_token.replace_range,
        items,
    })
}

pub(crate) fn ai_composer_inserted_skill_text(skill_name: &str) -> String {
    format!("${skill_name} ")
}

pub(crate) fn ai_composer_inserted_skill_binding(
    skill_name: &str,
    path: PathBuf,
    replace_range: Range<usize>,
) -> AiComposerSkillBinding {
    let token = format!("${skill_name}");
    let start = replace_range.start;
    let end = start + token.len();
    AiComposerSkillBinding {
        token,
        range: start..end,
        reference: AiPromptSkillReference {
            name: skill_name.to_string(),
            path,
        },
    }
}

pub(crate) fn reconcile_ai_composer_skill_bindings(
    previous_prompt: &str,
    previous_bindings: &[AiComposerSkillBinding],
    next_prompt: &str,
) -> Vec<AiComposerSkillBinding> {
    if previous_bindings.is_empty() {
        return Vec::new();
    }
    if previous_prompt == next_prompt {
        return previous_bindings.to_vec();
    }

    let Some(edit) = prompt_edit_diff(previous_prompt, next_prompt) else {
        return previous_bindings
            .iter()
            .filter(|binding| binding_token_matches_prompt(binding, next_prompt))
            .cloned()
            .collect();
    };

    previous_bindings
        .iter()
        .filter_map(|binding| binding_after_prompt_edit(binding, next_prompt, &edit))
        .collect()
}

pub(crate) fn selected_skills_from_bindings(
    bindings: &[AiComposerSkillBinding],
    skills: &[SkillMetadata],
) -> Vec<AiPromptSkillReference> {
    if bindings.is_empty() || skills.is_empty() {
        return Vec::new();
    }

    let mut seen_paths = BTreeSet::new();
    let mut resolved = Vec::new();

    for binding in bindings {
        if !skills.iter().any(|skill| {
            skill.enabled
                && skill.name == binding.reference.name
                && skill.path == binding.reference.path
        }) {
            continue;
        }
        if seen_paths.insert(binding.reference.path.clone()) {
            resolved.push(binding.reference.clone());
        }
    }

    resolved
}

pub(crate) fn trim_prompt_with_skill_bindings(
    prompt: &str,
    bindings: &[AiComposerSkillBinding],
) -> (String, Vec<AiComposerSkillBinding>) {
    let trim_start = prompt
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .map(char::len_utf8)
        .sum::<usize>();
    let trim_end = prompt.len().saturating_sub(
        prompt[trim_start..]
            .chars()
            .rev()
            .take_while(|ch| ch.is_whitespace())
            .map(char::len_utf8)
            .sum::<usize>(),
    );
    let trimmed = prompt[trim_start..trim_end].to_string();
    if bindings.is_empty() {
        return (trimmed, Vec::new());
    }

    let trimmed_bindings = bindings
        .iter()
        .filter_map(|binding| {
            if binding.range.start < trim_start || binding.range.end > trim_end {
                return None;
            }
            let shifted = AiComposerSkillBinding {
                range: (binding.range.start - trim_start)..(binding.range.end - trim_start),
                ..binding.clone()
            };
            binding_token_matches_prompt(&shifted, trimmed.as_str()).then_some(shifted)
        })
        .collect();

    (trimmed, trimmed_bindings)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptEditDiff {
    previous_range: Range<usize>,
    next_range: Range<usize>,
}

fn binding_after_prompt_edit(
    binding: &AiComposerSkillBinding,
    next_prompt: &str,
    edit: &PromptEditDiff,
) -> Option<AiComposerSkillBinding> {
    let previous_start = binding.range.start;
    let previous_end = binding.range.end;
    let edited_start = edit.previous_range.start;
    let edited_end = edit.previous_range.end;

    let next_range = if previous_end <= edited_start {
        binding.range.clone()
    } else if previous_start >= edited_end {
        let shifted_start = edit
            .next_range
            .end
            .checked_add(previous_start.saturating_sub(edited_end))?;
        let shifted_end = shifted_start.checked_add(binding.token.len())?;
        shifted_start..shifted_end
    } else {
        return None;
    };

    let next_binding = AiComposerSkillBinding {
        range: next_range,
        ..binding.clone()
    };
    binding_token_matches_prompt(&next_binding, next_prompt).then_some(next_binding)
}

fn binding_token_matches_prompt(binding: &AiComposerSkillBinding, prompt: &str) -> bool {
    prompt
        .get(binding.range.clone())
        .is_some_and(|slice| slice == binding.token)
}

fn prompt_edit_diff(previous_prompt: &str, next_prompt: &str) -> Option<PromptEditDiff> {
    let prefix = common_prefix_len(previous_prompt, next_prompt);
    let previous_suffix = &previous_prompt[prefix..];
    let next_suffix = &next_prompt[prefix..];
    let suffix = common_suffix_len(previous_suffix, next_suffix);

    let previous_end = previous_prompt.len().saturating_sub(suffix);
    let next_end = next_prompt.len().saturating_sub(suffix);
    if prefix > previous_end || prefix > next_end {
        return None;
    }

    Some(PromptEditDiff {
        previous_range: prefix..previous_end,
        next_range: prefix..next_end,
    })
}

fn common_prefix_len(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(left, right)| left == right)
        .map(|(ch, _)| ch.len_utf8())
        .sum()
}

fn common_suffix_len(left: &str, right: &str) -> usize {
    left.chars()
        .rev()
        .zip(right.chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(ch, _)| ch.len_utf8())
        .sum()
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

fn matched_skills(
    skills: &[SkillMetadata],
    query: &str,
    limit: usize,
) -> Vec<AiComposerSkillCompletionItem> {
    let mut ranked = preferred_enabled_skills(skills)
        .into_iter()
        .filter_map(|skill| {
            skill_match_score(query, &skill).map(|score| RankedSkill { skill, score })
        })
        .collect::<Vec<_>>();

    ranked.sort_by(compare_ranked_skills);
    ranked.truncate(limit);
    ranked
        .into_iter()
        .map(|ranked| {
            let description = skill_summary(&ranked.skill);
            AiComposerSkillCompletionItem {
                name: ranked.skill.name,
                path: ranked.skill.path,
                display_name: ranked
                    .skill
                    .interface
                    .as_ref()
                    .and_then(|interface| interface.display_name.clone()),
                description,
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct RankedSkill {
    skill: SkillMetadata,
    score: i32,
}

fn compare_ranked_skills(left: &RankedSkill, right: &RankedSkill) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.skill.name.len().cmp(&right.skill.name.len()))
        .then_with(|| left.skill.name.cmp(&right.skill.name))
        .then_with(|| left.skill.path.cmp(&right.skill.path))
}

fn skill_match_score(query: &str, skill: &SkillMetadata) -> Option<i32> {
    let query = normalize_skill_match_key(query);
    if query.is_empty() {
        return Some(0);
    }

    let name = normalize_skill_match_key(skill.name.as_str());
    let display_name = skill
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .map(normalize_skill_match_key);
    let summary = skill_summary(skill).map(|value| normalize_skill_match_key(value.as_str()));

    let mut best_score = None;
    best_score = merge_score(
        best_score,
        primary_skill_match_score(name.as_str(), query.as_str(), 10_000, 8_900, 8_000, 2_000),
    );
    if let Some(display_name) = display_name.as_ref() {
        best_score = merge_score(
            best_score,
            primary_skill_match_score(
                display_name.as_str(),
                query.as_str(),
                9_600,
                8_500,
                7_600,
                1_800,
            ),
        );
    }
    if let Some(summary) = summary.as_ref() {
        best_score = merge_score(
            best_score,
            secondary_skill_match_score(summary.as_str(), query.as_str(), 5_800, 4_600),
        );
    }

    best_score
}

fn primary_skill_match_score(
    candidate: &str,
    query: &str,
    exact_score: i32,
    starts_with_score: i32,
    contains_score: i32,
    subsequence_floor: i32,
) -> Option<i32> {
    if candidate.is_empty() {
        return None;
    }

    let mut best_score = None;
    if candidate == query {
        best_score = Some(exact_score);
    }

    if candidate.starts_with(query) {
        let score = starts_with_score - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = merge_score(best_score, Some(score));
    }

    if let Some(position) = candidate.find(query) {
        let boundary_bonus = if position == 0
            || is_match_boundary(candidate.as_bytes()[position.saturating_sub(1)])
        {
            180
        } else {
            0
        };
        let score = contains_score + boundary_bonus
            - (position as i32 * 10)
            - (candidate.len() as i32 - query.len() as i32).max(0);
        best_score = merge_score(best_score, Some(score));
    }

    if let Some(score) = subsequence_match_score(candidate, query) {
        best_score = merge_score(best_score, Some(score.max(subsequence_floor)));
    }

    best_score
}

fn secondary_skill_match_score(
    candidate: &str,
    query: &str,
    contains_score: i32,
    subsequence_floor: i32,
) -> Option<i32> {
    if candidate.is_empty() {
        return None;
    }

    let mut best_score = None;
    if let Some(position) = candidate.find(query) {
        let score = contains_score - (position as i32 * 4);
        best_score = merge_score(best_score, Some(score));
    }

    if let Some(score) = subsequence_match_score(candidate, query) {
        best_score = merge_score(best_score, Some(score.max(subsequence_floor)));
    }

    best_score
}

fn merge_score(current: Option<i32>, next: Option<i32>) -> Option<i32> {
    match (current, next) {
        (Some(current), Some(next)) => Some(current.max(next)),
        (None, Some(next)) => Some(next),
        (current, None) => current,
    }
}

fn preferred_enabled_skills(skills: &[SkillMetadata]) -> Vec<SkillMetadata> {
    preferred_enabled_skills_by_name(skills)
        .into_values()
        .collect::<Vec<_>>()
}

fn preferred_enabled_skills_by_name(skills: &[SkillMetadata]) -> BTreeMap<String, SkillMetadata> {
    let mut preferred = BTreeMap::new();

    for skill in skills.iter().filter(|skill| skill.enabled) {
        match preferred.get(skill.name.as_str()) {
            Some(existing) if compare_skill_preference(skill, existing) != Ordering::Less => {}
            _ => {
                preferred.insert(skill.name.clone(), skill.clone());
            }
        }
    }

    preferred
}

fn compare_skill_preference(left: &SkillMetadata, right: &SkillMetadata) -> Ordering {
    skill_scope_rank(left.scope)
        .cmp(&skill_scope_rank(right.scope))
        .then_with(|| left.path.cmp(&right.path))
}

const fn skill_scope_rank(scope: SkillScope) -> u8 {
    match scope {
        SkillScope::Repo => 0,
        SkillScope::User => 1,
        SkillScope::System => 2,
        SkillScope::Admin => 3,
    }
}

fn skill_summary(skill: &SkillMetadata) -> Option<String> {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.short_description.clone())
        .or_else(|| skill.short_description.clone())
        .or_else(|| {
            let description = skill.description.trim();
            (!description.is_empty()).then(|| description.to_string())
        })
}

fn normalize_skill_match_key(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app::{AiComposerSkillBinding, AiPromptSkillReference};
    use codex_app_server_protocol::{SkillInterface, SkillMetadata, SkillScope};

    use super::{
        ActivePrefixedToken, active_file_completion_token, active_skill_completion_token,
        ai_composer_inserted_skill_binding, inserted_path_text,
        reconcile_ai_composer_skill_bindings, selected_skills_from_bindings,
        skill_completion_menu_state,
    };

    fn token(text: &str, cursor_offset: usize) -> Option<ActivePrefixedToken> {
        active_file_completion_token(text, cursor_offset)
    }

    fn skill_token(text: &str, cursor_offset: usize) -> Option<ActivePrefixedToken> {
        active_skill_completion_token(text, cursor_offset)
    }

    fn skill(name: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: format!("{name} skill"),
            short_description: None,
            interface: None,
            dependencies: None,
            path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
            scope: SkillScope::Repo,
            enabled: true,
        }
    }

    fn selected_skill(name: &str) -> AiPromptSkillReference {
        AiPromptSkillReference {
            name: name.to_string(),
            path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
        }
    }

    fn binding(name: &str, start: usize) -> AiComposerSkillBinding {
        AiComposerSkillBinding {
            token: format!("${name}"),
            range: start..start + name.len() + 1,
            reference: selected_skill(name),
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
    fn active_skill_completion_token_shows_bare_dollar() {
        assert_eq!(
            skill_token("$", 1),
            Some(ActivePrefixedToken {
                query: String::new(),
                replace_range: 0..1,
            })
        );
        assert_eq!(
            skill_token("use $", 5),
            Some(ActivePrefixedToken {
                query: String::new(),
                replace_range: 4..5,
            })
        );
    }

    #[test]
    fn active_skill_completion_token_tracks_name_inside_token() {
        let text = "use $gpui-component now";

        assert_eq!(
            skill_token(text, 7),
            Some(ActivePrefixedToken {
                query: "gpui-component".to_string(),
                replace_range: 4..19,
            })
        );
        assert_eq!(
            skill_token(text, 15),
            Some(ActivePrefixedToken {
                query: "gpui-component".to_string(),
                replace_range: 4..19,
            })
        );
    }

    #[test]
    fn skill_completion_menu_ranks_name_matches_over_description_matches() {
        let mut docs = skill("openai-docs");
        docs.description = "Official OpenAI docs".to_string();
        let mut creator = skill("skill-creator");
        creator.description = "Create new skills".to_string();

        let menu =
            skill_completion_menu_state(&[creator, docs], "$skill", 6).expect("menu should exist");
        assert_eq!(menu.items[0].name, "skill-creator");
    }

    #[test]
    fn skill_completion_menu_uses_display_name_and_summary() {
        let mut gpui = skill("gpui-component");
        gpui.interface = Some(SkillInterface {
            display_name: Some("GPUI Component".to_string()),
            short_description: Some("Reusable GPUI UI components".to_string()),
            icon_small: None,
            icon_large: None,
            brand_color: None,
            default_prompt: None,
        });

        let menu = skill_completion_menu_state(&[gpui], "$", 1)
            .expect("menu should exist for bare dollar");
        assert_eq!(
            menu.items[0].display_name.as_deref(),
            Some("GPUI Component")
        );
        assert_eq!(
            menu.items[0].description.as_deref(),
            Some("Reusable GPUI UI components")
        );
    }

    #[test]
    fn ai_composer_inserted_skill_binding_tracks_token_range() {
        let binding = ai_composer_inserted_skill_binding(
            "gpui",
            PathBuf::from("/skills/gpui/SKILL.md"),
            4..9,
        );

        assert_eq!(binding.token, "$gpui");
        assert_eq!(binding.range, 4..9);
        assert_eq!(binding.reference, selected_skill("gpui"));
    }

    #[test]
    fn reconcile_skill_bindings_shifts_ranges_after_prefix_insert() {
        let bindings = vec![binding("gpui", 4)];

        let reconciled = reconcile_ai_composer_skill_bindings(
            "Use $gpui now",
            bindings.as_slice(),
            "Please use $gpui now",
        );

        assert_eq!(reconciled, vec![binding("gpui", 11)]);
    }

    #[test]
    fn reconcile_skill_bindings_drops_binding_when_token_is_edited() {
        let bindings = vec![binding("gpui", 4)];

        let reconciled = reconcile_ai_composer_skill_bindings(
            "Use $gpui now",
            bindings.as_slice(),
            "Use $gpux now",
        );

        assert!(reconciled.is_empty());
    }

    #[test]
    fn selected_skills_from_bindings_uses_only_matching_enabled_skills() {
        let mut repo_gpui = skill("gpui");
        repo_gpui.scope = SkillScope::Repo;
        repo_gpui.path = PathBuf::from("/repo/.codex/skills/gpui/SKILL.md");

        let mut installer = skill("skill-installer");
        installer.enabled = false;

        let bindings = vec![
            AiComposerSkillBinding {
                token: "$gpui".to_string(),
                range: 0..5,
                reference: AiPromptSkillReference {
                    name: "gpui".to_string(),
                    path: repo_gpui.path.clone(),
                },
            },
            AiComposerSkillBinding {
                token: "$skill-installer".to_string(),
                range: 10..26,
                reference: AiPromptSkillReference {
                    name: "skill-installer".to_string(),
                    path: installer.path.clone(),
                },
            },
        ];

        let selected =
            selected_skills_from_bindings(bindings.as_slice(), &[repo_gpui.clone(), installer]);

        assert_eq!(
            selected,
            vec![AiPromptSkillReference {
                name: "gpui".to_string(),
                path: repo_gpui.path,
            }]
        );
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
