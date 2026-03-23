use hunk_domain::diff::DiffCellKind;
use hunk_language::preview_highlight_spans_for_path;

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

impl From<hunk_language::PreviewSyntaxToken> for SyntaxTokenKind {
    fn from(value: hunk_language::PreviewSyntaxToken) -> Self {
        match value {
            hunk_language::PreviewSyntaxToken::Plain => Self::Plain,
            hunk_language::PreviewSyntaxToken::Keyword => Self::Keyword,
            hunk_language::PreviewSyntaxToken::String => Self::String,
            hunk_language::PreviewSyntaxToken::Number => Self::Number,
            hunk_language::PreviewSyntaxToken::Comment => Self::Comment,
            hunk_language::PreviewSyntaxToken::Function => Self::Function,
            hunk_language::PreviewSyntaxToken::TypeName => Self::TypeName,
            hunk_language::PreviewSyntaxToken::Constant => Self::Constant,
            hunk_language::PreviewSyntaxToken::Variable => Self::Variable,
            hunk_language::PreviewSyntaxToken::Operator => Self::Operator,
        }
    }
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
    apply_tree_sitter_syntax_map(file_path, line, &mut syntax_map);

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
    apply_tree_sitter_syntax_map(file_path, line, &mut syntax_map);
    let changed_map = vec![false; chars.len()];
    merge_styled_segments(&chars, &syntax_map, &changed_map)
}

#[cfg(test)]
#[allow(dead_code)]
pub(super) fn build_plain_line_segments(file_path: Option<&str>, line: &str) -> Vec<StyledSegment> {
    build_syntax_only_line_segments(file_path, line)
}

fn apply_tree_sitter_syntax_map(
    file_path: Option<&str>,
    line: &str,
    syntax_map: &mut [SyntaxTokenKind],
) {
    if line.is_empty() || syntax_map.is_empty() {
        return;
    }

    let char_offsets = char_byte_offsets(line);
    for span in preview_highlight_spans_for_path(file_path, line) {
        mark_byte_range(
            &char_offsets,
            syntax_map,
            span.byte_range.start,
            span.byte_range.end,
            span.token.into(),
        );
    }
}

fn char_byte_offsets(line: &str) -> Vec<usize> {
    line.char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(line.len()))
        .collect()
}

fn mark_byte_range(
    char_offsets: &[usize],
    syntax_map: &mut [SyntaxTokenKind],
    start_byte: usize,
    end_byte: usize,
    token: SyntaxTokenKind,
) {
    if start_byte >= end_byte {
        return;
    }

    for (index, kind) in syntax_map.iter_mut().enumerate() {
        let char_start = char_offsets[index];
        let char_end = char_offsets[index + 1];
        if char_end <= start_byte {
            continue;
        }
        if char_start >= end_byte {
            break;
        }
        *kind = token;
    }
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
    fn marks_typescript_keywords() {
        let segments = build_line_segments(
            Some("main.ts"),
            "const answer = parseBIP321(\"bitcoin:addr\");",
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
    fn marks_toml_strings_and_comments() {
        let segments = build_line_segments(
            Some("Cargo.toml"),
            "name = \"hunk\" # app name",
            DiffCellKind::Context,
            "",
            DiffCellKind::Context,
        );

        assert!(
            segments
                .iter()
                .any(|segment| segment.syntax == SyntaxTokenKind::String)
        );
        assert!(
            segments
                .iter()
                .any(|segment| segment.syntax == SyntaxTokenKind::Comment)
        );
    }
}
