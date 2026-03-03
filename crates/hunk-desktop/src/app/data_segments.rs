use std::path::Path;

use gpui::SharedString;

use super::data::CachedStyledSegment;
use super::highlight::{SyntaxTokenKind, render_with_whitespace_markers};

pub(super) fn compact_cached_segments_for_render(
    segments: Vec<CachedStyledSegment>,
    max_segments: usize,
) -> Vec<CachedStyledSegment> {
    if max_segments == 0 || segments.len() <= max_segments {
        return segments;
    }

    let chunk_size = segments.len().div_ceil(max_segments);
    let mut compacted = Vec::with_capacity(max_segments);
    for chunk in segments.chunks(chunk_size) {
        if chunk.is_empty() {
            continue;
        }

        let plain_capacity = chunk
            .iter()
            .map(|segment| segment.plain_text.as_ref().len())
            .sum::<usize>();
        let whitespace_capacity = chunk
            .iter()
            .map(|segment| segment.whitespace_text.as_ref().len())
            .sum::<usize>();
        let mut plain_text = String::with_capacity(plain_capacity);
        let mut whitespace_text = String::with_capacity(whitespace_capacity);

        let first_syntax = chunk[0].syntax;
        let mut mixed_syntax = false;
        let mut changed = false;
        for segment in chunk {
            plain_text.push_str(segment.plain_text.as_ref());
            whitespace_text.push_str(segment.whitespace_text.as_ref());
            changed |= segment.changed;
            if segment.syntax != first_syntax {
                mixed_syntax = true;
            }
        }

        compacted.push(CachedStyledSegment {
            plain_text: SharedString::from(plain_text),
            whitespace_text: SharedString::from(whitespace_text),
            syntax: if mixed_syntax {
                SyntaxTokenKind::Plain
            } else {
                first_syntax
            },
            changed,
        });
    }

    compacted
}

pub(super) fn cached_runtime_fallback_segments(
    text: &str,
    include_whitespace_markers: bool,
) -> Vec<CachedStyledSegment> {
    if text.is_empty() {
        return Vec::new();
    }

    let plain_text = SharedString::from(text.to_string());
    let whitespace_text = if include_whitespace_markers {
        SharedString::from(render_with_whitespace_markers(text))
    } else {
        plain_text.clone()
    };

    vec![CachedStyledSegment {
        plain_text,
        whitespace_text,
        syntax: SyntaxTokenKind::Plain,
        changed: false,
    }]
}

pub(super) fn is_probably_binary_extension(path: &str) -> bool {
    let Some(extension) = Path::new(path).extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    let extension = extension.to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "7z" | "a"
            | "apk"
            | "bin"
            | "bmp"
            | "class"
            | "dll"
            | "dmg"
            | "doc"
            | "docx"
            | "ear"
            | "eot"
            | "exe"
            | "gif"
            | "gz"
            | "ico"
            | "jar"
            | "jpeg"
            | "jpg"
            | "lib"
            | "lockb"
            | "mov"
            | "mp3"
            | "mp4"
            | "o"
            | "obj"
            | "otf"
            | "pdf"
            | "png"
            | "pyc"
            | "so"
            | "tar"
            | "tif"
            | "tiff"
            | "ttf"
            | "war"
            | "wasm"
            | "webm"
            | "webp"
            | "woff"
            | "woff2"
            | "xls"
            | "xlsx"
            | "zip"
    )
}

pub(super) fn is_binary_patch(patch: &str) -> bool {
    patch.contains('\0')
        || patch.contains("\nGIT binary patch\n")
        || patch
            .lines()
            .any(|line| line.starts_with("Binary files ") && line.ends_with(" differ"))
}
