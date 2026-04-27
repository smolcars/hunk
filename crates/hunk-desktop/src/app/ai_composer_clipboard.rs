use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};

use super::ai_attachment_images::is_supported_ai_image_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiComposerClipboardAttachments {
    pub(crate) item_count: usize,
    pub(crate) paths: Vec<PathBuf>,
}

pub(crate) fn ai_composer_clipboard_attachments(
    clipboard: &ClipboardItem,
    pasted_image_dir: &Path,
) -> Result<Option<AiComposerClipboardAttachments>> {
    let mut item_count = 0;
    let mut paths = Vec::new();

    for entry in clipboard.entries() {
        match entry {
            ClipboardEntry::Image(image) => {
                item_count += 1;
                if let Some(path) = ai_composer_write_pasted_image(image, pasted_image_dir)? {
                    paths.push(path);
                }
            }
            ClipboardEntry::ExternalPaths(external_paths) => {
                let external_paths = external_paths.paths();
                item_count += external_paths.len();
                paths.extend(external_paths.iter().cloned());
            }
            ClipboardEntry::String(text) => {
                let string_paths = ai_composer_paths_from_text(text.text());
                item_count += string_paths.len();
                paths.extend(string_paths);
            }
        }
    }

    if item_count == 0 {
        return Ok(None);
    }

    Ok(Some(AiComposerClipboardAttachments { item_count, paths }))
}

pub(crate) fn ai_composer_pasted_image_dir() -> PathBuf {
    std::env::temp_dir()
        .join("hunk-ai-composer-pasted-images")
        .join(std::process::id().to_string())
}

fn ai_composer_write_pasted_image(
    image: &Image,
    pasted_image_dir: &Path,
) -> Result<Option<PathBuf>> {
    let Some(extension) = ai_composer_image_extension(image.format()) else {
        return Ok(None);
    };

    fs::create_dir_all(pasted_image_dir)?;

    let path = pasted_image_dir.join(format!("clipboard-{}.{}", image.id(), extension));
    if !path.is_file() {
        fs::write(path.as_path(), image.bytes())?;
    }

    Ok(Some(path))
}

fn ai_composer_image_extension(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Webp => Some("webp"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::Bmp => Some("bmp"),
        ImageFormat::Tiff => Some("tiff"),
        ImageFormat::Svg | ImageFormat::Ico | ImageFormat::Pnm => None,
    }
}

fn ai_composer_paths_from_text(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| ai_composer_path_from_text_line(line.trim()))
        .collect()
}

fn ai_composer_path_from_text_line(line: &str) -> Option<PathBuf> {
    if line.is_empty() {
        return None;
    }

    let path = if line.starts_with("file://") {
        ai_composer_path_from_file_url(line)?
    } else {
        let path = PathBuf::from(line);
        path.is_absolute().then_some(path)?
    };

    is_supported_ai_image_path(path.as_path()).then_some(path)
}

fn ai_composer_path_from_file_url(line: &str) -> Option<PathBuf> {
    let path = line
        .strip_prefix("file://localhost")
        .or_else(|| line.strip_prefix("file://"))?;
    path.starts_with('/')
        .then(|| percent_decode(path))
        .flatten()
}

fn percent_decode(value: &str) -> Option<PathBuf> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
            continue;
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8(decoded).ok().map(PathBuf::from)
}
