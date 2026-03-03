mod parser;
mod side_by_side;
mod types;

pub use parser::parse_patch_document;
pub use side_by_side::parse_patch_side_by_side;
pub use types::{
    DiffCell, DiffCellKind, DiffDocument, DiffHunk, DiffLine, DiffLineKind, DiffRowKind,
    SideBySideRow,
};
