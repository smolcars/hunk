#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeSyntaxColorToken {
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

impl From<crate::app::highlight::SyntaxTokenKind> for CodeSyntaxColorToken {
    fn from(value: crate::app::highlight::SyntaxTokenKind) -> Self {
        match value {
            crate::app::highlight::SyntaxTokenKind::Plain => Self::Plain,
            crate::app::highlight::SyntaxTokenKind::Keyword => Self::Keyword,
            crate::app::highlight::SyntaxTokenKind::String => Self::String,
            crate::app::highlight::SyntaxTokenKind::Number => Self::Number,
            crate::app::highlight::SyntaxTokenKind::Comment => Self::Comment,
            crate::app::highlight::SyntaxTokenKind::Function => Self::Function,
            crate::app::highlight::SyntaxTokenKind::TypeName => Self::TypeName,
            crate::app::highlight::SyntaxTokenKind::Constant => Self::Constant,
            crate::app::highlight::SyntaxTokenKind::Variable => Self::Variable,
            crate::app::highlight::SyntaxTokenKind::Operator => Self::Operator,
        }
    }
}

impl From<hunk_domain::markdown_preview::MarkdownCodeTokenKind> for CodeSyntaxColorToken {
    fn from(value: hunk_domain::markdown_preview::MarkdownCodeTokenKind) -> Self {
        match value {
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Plain => Self::Plain,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Keyword => Self::Keyword,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::String => Self::String,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Number => Self::Number,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Comment => Self::Comment,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Function => Self::Function,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::TypeName => Self::TypeName,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Constant => Self::Constant,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Variable => Self::Variable,
            hunk_domain::markdown_preview::MarkdownCodeTokenKind::Operator => Self::Operator,
        }
    }
}

fn code_syntax_color(
    theme: &gpui_component::Theme,
    default_color: gpui::Hsla,
    token: impl Into<CodeSyntaxColorToken>,
) -> gpui::Hsla {
    let palette = crate::app::theme::hunk_editor_syntax_colors(theme, theme.mode.is_dark());
    match token.into() {
        CodeSyntaxColorToken::Plain => default_color,
        CodeSyntaxColorToken::Keyword => palette.keyword,
        CodeSyntaxColorToken::String => palette.string,
        CodeSyntaxColorToken::Number => palette.number,
        CodeSyntaxColorToken::Comment => palette.comment,
        CodeSyntaxColorToken::Function => palette.function,
        CodeSyntaxColorToken::TypeName => palette.type_name,
        CodeSyntaxColorToken::Constant => palette.constant,
        CodeSyntaxColorToken::Variable => palette.variable,
        CodeSyntaxColorToken::Operator => palette.operator,
    }
}

#[allow(dead_code)]
pub(crate) fn diff_syntax_color(
    theme: &gpui_component::Theme,
    default_color: gpui::Hsla,
    token: crate::app::highlight::SyntaxTokenKind,
) -> gpui::Hsla {
    code_syntax_color(theme, default_color, token)
}

pub(crate) fn markdown_syntax_color(
    theme: &gpui_component::Theme,
    default_color: gpui::Hsla,
    token: hunk_domain::markdown_preview::MarkdownCodeTokenKind,
) -> gpui::Hsla {
    code_syntax_color(theme, default_color, token)
}
