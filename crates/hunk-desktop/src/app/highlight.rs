use hunk_domain::diff::DiffCellKind;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SyntaxTokenKind {
    Plain,
    Keyword,
    String,
    Number,
    Comment,
    Function,
    TypeName,
    Constant,
    Variable,
    Operator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StyledSegment {
    pub(super) text: String,
    pub(super) syntax: SyntaxTokenKind,
    pub(super) changed: bool,
}

const MAX_INTRA_LINE_DIFF_CHARS: usize = 8_192;
const MAX_INTRA_LINE_DIFF_TOKENS: usize = 768;
const MAX_INTRA_LINE_LCS_MATRIX_CELLS: usize = 80_000;

pub(super) fn build_line_segments(
    file_path: Option<&str>,
    line: &str,
    kind: DiffCellKind,
    peer_line: &str,
    peer_kind: DiffCellKind,
) -> Vec<StyledSegment> {
    if line.is_empty() {
        return Vec::new();
    }

    let chars = line.chars().collect::<Vec<_>>();
    let mut syntax_map = vec![SyntaxTokenKind::Plain; chars.len()];
    apply_syntect_syntax_map(file_path, line, &mut syntax_map);

    let changed_map = intra_line_change_map(&chars, kind, peer_line, peer_kind);
    merge_styled_segments(&chars, &syntax_map, &changed_map)
}

pub(super) fn build_syntax_only_line_segments(
    file_path: Option<&str>,
    line: &str,
) -> Vec<StyledSegment> {
    if line.is_empty() {
        return Vec::new();
    }

    let chars = line.chars().collect::<Vec<_>>();
    let mut syntax_map = vec![SyntaxTokenKind::Plain; chars.len()];
    apply_syntect_syntax_map(file_path, line, &mut syntax_map);
    let changed_map = vec![false; chars.len()];
    merge_styled_segments(&chars, &syntax_map, &changed_map)
}

#[cfg(test)]
#[allow(dead_code)]
pub(super) fn build_plain_line_segments(file_path: Option<&str>, line: &str) -> Vec<StyledSegment> {
    build_syntax_only_line_segments(file_path, line)
}

#[allow(dead_code)]
pub(super) fn render_with_whitespace_markers(text: &str) -> String {
    let mut rendered = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            ' ' => rendered.push('·'),
            '\t' => rendered.push('⇥'),
            _ => rendered.push(ch),
        }
    }
    rendered
}

fn apply_syntect_syntax_map(
    file_path: Option<&str>,
    line: &str,
    syntax_map: &mut [SyntaxTokenKind],
) {
    let is_toml = file_extension(file_path).as_deref() == Some("toml");
    let syntax_set = syntax_set();
    if let Some(syntax) = syntax_for_path(syntax_set, file_path) {
        apply_syntect_scope_map(syntax_set, syntax, line, syntax_map);
    }

    if is_toml {
        apply_toml_fallback_syntax_map(line, syntax_map);
    }
}

fn apply_syntect_scope_map(
    syntax_set: &SyntaxSet,
    syntax: &SyntaxReference,
    line: &str,
    syntax_map: &mut [SyntaxTokenKind],
) {
    let mut parse_state = ParseState::new(syntax);
    let Ok(ops) = parse_state.parse_line(line, syntax_set) else {
        return;
    };

    let mut scope_stack = ScopeStack::new();
    let mut start = 0_usize;
    for (region, op) in ScopeRegionIterator::new(&ops, line) {
        let end = (start + region.chars().count()).min(syntax_map.len());
        let token = if scope_stack.apply(op).is_ok() {
            syntax_token_from_scope_stack(&scope_stack)
        } else {
            SyntaxTokenKind::Plain
        };
        for kind in syntax_map.iter_mut().take(end).skip(start) {
            *kind = token;
        }
        start = end;
        if start >= syntax_map.len() {
            break;
        }
    }
}

fn file_extension(file_path: Option<&str>) -> Option<String> {
    Path::new(file_path?)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn apply_toml_fallback_syntax_map(line: &str, syntax_map: &mut [SyntaxTokenKind]) {
    if line.is_empty() || syntax_map.is_empty() {
        return;
    }

    let chars = line.chars().collect::<Vec<_>>();
    let content_end = find_unquoted_comment_start(&chars).unwrap_or(chars.len());

    if content_end < chars.len() {
        mark_range_if_plain(
            syntax_map,
            content_end,
            chars.len(),
            SyntaxTokenKind::Comment,
        );
    }

    mark_toml_strings(&chars, content_end, syntax_map);
    mark_toml_table_header(&chars, content_end, syntax_map);
    mark_toml_key_and_operator(&chars, content_end, syntax_map);
    mark_toml_literals(&chars, content_end, syntax_map);
}

fn find_unquoted_comment_start(chars: &[char]) -> Option<usize> {
    let mut ix = 0_usize;
    let mut string_delim = None;
    let mut escaped = false;
    while ix < chars.len() {
        let ch = chars[ix];
        if let Some(delim) = string_delim {
            if delim == '"' && ch == '\\' && !escaped {
                escaped = true;
                ix = ix.saturating_add(1);
                continue;
            }
            if ch == delim && !(delim == '"' && escaped) {
                string_delim = None;
            }
            escaped = false;
            ix = ix.saturating_add(1);
            continue;
        }

        if ch == '"' || ch == '\'' {
            string_delim = Some(ch);
            escaped = false;
            ix = ix.saturating_add(1);
            continue;
        }

        if ch == '#' {
            return Some(ix);
        }

        ix = ix.saturating_add(1);
    }

    None
}

fn mark_toml_strings(chars: &[char], limit: usize, syntax_map: &mut [SyntaxTokenKind]) {
    let mut ix = 0_usize;
    while ix < limit {
        if chars[ix] != '"' && chars[ix] != '\'' {
            ix = ix.saturating_add(1);
            continue;
        }

        let start = ix;
        let delim = chars[ix];
        ix = ix.saturating_add(1);
        let mut escaped = false;
        while ix < limit {
            let ch = chars[ix];
            if delim == '"' && ch == '\\' && !escaped {
                escaped = true;
                ix = ix.saturating_add(1);
                continue;
            }

            if ch == delim && !(delim == '"' && escaped) {
                ix = ix.saturating_add(1);
                break;
            }

            escaped = false;
            ix = ix.saturating_add(1);
        }

        mark_range_if_plain(syntax_map, start, ix, SyntaxTokenKind::String);
    }
}

fn mark_toml_table_header(chars: &[char], limit: usize, syntax_map: &mut [SyntaxTokenKind]) {
    let Some(first_non_ws) = (0..limit).find(|&ix| !chars[ix].is_whitespace()) else {
        return;
    };
    if chars[first_non_ws] != '[' {
        return;
    }

    let Some(last_non_ws) = (0..limit).rfind(|&ix| !chars[ix].is_whitespace()) else {
        return;
    };
    if chars[last_non_ws] != ']' {
        return;
    }

    mark_range_if_plain(
        syntax_map,
        first_non_ws,
        last_non_ws.saturating_add(1),
        SyntaxTokenKind::TypeName,
    );
}

fn mark_toml_key_and_operator(chars: &[char], limit: usize, syntax_map: &mut [SyntaxTokenKind]) {
    let Some(equal_ix) = find_first_unquoted_char(chars, limit, '=') else {
        return;
    };

    mark_char_if_plain(syntax_map, equal_ix, SyntaxTokenKind::Operator);

    let key_start = (0..equal_ix)
        .find(|&ix| !chars[ix].is_whitespace())
        .unwrap_or(equal_ix);
    let key_end = (0..equal_ix)
        .rfind(|&ix| !chars[ix].is_whitespace())
        .map(|ix| ix.saturating_add(1))
        .unwrap_or(key_start);

    for (ix, ch) in chars.iter().enumerate().take(key_end).skip(key_start) {
        if ch.is_whitespace() {
            continue;
        }
        mark_char_if_plain(syntax_map, ix, SyntaxTokenKind::Variable);
    }
}

fn mark_toml_literals(chars: &[char], limit: usize, syntax_map: &mut [SyntaxTokenKind]) {
    let mut ix = 0_usize;
    while ix < limit {
        if syntax_map[ix] != SyntaxTokenKind::Plain {
            ix = ix.saturating_add(1);
            continue;
        }

        if starts_toml_number(chars, ix, limit) {
            let start = ix;
            ix = consume_toml_number(chars, ix, limit);
            mark_range_if_plain(syntax_map, start, ix, SyntaxTokenKind::Number);
            continue;
        }

        if chars[ix].is_ascii_alphabetic() {
            let start = ix;
            ix = consume_toml_word(chars, ix, limit);
            let token = chars[start..ix]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase();
            if token == "true" || token == "false" {
                mark_range_if_plain(syntax_map, start, ix, SyntaxTokenKind::Constant);
            }
            continue;
        }

        ix = ix.saturating_add(1);
    }
}

fn starts_toml_number(chars: &[char], ix: usize, limit: usize) -> bool {
    if ix >= limit {
        return false;
    }

    if chars[ix].is_ascii_digit() {
        return true;
    }

    (chars[ix] == '-' || chars[ix] == '+') && ix + 1 < limit && chars[ix + 1].is_ascii_digit()
}

fn consume_toml_number(chars: &[char], mut ix: usize, limit: usize) -> usize {
    while ix < limit
        && (chars[ix].is_ascii_alphanumeric()
            || matches!(
                chars[ix],
                '_' | '.' | '-' | '+' | ':' | 'T' | 'Z' | 't' | 'z'
            ))
    {
        ix = ix.saturating_add(1);
    }
    ix
}

fn consume_toml_word(chars: &[char], mut ix: usize, limit: usize) -> usize {
    while ix < limit && (chars[ix].is_ascii_alphanumeric() || matches!(chars[ix], '_' | '-')) {
        ix = ix.saturating_add(1);
    }
    ix
}

fn find_first_unquoted_char(chars: &[char], limit: usize, needle: char) -> Option<usize> {
    let mut ix = 0_usize;
    let mut string_delim = None;
    let mut escaped = false;
    while ix < limit {
        let ch = chars[ix];
        if let Some(delim) = string_delim {
            if delim == '"' && ch == '\\' && !escaped {
                escaped = true;
                ix = ix.saturating_add(1);
                continue;
            }
            if ch == delim && !(delim == '"' && escaped) {
                string_delim = None;
            }
            escaped = false;
            ix = ix.saturating_add(1);
            continue;
        }

        if ch == '"' || ch == '\'' {
            string_delim = Some(ch);
            escaped = false;
            ix = ix.saturating_add(1);
            continue;
        }

        if ch == needle {
            return Some(ix);
        }

        ix = ix.saturating_add(1);
    }

    None
}

fn mark_range_if_plain(
    syntax_map: &mut [SyntaxTokenKind],
    start: usize,
    end: usize,
    token: SyntaxTokenKind,
) {
    let end = end.min(syntax_map.len());
    for kind in syntax_map.iter_mut().take(end).skip(start) {
        if *kind == SyntaxTokenKind::Plain {
            *kind = token;
        }
    }
}

fn mark_char_if_plain(syntax_map: &mut [SyntaxTokenKind], ix: usize, token: SyntaxTokenKind) {
    if let Some(kind) = syntax_map.get_mut(ix)
        && *kind == SyntaxTokenKind::Plain
    {
        *kind = token;
    }
}

fn syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_nonewlines)
}

fn syntax_for_path<'a>(
    syntax_set: &'a SyntaxSet,
    file_path: Option<&str>,
) -> Option<&'a SyntaxReference> {
    let file_path = file_path?;
    if let Some(cached) = lookup_syntax_name_cache(file_path) {
        return cached.and_then(|name| syntax_set.find_syntax_by_name(name.as_str()));
    }

    let path = Path::new(file_path);
    let file_name = path.file_name()?.to_str()?;
    let resolved = if let Some(tokens) = special_file_tokens(file_name)
        && let Some(syntax) = find_first_syntax_by_tokens(syntax_set, tokens)
    {
        Some(syntax)
    } else if let Some(extension) = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    {
        if let Some(tokens) = language_tokens_for_extension(&extension)
            && let Some(syntax) = find_first_syntax_by_tokens(syntax_set, tokens)
        {
            Some(syntax)
        } else {
            syntax_set.find_syntax_by_extension(&extension)
        }
    } else {
        None
    };

    store_syntax_name_cache(file_path, resolved.map(|syntax| syntax.name.clone()));
    resolved
}

fn syntax_name_cache() -> &'static RwLock<HashMap<String, Option<String>>> {
    static SYNTAX_NAME_CACHE: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();
    SYNTAX_NAME_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn lookup_syntax_name_cache(file_path: &str) -> Option<Option<String>> {
    syntax_name_cache()
        .read()
        .ok()
        .and_then(|cache| cache.get(file_path).cloned())
}

fn store_syntax_name_cache(file_path: &str, syntax_name: Option<String>) {
    let Ok(mut cache) = syntax_name_cache().write() else {
        return;
    };
    const SYNTAX_NAME_CACHE_MAX_ENTRIES: usize = 4096;
    if cache.len() >= SYNTAX_NAME_CACHE_MAX_ENTRIES {
        cache.clear();
    }
    cache.insert(file_path.to_string(), syntax_name);
}

fn find_first_syntax_by_tokens<'a>(
    syntax_set: &'a SyntaxSet,
    tokens: &[&str],
) -> Option<&'a SyntaxReference> {
    tokens
        .iter()
        .find_map(|token| syntax_set.find_syntax_by_token(token))
}

fn special_file_tokens(file_name: &str) -> Option<&'static [&'static str]> {
    if file_name.eq_ignore_ascii_case("dockerfile") {
        return Some(&["dockerfile", "docker", "sh"]);
    }
    None
}

fn language_tokens_for_extension(extension: &str) -> Option<&'static [&'static str]> {
    match extension {
        // JavaScript / TypeScript
        "js" | "jsx" | "mjs" | "cjs" => Some(&["js", "javascript"]),
        "ts" | "tsx" => Some(&["ts", "typescript", "js"]),
        // Systems languages
        "go" => Some(&["go"]),
        "rs" => Some(&["rs", "rust"]),
        "swift" => Some(&["swift"]),
        "kt" | "kts" => Some(&["kotlin", "java"]),
        "java" => Some(&["java"]),
        // C / C++
        "c" | "h" => Some(&["c", "cpp"]),
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Some(&["cpp", "c++", "c"]),
        // Scripting and config
        "py" | "pyi" => Some(&["py", "python"]),
        "json" | "jsonc" => Some(&["json", "js"]),
        "yml" | "yaml" => Some(&["yaml", "yml"]),
        "toml" => Some(&["toml"]),
        "tf" | "tfvars" | "hcl" => Some(&["terraform", "tf", "hcl"]),
        _ => None,
    }
}

fn syntax_token_from_scope_stack(scope_stack: &ScopeStack) -> SyntaxTokenKind {
    for scope in scope_stack.as_slice().iter().rev() {
        let scope_name = scope.build_string();
        if is_comment_scope(&scope_name) {
            return SyntaxTokenKind::Comment;
        }
        if is_string_scope(&scope_name) {
            return SyntaxTokenKind::String;
        }
        if is_number_scope(&scope_name) {
            return SyntaxTokenKind::Number;
        }
        if is_function_scope(&scope_name) {
            return SyntaxTokenKind::Function;
        }
        if is_type_scope(&scope_name) {
            return SyntaxTokenKind::TypeName;
        }
        if is_constant_scope(&scope_name) {
            return SyntaxTokenKind::Constant;
        }
        if is_keyword_scope(&scope_name) {
            return SyntaxTokenKind::Keyword;
        }
        if is_variable_scope(&scope_name) {
            return SyntaxTokenKind::Variable;
        }
        if is_operator_scope(&scope_name) {
            return SyntaxTokenKind::Operator;
        }
    }
    SyntaxTokenKind::Plain
}

fn is_comment_scope(scope_name: &str) -> bool {
    scope_name.starts_with("comment")
        || scope_name.contains(".comment.")
        || scope_name.ends_with(".comment")
}

fn is_string_scope(scope_name: &str) -> bool {
    scope_name.starts_with("string")
        || scope_name.contains(".string.")
        || scope_name.ends_with(".string")
}

fn is_number_scope(scope_name: &str) -> bool {
    scope_name.starts_with("constant.numeric")
        || scope_name.contains(".constant.numeric.")
        || scope_name.contains(".number.")
        || scope_name.ends_with(".number")
        || scope_name.ends_with(".numeric")
}

fn is_function_scope(scope_name: &str) -> bool {
    scope_name.starts_with("entity.name.function")
        || scope_name.contains(".entity.name.function.")
        || scope_name.starts_with("support.function")
        || scope_name.contains(".support.function.")
        || scope_name.starts_with("variable.function")
        || scope_name.contains(".variable.function.")
        || scope_name.starts_with("meta.function")
}

fn is_type_scope(scope_name: &str) -> bool {
    scope_name.starts_with("entity.name.type")
        || scope_name.contains(".entity.name.type.")
        || scope_name.starts_with("entity.name.class")
        || scope_name.contains(".entity.name.class.")
        || scope_name.starts_with("support.type")
        || scope_name.contains(".support.type.")
        || scope_name.starts_with("storage.type")
        || scope_name.contains(".storage.type.")
}

fn is_constant_scope(scope_name: &str) -> bool {
    scope_name.starts_with("constant")
        || scope_name.contains(".constant.")
        || scope_name.ends_with(".constant")
}

fn is_keyword_scope(scope_name: &str) -> bool {
    scope_name.starts_with("keyword")
        || scope_name.contains(".keyword.")
        || scope_name.ends_with(".keyword")
        || scope_name.starts_with("storage.modifier")
        || scope_name.contains(".storage.modifier.")
        || scope_name.starts_with("storage.control")
        || scope_name.contains(".storage.control.")
}

fn is_variable_scope(scope_name: &str) -> bool {
    scope_name.starts_with("variable")
        || scope_name.contains(".variable.")
        || scope_name.starts_with("entity.name.variable")
        || scope_name.contains(".entity.name.variable.")
        || scope_name.starts_with("support.variable")
        || scope_name.contains(".support.variable.")
}

fn is_operator_scope(scope_name: &str) -> bool {
    scope_name.starts_with("keyword.operator")
        || scope_name.contains(".keyword.operator.")
        || scope_name.starts_with("punctuation")
        || scope_name.contains(".punctuation.")
}

fn intra_line_change_map(
    line_chars: &[char],
    kind: DiffCellKind,
    peer_line: &str,
    peer_kind: DiffCellKind,
) -> Vec<bool> {
    let should_diff_pair = matches!(
        (kind, peer_kind),
        (DiffCellKind::Removed, DiffCellKind::Added) | (DiffCellKind::Added, DiffCellKind::Removed)
    );
    if !should_diff_pair {
        return vec![false; line_chars.len()];
    }

    let peer_chars = peer_line.chars().collect::<Vec<_>>();
    if line_chars.len() > MAX_INTRA_LINE_DIFF_CHARS || peer_chars.len() > MAX_INTRA_LINE_DIFF_CHARS
    {
        return coarse_changed_map(line_chars, &peer_chars);
    }

    let line_tokens = tokenize(line_chars);
    let peer_tokens = tokenize(&peer_chars);
    let token_matrix_size = line_tokens.len().saturating_mul(peer_tokens.len());
    if line_tokens.len() > MAX_INTRA_LINE_DIFF_TOKENS
        || peer_tokens.len() > MAX_INTRA_LINE_DIFF_TOKENS
        || token_matrix_size > MAX_INTRA_LINE_LCS_MATRIX_CELLS
    {
        return coarse_changed_map(line_chars, &peer_chars);
    }

    let left_is_current = kind == DiffCellKind::Removed;
    let (common_current, common_peer) = if left_is_current {
        lcs_common_token_flags(&line_tokens, &peer_tokens)
    } else {
        lcs_common_token_flags(&peer_tokens, &line_tokens)
    };
    let common_flags = if left_is_current {
        common_current
    } else {
        common_peer
    };

    let mut changed_map = vec![false; line_chars.len()];
    for (ix, token) in line_tokens.iter().enumerate() {
        let is_common = common_flags.get(ix).copied().unwrap_or(false);
        if is_common {
            continue;
        }
        for changed in changed_map.iter_mut().take(token.end).skip(token.start) {
            *changed = true;
        }
    }

    changed_map
}

fn coarse_changed_map(line_chars: &[char], peer_chars: &[char]) -> Vec<bool> {
    if line_chars == peer_chars {
        return vec![false; line_chars.len()];
    }
    vec![true; line_chars.len()]
}

#[derive(Debug, Clone)]
struct Token {
    start: usize,
    end: usize,
    text: String,
}

fn tokenize(chars: &[char]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut ix = 0_usize;

    while ix < chars.len() {
        let start = ix;
        let kind = if chars[ix].is_ascii_alphanumeric() || chars[ix] == '_' {
            0_u8
        } else if chars[ix].is_whitespace() {
            1_u8
        } else {
            2_u8
        };

        ix = ix.saturating_add(1);
        while ix < chars.len() {
            let next_kind = if chars[ix].is_ascii_alphanumeric() || chars[ix] == '_' {
                0_u8
            } else if chars[ix].is_whitespace() {
                1_u8
            } else {
                2_u8
            };
            if next_kind != kind {
                break;
            }
            ix = ix.saturating_add(1);
        }

        tokens.push(Token {
            start,
            end: ix,
            text: chars[start..ix].iter().collect::<String>(),
        });
    }

    tokens
}

fn lcs_common_token_flags(left: &[Token], right: &[Token]) -> (Vec<bool>, Vec<bool>) {
    let n = left.len();
    let m = right.len();
    let mut dp = vec![vec![0_u16; m + 1]; n + 1];

    for i in 0..n {
        for (j, right_token) in right.iter().enumerate().take(m) {
            dp[i + 1][j + 1] = if left[i].text == right_token.text {
                dp[i][j].saturating_add(1)
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut left_common = vec![false; n];
    let mut right_common = vec![false; m];
    let mut i = n;
    let mut j = m;
    while i > 0 && j > 0 {
        if left[i - 1].text == right[j - 1].text {
            left_common[i - 1] = true;
            right_common[j - 1] = true;
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    (left_common, right_common)
}

fn merge_styled_segments(
    chars: &[char],
    syntax_map: &[SyntaxTokenKind],
    changed_map: &[bool],
) -> Vec<StyledSegment> {
    if chars.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut start = 0_usize;
    for ix in 1..chars.len() {
        if syntax_map[ix] == syntax_map[start] && changed_map[ix] == changed_map[start] {
            continue;
        }

        segments.push(StyledSegment {
            text: chars[start..ix].iter().collect::<String>(),
            syntax: syntax_map[start],
            changed: changed_map[start],
        });
        start = ix;
    }

    segments.push(StyledSegment {
        text: chars[start..].iter().collect::<String>(),
        syntax: syntax_map[start],
        changed: changed_map[start],
    });
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_changed_token_pairs() {
        let left = build_line_segments(
            Some("src/main.rs"),
            "let answer = 42;",
            DiffCellKind::Removed,
            "let answer = 7;",
            DiffCellKind::Added,
        );
        let right = build_line_segments(
            Some("src/main.rs"),
            "let answer = 7;",
            DiffCellKind::Added,
            "let answer = 42;",
            DiffCellKind::Removed,
        );

        assert!(left.iter().any(|segment| segment.changed));
        assert!(right.iter().any(|segment| segment.changed));
    }

    #[test]
    fn falls_back_to_coarse_change_map_for_large_line_pairs() {
        let base = "x ".repeat(MAX_INTRA_LINE_DIFF_CHARS + 128);
        let mut modified = base.clone();
        modified.push('y');

        let segments = build_line_segments(
            Some("src/main.ts"),
            &base,
            DiffCellKind::Removed,
            &modified,
            DiffCellKind::Added,
        );
        assert!(!segments.is_empty());
        assert!(segments.iter().any(|segment| segment.changed));
    }

    #[test]
    fn coarse_change_map_keeps_identical_large_pairs_unchanged() {
        let line = "token ".repeat(MAX_INTRA_LINE_DIFF_CHARS + 64);
        let segments = build_line_segments(
            Some("src/main.ts"),
            &line,
            DiffCellKind::Removed,
            &line,
            DiffCellKind::Added,
        );

        assert!(!segments.is_empty());
        assert!(segments.iter().all(|segment| !segment.changed));
    }

    #[test]
    fn marks_rust_keywords() {
        let segments = build_line_segments(
            Some("src/main.rs"),
            "fn main() { let v = 1; }",
            DiffCellKind::Context,
            "",
            DiffCellKind::Context,
        );
        assert!(
            segments
                .iter()
                .any(|segment| segment.syntax == SyntaxTokenKind::Keyword)
        );
    }

    #[test]
    fn marks_python_keywords() {
        let segments = build_line_segments(
            Some("main.py"),
            "def main():",
            DiffCellKind::Context,
            "",
            DiffCellKind::Context,
        );
        assert!(segments.iter().any(|segment| {
            matches!(
                segment.syntax,
                SyntaxTokenKind::Keyword | SyntaxTokenKind::TypeName
            )
        }));
    }

    #[test]
    fn resolves_supported_languages() {
        let syntax_set = syntax_set();
        let required_paths = [
            "file.js",
            "file.ts",
            "file.go",
            "file.rs",
            "file.py",
            "Main.java",
            "file.c",
            "file.cpp",
            "Dockerfile",
            "config.yaml",
            "file.json",
        ];

        for path in required_paths {
            assert!(
                syntax_for_path(syntax_set, Some(path)).is_some(),
                "expected syntax for {path}"
            );
        }

        // Depending on the syntect bundle version, some grammars may be absent.
        let optional_paths = ["file.swift", "file.kt", "file.tsx", "Cargo.toml"];
        for path in optional_paths {
            let _ = syntax_for_path(syntax_set, Some(path));
        }
    }

    #[test]
    fn resolves_json_as_json_family() {
        let syntax_set = syntax_set();
        let syntax = syntax_for_path(syntax_set, Some("payload.json")).expect("json syntax");
        let syntax_name = syntax.name.to_ascii_lowercase();
        assert!(
            syntax_name.contains("json") || syntax_name.contains("javascript"),
            "unexpected json syntax mapping: {}",
            syntax.name
        );
    }

    #[test]
    fn resolves_terraform_family_when_available() {
        let syntax_set = syntax_set();
        let dependency_supports_terraform = syntax_set.find_syntax_by_extension("tf").is_some()
            || syntax_set.find_syntax_by_extension("tfvars").is_some()
            || ["terraform", "tf", "hcl"]
                .iter()
                .any(|token| syntax_set.find_syntax_by_token(token).is_some());

        if !dependency_supports_terraform {
            return;
        }

        let terraform_paths = ["main.tf", "vars.tfvars", "config.hcl"];
        for path in terraform_paths {
            assert!(
                syntax_for_path(syntax_set, Some(path)).is_some(),
                "expected terraform-family syntax for {path}"
            );
        }
    }
}
