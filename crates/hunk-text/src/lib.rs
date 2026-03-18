use std::error::Error;
use std::fmt;
use std::ops::Range;

use ropey::Rope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferId(u64);

impl BufferId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextPosition {
    pub line: usize,
    pub column: usize,
}

impl TextPosition {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextRange {
    pub start: TextPosition,
    pub end: TextPosition,
}

impl TextRange {
    pub fn new(start: TextPosition, end: TextPosition) -> Self {
        if start <= end {
            Self { start, end }
        } else {
            Self {
                start: end,
                end: start,
            }
        }
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Selection {
    pub anchor: TextPosition,
    pub head: TextPosition,
}

impl Selection {
    pub const fn caret(position: TextPosition) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    pub const fn new(anchor: TextPosition, head: TextPosition) -> Self {
        Self { anchor, head }
    }

    pub fn range(self) -> TextRange {
        TextRange::new(self.anchor, self.head)
    }

    pub fn is_caret(self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorBias {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub byte: usize,
    pub bias: AnchorBias,
}

impl Anchor {
    pub const fn new(byte: usize, bias: AnchorBias) -> Self {
        Self { byte, bias }
    }

    pub fn apply_transaction(self, transaction: &Transaction) -> Self {
        let mut mapped = self.byte;
        for edit in transaction.edits() {
            let removed_len = edit.range.end - edit.range.start;
            let inserted_len = edit.text.len();

            if mapped < edit.range.start {
                continue;
            }

            if mapped > edit.range.end {
                mapped =
                    ((mapped as isize) + inserted_len as isize - removed_len as isize) as usize;
                continue;
            }

            if mapped == edit.range.start && mapped == edit.range.end {
                mapped = match self.bias {
                    AnchorBias::Left => edit.range.start,
                    AnchorBias::Right => edit.range.start + inserted_len,
                };
                continue;
            }

            mapped = match self.bias {
                AnchorBias::Left => edit.range.start,
                AnchorBias::Right => edit.range.start + inserted_len,
            };
        }

        Self {
            byte: mapped,
            bias: self.bias,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSnapshot {
    pub buffer_id: BufferId,
    pub version: u64,
    rope: Rope,
}

impl TextSnapshot {
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn byte_len(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn slice(&self, byte_range: Range<usize>) -> Result<String, TextError> {
        validate_byte_boundary(&self.rope, byte_range.start)?;
        validate_byte_boundary(&self.rope, byte_range.end)?;
        Ok(self
            .rope
            .slice(self.rope.byte_to_char(byte_range.start)..self.rope.byte_to_char(byte_range.end))
            .to_string())
    }

    pub fn position_to_byte(&self, position: TextPosition) -> Result<usize, TextError> {
        if position.line >= self.line_count() {
            return Err(TextError::LineOutOfBounds {
                line: position.line,
                max: self.line_count().saturating_sub(1),
            });
        }

        let line_start = self.rope.line_to_char(position.line);
        let line = self.rope.line(position.line);
        if position.column > line.len_chars() {
            return Err(TextError::ColumnOutOfBounds {
                line: position.line,
                column: position.column,
                max: line.len_chars(),
            });
        }

        Ok(self.rope.char_to_byte(line_start + position.column))
    }

    pub fn byte_to_position(&self, byte: usize) -> Result<TextPosition, TextError> {
        validate_byte_boundary(&self.rope, byte)?;
        let char_index = self.rope.byte_to_char(byte);
        let line = self.rope.char_to_line(char_index);
        let line_start = self.rope.line_to_char(line);
        Ok(TextPosition::new(line, char_index - line_start))
    }

    pub fn line_to_byte(&self, line: usize) -> Result<usize, TextError> {
        if line >= self.line_count() {
            return Err(TextError::LineOutOfBounds {
                line,
                max: self.line_count().saturating_sub(1),
            });
        }
        Ok(self.rope.char_to_byte(self.rope.line_to_char(line)))
    }

    pub fn anchor_before(&self, byte: usize) -> Result<Anchor, TextError> {
        validate_byte_boundary(&self.rope, byte)?;
        Ok(Anchor::new(byte, AnchorBias::Left))
    }

    pub fn anchor_after(&self, byte: usize) -> Result<Anchor, TextError> {
        validate_byte_boundary(&self.rope, byte)?;
        Ok(Anchor::new(byte, AnchorBias::Right))
    }

    pub fn find_next(
        &self,
        needle: &str,
        start_byte: usize,
    ) -> Result<Option<SearchMatch>, TextError> {
        if needle.is_empty() {
            return Ok(None);
        }
        validate_byte_boundary(&self.rope, start_byte)?;
        let text = self.text();
        Ok(text[start_byte..].find(needle).map(|offset| SearchMatch {
            byte_range: start_byte + offset..start_byte + offset + needle.len(),
        }))
    }

    pub fn find_all(&self, needle: &str) -> Vec<SearchMatch> {
        if needle.is_empty() {
            return Vec::new();
        }

        let text = self.text();
        let mut start = 0;
        let mut matches = Vec::new();
        while let Some(offset) = text[start..].find(needle) {
            let match_start = start + offset;
            let match_end = match_start + needle.len();
            matches.push(SearchMatch {
                byte_range: match_start..match_end,
            });
            start = match_end;
        }
        matches
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    pub fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Transaction {
    edits: Vec<Edit>,
}

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace(mut self, range: Range<usize>, text: impl Into<String>) -> Self {
        self.edits.push(Edit::replace(range, text));
        self
    }

    pub fn push_replace(&mut self, range: Range<usize>, text: impl Into<String>) {
        self.edits.push(Edit::replace(range, text));
    }

    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn sorted_edits(&self) -> Result<Vec<Edit>, TextError> {
        let mut edits = self.edits.clone();
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let mut previous_end = None;
        for edit in &edits {
            if edit.range.start > edit.range.end {
                return Err(TextError::InvalidRange {
                    start: edit.range.start,
                    end: edit.range.end,
                });
            }
            if let Some(end) = previous_end
                && edit.range.start < end
            {
                return Err(TextError::OverlappingEdits);
            }
            previous_end = Some(edit.range.end);
        }
        Ok(edits)
    }
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    forward: Transaction,
    inverse: Transaction,
}

#[derive(Debug, Clone)]
pub struct TextBuffer {
    id: BufferId,
    rope: Rope,
    version: u64,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
}

impl TextBuffer {
    pub fn new(id: BufferId, text: &str) -> Self {
        Self {
            id,
            rope: Rope::from_str(text),
            version: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub const fn id(&self) -> BufferId {
        self.id
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn byte_len(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn snapshot(&self) -> TextSnapshot {
        TextSnapshot {
            buffer_id: self.id,
            version: self.version,
            rope: self.rope.clone(),
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.rope = Rope::from_str(text);
        self.version = self.version.saturating_add(1);
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn apply_transaction(&mut self, transaction: Transaction) -> Result<(), TextError> {
        if transaction.is_empty() {
            return Ok(());
        }

        let normalized = transaction.sorted_edits()?;
        validate_edits(&self.rope, &normalized)?;
        let inverse = inverse_transaction(&self.rope, &normalized)?;

        apply_edits(&mut self.rope, &normalized)?;
        self.version = self.version.saturating_add(1);
        self.undo_stack.push(HistoryEntry {
            forward: Transaction { edits: normalized },
            inverse,
        });
        self.redo_stack.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> Result<bool, TextError> {
        let Some(entry) = self.undo_stack.pop() else {
            return Ok(false);
        };
        apply_edits(&mut self.rope, &entry.inverse.sorted_edits()?)?;
        self.version = self.version.saturating_add(1);
        self.redo_stack.push(entry);
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, TextError> {
        let Some(entry) = self.redo_stack.pop() else {
            return Ok(false);
        };
        apply_edits(&mut self.rope, &entry.forward.sorted_edits()?)?;
        self.version = self.version.saturating_add(1);
        self.undo_stack.push(entry);
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    InvalidRange {
        start: usize,
        end: usize,
    },
    OverlappingEdits,
    ByteOutOfBounds {
        byte: usize,
        len: usize,
    },
    InvalidByteBoundary {
        byte: usize,
    },
    LineOutOfBounds {
        line: usize,
        max: usize,
    },
    ColumnOutOfBounds {
        line: usize,
        column: usize,
        max: usize,
    },
}

impl fmt::Display for TextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::InvalidRange { start, end } => {
                write!(
                    formatter,
                    "invalid byte range: start {start} is after end {end}"
                )
            }
            TextError::OverlappingEdits => {
                formatter.write_str("transaction contains overlapping edits")
            }
            TextError::ByteOutOfBounds { byte, len } => {
                write!(
                    formatter,
                    "byte offset {byte} is out of bounds for buffer of length {len}"
                )
            }
            TextError::InvalidByteBoundary { byte } => {
                write!(
                    formatter,
                    "byte offset {byte} does not fall on a UTF-8 boundary"
                )
            }
            TextError::LineOutOfBounds { line, max } => {
                write!(formatter, "line {line} is out of bounds; max line is {max}")
            }
            TextError::ColumnOutOfBounds { line, column, max } => {
                write!(
                    formatter,
                    "column {column} is out of bounds for line {line}; max column is {max}"
                )
            }
        }
    }
}

impl Error for TextError {}

fn validate_edits(rope: &Rope, edits: &[Edit]) -> Result<(), TextError> {
    for edit in edits {
        validate_byte_boundary(rope, edit.range.start)?;
        validate_byte_boundary(rope, edit.range.end)?;
    }
    Ok(())
}

fn validate_byte_boundary(rope: &Rope, byte: usize) -> Result<(), TextError> {
    if byte > rope.len_bytes() {
        return Err(TextError::ByteOutOfBounds {
            byte,
            len: rope.len_bytes(),
        });
    }

    let char_index = rope.byte_to_char(byte);
    if rope.char_to_byte(char_index) != byte {
        return Err(TextError::InvalidByteBoundary { byte });
    }
    Ok(())
}

fn apply_edits(rope: &mut Rope, edits: &[Edit]) -> Result<(), TextError> {
    for edit in edits.iter().rev() {
        let start = rope.byte_to_char(edit.range.start);
        let end = rope.byte_to_char(edit.range.end);
        rope.remove(start..end);
        rope.insert(start, &edit.text);
    }
    Ok(())
}

fn inverse_transaction(rope: &Rope, edits: &[Edit]) -> Result<Transaction, TextError> {
    let mut offset = 0isize;
    let mut inverse = Vec::with_capacity(edits.len());

    for edit in edits {
        let original_text = rope
            .slice(rope.byte_to_char(edit.range.start)..rope.byte_to_char(edit.range.end))
            .to_string();
        let start = ((edit.range.start as isize) + offset) as usize;
        let end = start + edit.text.len();
        inverse.push(Edit::replace(start..end, original_text));
        offset += edit.text.len() as isize - (edit.range.end - edit.range.start) as isize;
    }

    Ok(Transaction { edits: inverse })
}
