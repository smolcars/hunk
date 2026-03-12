use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write as _;
use std::path::Path;

use anyhow::{Result, anyhow};

pub(super) use super::data_segments::{
    cached_runtime_fallback_segments, compact_cached_segments_for_render, is_binary_patch,
    is_probably_binary_extension,
};
use super::highlight::{
    StyledSegment, SyntaxTokenKind, build_line_segments, build_syntax_only_line_segments,
    render_with_whitespace_markers,
};
pub(super) use super::workspace_view::{WorkspaceSwitchAction, WorkspaceViewMode};
use super::*;
use hunk_domain::diff::parse_patch_side_by_side;
use hunk_git::git::{RepoTreeEntry, RepoTreeEntryKind};

#[derive(Default)]
struct RepoTreeFolder {
    ignored: bool,
    folders: BTreeMap<String, RepoTreeFolder>,
    files: BTreeMap<String, RepoTreeFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RepoTreeFile {
    ignored: bool,
    status: Option<FileStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RepoTreeNodeKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RepoTreeNode {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) kind: RepoTreeNodeKind,
    pub(super) ignored: bool,
    pub(super) file_status: Option<FileStatus>,
    pub(super) children: Vec<RepoTreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RepoTreeRow {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) kind: RepoTreeNodeKind,
    pub(super) ignored: bool,
    pub(super) file_status: Option<FileStatus>,
    pub(super) depth: usize,
    pub(super) expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileEditorDocument {
    pub(super) text: String,
    pub(super) byte_len: usize,
    pub(super) language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CachedStyledSegment {
    pub(super) plain_text: SharedString,
    pub(super) whitespace_text: SharedString,
    pub(super) syntax: SyntaxTokenKind,
    pub(super) changed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DiffRowSegmentCache {
    pub(super) quality: DiffSegmentQuality,
    pub(super) left: Vec<CachedStyledSegment>,
    pub(super) right: Vec<CachedStyledSegment>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DiffSegmentQuality {
    #[default]
    Plain,
    SyntaxOnly,
    Detailed,
}

#[derive(Debug, Clone)]
pub(super) struct FileRowRange {
    pub(super) path: String,
    pub(super) status: FileStatus,
    pub(super) start_row: usize,
    pub(super) end_row: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DiffStreamRowKind {
    FileHeader,
    CoreCode,
    CoreHunkHeader,
    CoreMeta,
    CoreEmpty,
    FileLoading,
    FileCollapsed,
    FileError,
    EmptyState,
}

#[derive(Debug, Clone)]
pub(super) struct DiffStreamRowMeta {
    pub(super) stable_id: u64,
    pub(super) file_path: Option<String>,
    pub(super) file_status: Option<FileStatus>,
    pub(super) kind: DiffStreamRowKind,
}

pub(super) struct DiffStream {
    pub(super) rows: Vec<SideBySideRow>,
    pub(super) row_metadata: Vec<DiffStreamRowMeta>,
    pub(super) row_segments: Vec<Option<DiffRowSegmentCache>>,
    pub(super) file_ranges: Vec<FileRowRange>,
    pub(super) file_line_stats: BTreeMap<String, LineStats>,
}

struct LoadedFileDiffRows {
    core_rows: Vec<SideBySideRow>,
    stats: LineStats,
    load_error: Option<String>,
}

const DETAILED_SEGMENT_MAX_CHANGED_LINES: u64 = 8_000;
const MAX_RENDER_SEGMENTS_PER_CELL_DETAILED: usize = 48;
const MAX_RENDER_SEGMENTS_PER_CELL_LARGE_FILE: usize = 24;

pub(super) fn use_detailed_segments_for_file(line_stats: LineStats) -> bool {
    line_stats.changed() <= DETAILED_SEGMENT_MAX_CHANGED_LINES
}

pub(super) fn base_segment_quality_for_file(line_stats: LineStats) -> DiffSegmentQuality {
    if use_detailed_segments_for_file(line_stats) {
        DiffSegmentQuality::Detailed
    } else {
        DiffSegmentQuality::SyntaxOnly
    }
}

pub(super) fn effective_segment_quality(
    base_quality: DiffSegmentQuality,
    recently_scrolling: bool,
) -> DiffSegmentQuality {
    if !recently_scrolling {
        return base_quality;
    }

    match base_quality {
        DiffSegmentQuality::Detailed => DiffSegmentQuality::SyntaxOnly,
        DiffSegmentQuality::SyntaxOnly => DiffSegmentQuality::Plain,
        DiffSegmentQuality::Plain => DiffSegmentQuality::Plain,
    }
}

pub(super) fn build_repo_tree(entries: &[RepoTreeEntry]) -> Vec<RepoTreeNode> {
    let mut root = RepoTreeFolder::default();

    for entry in entries {
        let mut parts = entry.path.split('/').peekable();
        let mut cursor = &mut root;
        while let Some(part) = parts.next() {
            if parts.peek().is_some() {
                cursor = cursor.folders.entry(part.to_string()).or_default();
                continue;
            }

            match entry.kind {
                RepoTreeEntryKind::Directory => {
                    let folder = cursor.folders.entry(part.to_string()).or_default();
                    folder.ignored = entry.ignored;
                }
                RepoTreeEntryKind::File => {
                    cursor.files.insert(
                        part.to_string(),
                        RepoTreeFile {
                            ignored: entry.ignored,
                            status: None,
                        },
                    );
                }
            }
        }
    }

    build_repo_tree_nodes(&root, "")
}

pub(super) fn build_changed_files_tree(files: &[ChangedFile]) -> Vec<RepoTreeNode> {
    files
        .iter()
        .map(|file| RepoTreeNode {
            path: file.path.clone(),
            name: file.path.clone(),
            kind: RepoTreeNodeKind::File,
            ignored: false,
            file_status: Some(file.status),
            children: Vec::new(),
        })
        .collect()
}

pub(super) fn flatten_repo_tree_rows(
    nodes: &[RepoTreeNode],
    expanded_dirs: &BTreeSet<String>,
) -> Vec<RepoTreeRow> {
    let mut rows = Vec::new();
    append_repo_tree_rows(nodes, expanded_dirs, 0, &mut rows);
    rows
}

pub(super) fn count_repo_tree_kind(nodes: &[RepoTreeNode], kind: RepoTreeNodeKind) -> usize {
    nodes
        .iter()
        .map(|node| {
            let self_count = usize::from(node.kind == kind);
            self_count + count_repo_tree_kind(&node.children, kind)
        })
        .sum::<usize>()
}

pub(super) fn load_file_editor_document(
    repo_root: &Path,
    file_path: &str,
    max_bytes: usize,
) -> Result<FileEditorDocument> {
    let absolute_path = repo_root.join(file_path);
    let bytes = fs::read(&absolute_path)
        .map_err(|err| anyhow!("failed to read {}: {err}", absolute_path.display()))?;
    if bytes.len() > max_bytes {
        return Err(anyhow!(
            "file is too large to edit ({} bytes, max {})",
            bytes.len(),
            max_bytes
        ));
    }
    if is_probably_binary_bytes(&bytes) {
        return Err(anyhow!("binary file editing is not supported"));
    }

    let text = String::from_utf8(bytes)
        .map_err(|_| anyhow!("file is not UTF-8 text and cannot be edited"))?;

    Ok(FileEditorDocument {
        byte_len: text.len(),
        language: editor_language_hint(file_path),
        text,
    })
}

pub(super) fn save_file_editor_document(
    repo_root: &Path,
    file_path: &str,
    text: &str,
) -> Result<()> {
    let absolute_path = repo_root.join(file_path);
    let existing_permissions = fs::metadata(&absolute_path)
        .ok()
        .map(|meta| meta.permissions());
    let parent = absolute_path.parent().ok_or_else(|| {
        anyhow!(
            "cannot save {}: resolved path has no parent",
            absolute_path.display()
        )
    })?;

    let mut temp_name = absolute_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("hunk-save")
        .to_string();
    temp_name.push_str(".hunk-tmp.");
    temp_name.push_str(&std::process::id().to_string());
    temp_name.push('.');
    temp_name.push_str(
        &std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string(),
    );

    let temp_path = parent.join(temp_name);
    let mut temp_file =
        fs::File::create(&temp_path).map_err(|err| anyhow!("failed to create temp file: {err}"))?;
    temp_file
        .write_all(text.as_bytes())
        .map_err(|err| anyhow!("failed to write temp file {}: {err}", temp_path.display()))?;
    temp_file
        .sync_all()
        .map_err(|err| anyhow!("failed to fsync temp file {}: {err}", temp_path.display()))?;
    drop(temp_file);
    if let Some(permissions) = existing_permissions {
        fs::set_permissions(&temp_path, permissions).map_err(|err| {
            anyhow!(
                "failed to preserve permissions for temp file {}: {err}",
                temp_path.display()
            )
        })?;
    }

    if let Err(err) = fs::rename(&temp_path, &absolute_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(anyhow!(
            "failed to move {} into place: {err}",
            absolute_path.display()
        ));
    }

    if let Ok(dir_handle) = fs::File::open(parent) {
        let _ = dir_handle.sync_all();
    }

    Ok(())
}

fn join_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn build_repo_tree_nodes(folder: &RepoTreeFolder, prefix: &str) -> Vec<RepoTreeNode> {
    let mut nodes = Vec::new();

    for (name, child_folder) in &folder.folders {
        let path = join_path(prefix, name);
        nodes.push(RepoTreeNode {
            path: path.clone(),
            name: name.clone(),
            kind: RepoTreeNodeKind::Directory,
            ignored: child_folder.ignored,
            file_status: None,
            children: build_repo_tree_nodes(child_folder, &path),
        });
    }

    for (name, file) in &folder.files {
        let path = join_path(prefix, name);
        nodes.push(RepoTreeNode {
            path,
            name: name.clone(),
            kind: RepoTreeNodeKind::File,
            ignored: file.ignored,
            file_status: file.status,
            children: Vec::new(),
        });
    }

    nodes
}

fn append_repo_tree_rows(
    nodes: &[RepoTreeNode],
    expanded_dirs: &BTreeSet<String>,
    depth: usize,
    rows: &mut Vec<RepoTreeRow>,
) {
    for node in nodes {
        let expanded =
            node.kind == RepoTreeNodeKind::Directory && expanded_dirs.contains(node.path.as_str());
        rows.push(RepoTreeRow {
            path: node.path.clone(),
            name: node.name.clone(),
            kind: node.kind,
            ignored: node.ignored,
            file_status: node.file_status,
            depth,
            expanded,
        });

        if expanded && node.kind == RepoTreeNodeKind::Directory {
            append_repo_tree_rows(&node.children, expanded_dirs, depth + 1, rows);
        }
    }
}

fn is_probably_binary_bytes(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

fn editor_language_hint(file_path: &str) -> String {
    let path = Path::new(file_path);

    if let Some(name) = path.file_name().and_then(|file| file.to_str()) {
        match name {
            "Dockerfile" => return "text".to_string(),
            "Makefile" => return "make".to_string(),
            "CMakeLists.txt" => return "cmake".to_string(),
            ".zshrc" | ".bashrc" | ".bash_profile" => return "bash".to_string(),
            "Cargo.toml" | "Cargo.lock" => return "toml".to_string(),
            _ => {}
        }
    }

    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        let extension = extension.to_ascii_lowercase();
        let language = match extension.as_str() {
            "rs" => "rust",
            "toml" => "toml",
            "js" | "mjs" | "cjs" | "jsx" => "javascript",
            "tsx" => "tsx",
            "ts" => "typescript",
            "json" | "jsonc" => "json",
            "yaml" | "yml" => "yaml",
            "md" | "markdown" | "mdx" => "markdown",
            "py" => "python",
            "rb" => "ruby",
            "go" => "go",
            "java" => "java",
            "swift" => "swift",
            "c" | "h" => "c",
            "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => "cpp",
            "cs" => "csharp",
            "cmake" => "cmake",
            "graphql" | "gql" => "graphql",
            "bash" | "sh" | "zsh" => "bash",
            "html" | "htm" => "html",
            "css" | "scss" | "sass" => "css",
            "ejs" => "ejs",
            "erb" => "erb",
            "ex" | "exs" => "elixir",
            "sql" => "sql",
            "proto" => "proto",
            "scala" => "scala",
            "zig" => "zig",
            "diff" | "patch" => "diff",
            "lock" => "toml",
            _ => "text",
        };
        return language.to_string();
    }

    "text".to_string()
}

pub(super) fn is_markdown_path(file_path: &str) -> bool {
    editor_language_hint(file_path) == "markdown"
}

pub(super) fn message_row(kind: DiffRowKind, text: impl Into<String>) -> SideBySideRow {
    SideBySideRow {
        kind,
        left: DiffCell {
            line: None,
            text: String::new(),
            kind: DiffCellKind::None,
        },
        right: DiffCell {
            line: None,
            text: String::new(),
            kind: DiffCellKind::None,
        },
        text: text.into(),
    }
}

pub(super) fn build_diff_stream_from_patch_map(
    files: &[ChangedFile],
    collapsed_files: &BTreeSet<String>,
    previous_file_line_stats: &BTreeMap<String, LineStats>,
    patches_by_path: &BTreeMap<String, String>,
    loading_paths: &BTreeSet<String>,
) -> DiffStream {
    let mut rows = Vec::new();
    let mut row_metadata = Vec::new();
    let mut row_segments = Vec::new();
    let mut file_ranges = Vec::with_capacity(files.len());
    let mut file_line_stats = BTreeMap::new();

    for file in files {
        let start_row = rows.len();
        let mut file_row_ordinal = 0_usize;
        push_stream_row(
            &mut rows,
            &mut row_metadata,
            message_row(DiffRowKind::Meta, file.path.clone()),
            DiffStreamRowKind::FileHeader,
            Some(file.path.as_str()),
            Some(file.status),
            file_row_ordinal,
        );
        row_segments.push(None);
        file_row_ordinal = file_row_ordinal.saturating_add(1);

        if collapsed_files.contains(file.path.as_str()) {
            let collapsed_stats = previous_file_line_stats
                .get(file.path.as_str())
                .copied()
                .unwrap_or_default();
            file_line_stats.insert(file.path.clone(), collapsed_stats);
            let collapsed_message = if collapsed_stats.changed() > 0 {
                format!(
                    "File collapsed ({} changed lines hidden, counts may be stale). Expand to refresh.",
                    collapsed_stats.changed()
                )
            } else {
                "File collapsed. Expand to load and refresh its diff.".to_string()
            };
            push_stream_row(
                &mut rows,
                &mut row_metadata,
                message_row(DiffRowKind::Empty, collapsed_message),
                DiffStreamRowKind::FileCollapsed,
                Some(file.path.as_str()),
                Some(file.status),
                file_row_ordinal,
            );
            row_segments.push(None);
        } else if loading_paths.contains(file.path.as_str()) {
            let loading_stats = previous_file_line_stats
                .get(file.path.as_str())
                .copied()
                .unwrap_or_default();
            file_line_stats.insert(file.path.clone(), loading_stats);
            push_stream_row(
                &mut rows,
                &mut row_metadata,
                message_row(DiffRowKind::Meta, "Loading file diff..."),
                DiffStreamRowKind::FileLoading,
                Some(file.path.as_str()),
                Some(file.status),
                file_row_ordinal,
            );
            row_segments.push(None);
        } else {
            let patch = patches_by_path
                .get(file.path.as_str())
                .map(String::as_str)
                .unwrap_or_default();
            let loaded_file = load_file_diff_rows(file, patch);
            file_line_stats.insert(file.path.clone(), loaded_file.stats);
            if let Some(load_error) = loaded_file.load_error {
                push_stream_row(
                    &mut rows,
                    &mut row_metadata,
                    message_row(DiffRowKind::Meta, load_error),
                    DiffStreamRowKind::FileError,
                    Some(file.path.as_str()),
                    Some(file.status),
                    file_row_ordinal,
                );
                row_segments.push(None);
            } else {
                for row in loaded_file.core_rows.into_iter().filter(|row| {
                    matches!(
                        row.kind,
                        DiffRowKind::Code | DiffRowKind::HunkHeader | DiffRowKind::Empty
                    )
                }) {
                    let row_kind = stream_kind_for_core_row(&row);
                    push_stream_row(
                        &mut rows,
                        &mut row_metadata,
                        row,
                        row_kind,
                        Some(file.path.as_str()),
                        Some(file.status),
                        file_row_ordinal,
                    );
                    row_segments.push(None);
                    file_row_ordinal = file_row_ordinal.saturating_add(1);
                }
            }
        }

        let end_row = rows.len();
        file_ranges.push(FileRowRange {
            path: file.path.clone(),
            status: file.status,
            start_row,
            end_row,
        });
    }

    if rows.is_empty() {
        push_stream_row(
            &mut rows,
            &mut row_metadata,
            message_row(DiffRowKind::Empty, "No changed files."),
            DiffStreamRowKind::EmptyState,
            None,
            None,
            0,
        );
        row_segments.push(None);
    }

    debug_assert_eq!(row_segments.len(), rows.len());

    DiffStream {
        rows,
        row_metadata,
        row_segments,
        file_ranges,
        file_line_stats,
    }
}

fn load_file_diff_rows(file: &ChangedFile, patch: &str) -> LoadedFileDiffRows {
    if is_probably_binary_extension(file.path.as_str()) {
        return LoadedFileDiffRows {
            core_rows: Vec::new(),
            stats: LineStats::default(),
            load_error: Some(format!(
                "Preview unavailable for {}: binary file type.",
                file.path
            )),
        };
    }

    if is_binary_patch(patch) {
        return LoadedFileDiffRows {
            core_rows: Vec::new(),
            stats: LineStats::default(),
            load_error: Some(format!(
                "Preview unavailable for {}: binary diff.",
                file.path
            )),
        };
    }

    let core_rows = parse_patch_side_by_side(patch);
    let stats = line_stats_from_rows(&core_rows);
    LoadedFileDiffRows {
        core_rows,
        stats,
        load_error: None,
    }
}

pub(super) fn cached_segments_from_styled(
    segments: Vec<StyledSegment>,
) -> Vec<CachedStyledSegment> {
    segments
        .into_iter()
        .map(|segment| {
            let plain_text = SharedString::from(segment.text);
            let whitespace_text =
                SharedString::from(render_with_whitespace_markers(plain_text.as_ref()));
            CachedStyledSegment {
                plain_text,
                whitespace_text,
                syntax: segment.syntax,
                changed: segment.changed,
            }
        })
        .collect::<Vec<_>>()
}

pub(super) fn build_diff_row_segment_cache_from_cells(
    file_path: Option<&str>,
    left_text: &str,
    left_kind: DiffCellKind,
    right_text: &str,
    right_kind: DiffCellKind,
    quality: DiffSegmentQuality,
) -> DiffRowSegmentCache {
    match quality {
        DiffSegmentQuality::Detailed => {
            let left = compact_cached_segments_for_render(
                cached_segments_from_styled(build_line_segments(
                    file_path, left_text, left_kind, right_text, right_kind,
                )),
                MAX_RENDER_SEGMENTS_PER_CELL_DETAILED,
            );
            let right = compact_cached_segments_for_render(
                cached_segments_from_styled(build_line_segments(
                    file_path, right_text, right_kind, left_text, left_kind,
                )),
                MAX_RENDER_SEGMENTS_PER_CELL_DETAILED,
            );

            DiffRowSegmentCache {
                quality,
                left,
                right,
            }
        }
        DiffSegmentQuality::SyntaxOnly => {
            let left = compact_cached_segments_for_render(
                cached_segments_from_styled(build_syntax_only_line_segments(file_path, left_text)),
                MAX_RENDER_SEGMENTS_PER_CELL_LARGE_FILE,
            );
            let right = compact_cached_segments_for_render(
                cached_segments_from_styled(build_syntax_only_line_segments(file_path, right_text)),
                MAX_RENDER_SEGMENTS_PER_CELL_LARGE_FILE,
            );

            DiffRowSegmentCache {
                quality,
                left,
                right,
            }
        }
        DiffSegmentQuality::Plain => DiffRowSegmentCache {
            quality,
            left: cached_runtime_fallback_segments(left_text, true),
            right: cached_runtime_fallback_segments(right_text, true),
        },
    }
}

fn stream_kind_for_core_row(row: &SideBySideRow) -> DiffStreamRowKind {
    match row.kind {
        DiffRowKind::Code => DiffStreamRowKind::CoreCode,
        DiffRowKind::HunkHeader => DiffStreamRowKind::CoreHunkHeader,
        DiffRowKind::Meta => DiffStreamRowKind::CoreMeta,
        DiffRowKind::Empty => DiffStreamRowKind::CoreEmpty,
    }
}

fn push_stream_row(
    rows: &mut Vec<SideBySideRow>,
    row_metadata: &mut Vec<DiffStreamRowMeta>,
    row: SideBySideRow,
    kind: DiffStreamRowKind,
    file_path: Option<&str>,
    file_status: Option<FileStatus>,
    ordinal: usize,
) -> u64 {
    let stable_id = compute_stable_row_id(file_path, kind, ordinal);
    rows.push(row);
    row_metadata.push(DiffStreamRowMeta {
        stable_id,
        file_path: file_path.map(ToString::to_string),
        file_status,
        kind,
    });
    stable_id
}

fn compute_stable_row_id(file_path: Option<&str>, kind: DiffStreamRowKind, ordinal: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    file_path.unwrap_or("__stream__").hash(&mut hasher);
    stable_kind_tag(kind).hash(&mut hasher);
    ordinal.hash(&mut hasher);
    hasher.finish()
}

fn stable_kind_tag(kind: DiffStreamRowKind) -> &'static str {
    match kind {
        DiffStreamRowKind::FileHeader => "file-header",
        DiffStreamRowKind::CoreCode => "core-code",
        DiffStreamRowKind::CoreHunkHeader => "core-hunk-header",
        DiffStreamRowKind::CoreMeta => "core-meta",
        DiffStreamRowKind::CoreEmpty => "core-empty",
        DiffStreamRowKind::FileLoading => "file-loading",
        DiffStreamRowKind::FileCollapsed => "file-collapsed",
        DiffStreamRowKind::FileError => "file-error",
        DiffStreamRowKind::EmptyState => "empty-state",
    }
}

fn line_stats_from_rows(rows: &[SideBySideRow]) -> LineStats {
    let mut stats = LineStats::default();

    for row in rows {
        if row.kind != DiffRowKind::Code {
            continue;
        }

        if row.left.kind == DiffCellKind::Removed {
            stats.removed = stats.removed.saturating_add(1);
        }
        if row.right.kind == DiffCellKind::Added {
            stats.added = stats.added.saturating_add(1);
        }
    }

    stats
}

pub(super) fn decimal_digits(value: u32) -> u32 {
    if value == 0 { 1 } else { value.ilog10() + 1 }
}

pub(super) fn line_number_column_width(digits: u32) -> f32 {
    digits as f32 * DIFF_MONO_CHAR_WIDTH + DIFF_LINE_NUMBER_EXTRA_PADDING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_row_id_is_deterministic_for_same_row() {
        let first = compute_stable_row_id(Some("src/lib.rs"), DiffStreamRowKind::CoreCode, 2);
        let second = compute_stable_row_id(Some("src/lib.rs"), DiffStreamRowKind::CoreCode, 2);

        assert_eq!(first, second);
    }

    #[test]
    fn stable_row_id_changes_when_ordinal_changes() {
        let first = compute_stable_row_id(Some("src/lib.rs"), DiffStreamRowKind::CoreMeta, 0);
        let second = compute_stable_row_id(Some("src/lib.rs"), DiffStreamRowKind::CoreMeta, 1);

        assert_ne!(first, second);
    }

    #[test]
    fn editor_language_hint_maps_rust_and_ts() {
        assert_eq!(editor_language_hint("src/main.rs"), "rust");
        assert_eq!(editor_language_hint("web/app.ts"), "typescript");
        assert_eq!(editor_language_hint("web/app.tsx"), "tsx");
        assert_eq!(editor_language_hint("web/app.jsx"), "javascript");
    }

    #[test]
    fn editor_language_hint_uses_filename_for_special_cases() {
        assert_eq!(editor_language_hint("Dockerfile"), "text");
        assert_eq!(editor_language_hint("Cargo.lock"), "toml");
        assert_eq!(editor_language_hint("CMakeLists.txt"), "cmake");
    }

    #[test]
    fn editor_language_hint_falls_back_to_text_for_unknown_extensions() {
        assert_eq!(editor_language_hint("docs/schema.xml"), "text");
    }

    #[test]
    fn compact_cached_segments_caps_count_and_preserves_text() {
        let mut expected = String::new();
        let styled = (0..20)
            .map(|ix| {
                let text = format!("part-{ix}|");
                expected.push_str(&text);
                StyledSegment {
                    text,
                    syntax: if ix % 2 == 0 {
                        SyntaxTokenKind::Keyword
                    } else {
                        SyntaxTokenKind::String
                    },
                    changed: ix % 3 == 0,
                }
            })
            .collect::<Vec<_>>();
        let cached = cached_segments_from_styled(styled);

        let compacted = compact_cached_segments_for_render(cached, 6);
        assert!(compacted.len() <= 6);

        let reconstructed = compacted.iter().fold(String::new(), |mut acc, segment| {
            acc.push_str(segment.plain_text.as_ref());
            acc
        });
        assert_eq!(reconstructed, expected);
    }

    #[test]
    fn large_file_segment_mode_keeps_syntax_without_changed_pair_lcs() {
        let row = SideBySideRow {
            kind: DiffRowKind::Code,
            left: DiffCell {
                line: Some(1),
                text: "name = \"hunk\" # app".to_string(),
                kind: DiffCellKind::Added,
            },
            right: DiffCell {
                line: Some(1),
                text: "name = \"hunk\" # app".to_string(),
                kind: DiffCellKind::Added,
            },
            text: String::new(),
        };

        let cache = build_diff_row_segment_cache_from_cells(
            Some("Cargo.toml"),
            &row.left.text,
            row.left.kind,
            &row.right.text,
            row.right.kind,
            DiffSegmentQuality::SyntaxOnly,
        );
        assert!(
            cache
                .left
                .iter()
                .any(|segment| segment.syntax != SyntaxTokenKind::Plain),
            "expected syntax-only large-file mode to keep non-plain tokens"
        );
        assert_eq!(cache.quality, DiffSegmentQuality::SyntaxOnly);
        assert!(cache.left.iter().all(|segment| !segment.changed));
        assert!(cache.right.iter().all(|segment| !segment.changed));
    }

    #[test]
    fn changed_files_tree_is_flat_and_uses_full_paths() {
        let files = vec![
            ChangedFile {
                path: "src/main.rs".to_string(),
                status: FileStatus::Modified,
                staged: false,
                unstaged: true,
                untracked: false,
            },
            ChangedFile {
                path: "README.md".to_string(),
                status: FileStatus::Untracked,
                staged: false,
                unstaged: true,
                untracked: true,
            },
        ];

        let nodes = build_changed_files_tree(&files);
        assert_eq!(nodes.len(), 2);
        assert!(
            nodes
                .iter()
                .all(|node| node.kind == RepoTreeNodeKind::File && node.children.is_empty())
        );
        assert_eq!(nodes[0].name, "src/main.rs");
        assert_eq!(nodes[0].file_status, Some(FileStatus::Modified));
        assert_eq!(nodes[1].name, "README.md");
        assert_eq!(nodes[1].file_status, Some(FileStatus::Untracked));
    }
}
