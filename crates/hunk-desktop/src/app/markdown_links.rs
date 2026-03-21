use std::ops::Range;
use std::path::{Component, Path, PathBuf};

#[cfg(not(test))]
use anyhow::Context as _;
use hunk_domain::markdown_preview::MarkdownInlineSpan;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkdownLinkRange {
    pub range: Range<usize>,
    pub raw_target: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkdownWorkspaceFileLink {
    pub raw_target: String,
    pub normalized_path: String,
    pub line: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MarkdownLinkTarget {
    ExternalUrl(String),
    WorkspaceFile(MarkdownWorkspaceFileLink),
}

pub(crate) fn markdown_inline_text_and_link_ranges(
    spans: &[MarkdownInlineSpan],
) -> (String, Vec<MarkdownLinkRange>) {
    let mut text = String::new();
    let mut link_ranges = Vec::new();
    let mut cursor = 0usize;

    for span in spans {
        if span.style.hard_break {
            if !text.ends_with('\n') {
                text.push('\n');
                cursor += 1;
            }
            continue;
        }
        if span.text.is_empty() {
            continue;
        }

        let start = cursor;
        text.push_str(span.text.as_str());
        cursor += span.text.len();

        let Some(raw_target) = span.style.link.as_ref() else {
            continue;
        };
        push_markdown_link_range(&mut link_ranges, start..cursor, raw_target);
    }

    (text, link_ranges)
}

pub(crate) fn resolve_markdown_link_target(
    raw_target: &str,
    workspace_root: Option<&Path>,
    current_document_path: Option<&str>,
) -> Option<MarkdownLinkTarget> {
    let trimmed = raw_target.trim();
    if trimmed.is_empty() {
        return None;
    }

    if is_external_markdown_url(trimmed) {
        return Some(MarkdownLinkTarget::ExternalUrl(trimmed.to_string()));
    }

    let workspace_root = workspace_root?;
    let (path_part, line) = split_markdown_file_target(trimmed);
    let normalized_path = if Path::new(path_part).is_absolute() {
        normalize_absolute_workspace_path(Path::new(path_part), workspace_root)?
    } else {
        normalize_workspace_relative_path(path_part, current_document_path)?
    };

    workspace_root
        .join(normalized_path.as_str())
        .is_file()
        .then_some(MarkdownLinkTarget::WorkspaceFile(
            MarkdownWorkspaceFileLink {
                raw_target: trimmed.to_string(),
                normalized_path,
                line,
            },
        ))
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn open_url_in_browser(url: &str) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        let _ = url;
        Ok(())
    }

    #[cfg(not(test))]
    {
        super::url_open::open_url_in_browser(url).context("failed to open browser URL")
    }
}

fn push_markdown_link_range(
    link_ranges: &mut Vec<MarkdownLinkRange>,
    range: Range<usize>,
    raw_target: &str,
) {
    if let Some(previous) = link_ranges.last_mut()
        && previous.raw_target == raw_target
        && previous.range.end == range.start
    {
        previous.range.end = range.end;
        return;
    }

    link_ranges.push(MarkdownLinkRange {
        range,
        raw_target: raw_target.to_string(),
    });
}

fn is_external_markdown_url(raw_target: &str) -> bool {
    let normalized = raw_target.trim().to_ascii_lowercase();
    normalized.starts_with("http://")
        || normalized.starts_with("https://")
        || normalized.starts_with("mailto:")
}

fn split_markdown_file_target(raw_target: &str) -> (&str, Option<usize>) {
    let trimmed = raw_target.trim();
    if let Some((path, fragment)) = trimmed.rsplit_once('#')
        && let Some(line) = parse_markdown_line_fragment(fragment)
    {
        return (path, Some(line));
    }
    if let Some((path, line)) = parse_colon_line_suffix(trimmed) {
        return (path, Some(line));
    }
    (trimmed, None)
}

fn parse_markdown_line_fragment(fragment: &str) -> Option<usize> {
    let normalized = fragment.trim();
    let line_fragment = normalized
        .strip_prefix('L')
        .or_else(|| normalized.strip_prefix('l'))?;
    let digits = line_fragment
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok().filter(|line| *line > 0)
}

fn parse_colon_line_suffix(raw_target: &str) -> Option<(&str, usize)> {
    let (path, suffix) = raw_target.rsplit_once(':')?;
    let line = suffix
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|line| *line > 0)?;
    if path.is_empty() {
        return None;
    }
    Some((path, line))
}

fn normalize_absolute_workspace_path(path: &Path, workspace_root: &Path) -> Option<String> {
    let canonical_root =
        std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    canonical_path
        .strip_prefix(canonical_root.as_path())
        .ok()
        .or_else(|| path.strip_prefix(workspace_root).ok())
        .and_then(pathbuf_to_workspace_relative)
}

fn normalize_workspace_relative_path(
    path: &str,
    current_document_path: Option<&str>,
) -> Option<String> {
    let candidate = if let Some(current_document_path) = current_document_path {
        Path::new(current_document_path)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(path)
    } else {
        PathBuf::from(path)
    };

    pathbuf_to_workspace_relative(
        normalize_workspace_relative_pathbuf(candidate.as_path())?.as_path(),
    )
}

fn normalize_workspace_relative_pathbuf(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }

    if path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn pathbuf_to_workspace_relative(path: &Path) -> Option<String> {
    let normalized = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    (!normalized.is_empty()).then_some(normalized)
}
