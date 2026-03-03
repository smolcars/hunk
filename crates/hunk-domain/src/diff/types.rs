#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRowKind {
    Code,
    HunkHeader,
    Meta,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffCellKind {
    None,
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct DiffCell {
    pub line: Option<u32>,
    pub text: String,
    pub kind: DiffCellKind,
}

impl DiffCell {
    pub(crate) fn empty() -> Self {
        Self {
            line: None,
            text: String::new(),
            kind: DiffCellKind::None,
        }
    }

    pub(crate) fn new(line: Option<u32>, text: impl Into<String>, kind: DiffCellKind) -> Self {
        Self {
            line,
            text: text.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SideBySideRow {
    pub kind: DiffRowKind,
    pub left: DiffCell,
    pub right: DiffCell,
    pub text: String,
}

impl SideBySideRow {
    pub(crate) fn meta(kind: DiffRowKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            left: DiffCell::empty(),
            right: DiffCell::empty(),
            text: text.into(),
        }
    }

    pub(crate) fn code(left: DiffCell, right: DiffCell) -> Self {
        Self {
            kind: DiffRowKind::Code,
            left,
            right,
            text: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
}

impl DiffLine {
    pub(crate) fn new(
        kind: DiffLineKind,
        old_line: Option<u32>,
        new_line: Option<u32>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            old_line,
            new_line,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: Option<u32>,
    pub new_start: Option<u32>,
    pub lines: Vec<DiffLine>,
    pub trailing_meta: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffDocument {
    pub prelude: Vec<String>,
    pub hunks: Vec<DiffHunk>,
    pub epilogue: Vec<String>,
}
