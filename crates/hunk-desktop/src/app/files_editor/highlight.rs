use gpui::{Font, Hsla, TextRun};
use helix_core::syntax::{HighlightEvent, Highlighter};
use helix_term::ui::EditorView;
use helix_view::graphics::Style as HelixStyle;
use helix_view::{Document, Editor, Theme};

use super::theme::color_to_hsla;

struct SyntaxHighlighter<'h, 'r, 't> {
    inner: Option<Highlighter<'h>>,
    text: helix_core::ropey::RopeSlice<'r>,
    pos: usize,
    theme: &'t Theme,
    text_style: HelixStyle,
    style: HelixStyle,
}

impl<'h, 'r, 't> SyntaxHighlighter<'h, 'r, 't> {
    fn new(
        inner: Option<Highlighter<'h>>,
        text: helix_core::ropey::RopeSlice<'r>,
        theme: &'t Theme,
        text_style: HelixStyle,
    ) -> Self {
        let mut highlighter = Self {
            inner,
            text,
            pos: 0,
            theme,
            style: text_style,
            text_style,
        };
        highlighter.update_pos();
        highlighter
    }

    fn update_pos(&mut self) {
        self.pos = self
            .inner
            .as_ref()
            .and_then(|highlighter| {
                let next_byte_idx = highlighter.next_event_offset();
                (next_byte_idx != u32::MAX).then(|| {
                    let bounded = (next_byte_idx as usize).min(self.text.len_bytes());
                    let mut char_idx = self.text.byte_to_char(bounded);
                    while char_idx < self.text.len_chars()
                        && self.text.char_to_byte(char_idx) < bounded
                    {
                        char_idx += 1;
                    }
                    char_idx
                })
            })
            .unwrap_or(usize::MAX);
    }

    fn advance(&mut self) {
        let Some(highlighter) = self.inner.as_mut() else {
            return;
        };

        let (event, highlights) = highlighter.advance();
        let base = match event {
            HighlightEvent::Refresh => self.text_style,
            HighlightEvent::Push => self.style,
        };

        self.style = highlights.fold(base, |acc, highlight| {
            acc.patch(self.theme.highlight(highlight))
        });
        self.update_pos();
    }
}

pub(super) fn syntax_runs(
    editor: &Editor,
    doc: &Document,
    anchor: usize,
    lines: u16,
    end_char: usize,
    default_foreground: Hsla,
    font: Font,
) -> Vec<TextRun> {
    let base_text_style = editor.theme.get("ui.text");
    let default_foreground = base_text_style
        .fg
        .and_then(color_to_hsla)
        .unwrap_or(default_foreground);
    let loader = editor.syn_loader.load();
    let mut styles = SyntaxHighlighter::new(
        EditorView::doc_syntax_highlighter(doc, anchor, lines, &loader),
        doc.text().slice(..),
        &editor.theme,
        base_text_style,
    );
    let mut position = anchor;
    let mut runs = Vec::new();

    while position < end_char {
        while position >= styles.pos {
            styles.advance();
        }

        let fg = styles
            .style
            .fg
            .and_then(color_to_hsla)
            .unwrap_or(default_foreground);
        let bg = styles.style.bg.and_then(color_to_hsla);
        let next_position = styles.pos.min(end_char);
        if next_position <= position {
            break;
        }
        let len = styles.text.char_to_byte(next_position) - styles.text.char_to_byte(position);

        runs.push(TextRun {
            len,
            font: font.clone(),
            color: fg,
            background_color: bg,
            underline: None,
            strikethrough: None,
        });
        position += len;
    }

    if runs.is_empty() {
        runs.push(TextRun {
            len: styles.text.char_to_byte(end_char) - styles.text.char_to_byte(anchor),
            font,
            color: default_foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    runs
}
