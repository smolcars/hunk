use hunk_domain::diff::{DiffLineKind, parse_patch_document};

const DEFAULT_DISPLAY_PATH: &str = "changes";
pub(crate) const AI_WORKSPACE_INLINE_DIFF_DEFAULT_MAX_FILES: usize = 4;
pub(crate) const AI_WORKSPACE_INLINE_DIFF_DEFAULT_MAX_HUNKS_PER_FILE: usize = 6;
pub(crate) const AI_WORKSPACE_INLINE_DIFF_DEFAULT_MAX_LINES_PER_HUNK: usize = 80;
pub(crate) const AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AiWorkspaceInlineDiffOptions {
    pub(crate) max_files: usize,
    pub(crate) max_hunks_per_file: usize,
    pub(crate) max_lines_per_hunk: usize,
}

impl Default for AiWorkspaceInlineDiffOptions {
    fn default() -> Self {
        Self {
            max_files: AI_WORKSPACE_INLINE_DIFF_DEFAULT_MAX_FILES,
            max_hunks_per_file: AI_WORKSPACE_INLINE_DIFF_DEFAULT_MAX_HUNKS_PER_FILE,
            max_lines_per_hunk: AI_WORKSPACE_INLINE_DIFF_DEFAULT_MAX_LINES_PER_HUNK,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceInlineDiffPresentationPolicy {
    pub(crate) collapsed_by_default: bool,
    pub(crate) recommend_open_in_review: bool,
    pub(crate) truncation_notice: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiWorkspaceInlineDiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceInlineDiffLine {
    pub(crate) kind: AiWorkspaceInlineDiffLineKind,
    pub(crate) old_line: Option<u32>,
    pub(crate) new_line: Option<u32>,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceInlineDiffHunk {
    pub(crate) header: String,
    pub(crate) lines: Vec<AiWorkspaceInlineDiffLine>,
    pub(crate) trailing_meta: Vec<String>,
    pub(crate) truncated_line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiWorkspaceInlineDiffFile {
    pub(crate) old_path: Option<String>,
    pub(crate) new_path: Option<String>,
    pub(crate) display_path: String,
    pub(crate) prelude_meta: Vec<String>,
    pub(crate) hunks: Vec<AiWorkspaceInlineDiffHunk>,
    pub(crate) epilogue_meta: Vec<String>,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) truncated_hunk_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiWorkspaceInlineDiffProjection {
    pub(crate) files: Vec<AiWorkspaceInlineDiffFile>,
    pub(crate) total_added: usize,
    pub(crate) total_removed: usize,
    pub(crate) truncated_file_count: usize,
}

#[derive(Debug, Clone)]
struct AiWorkspaceInlineDiffFileSection {
    old_path: Option<String>,
    new_path: Option<String>,
    patch: String,
}

pub(crate) fn ai_workspace_project_inline_diff(
    diff_text: &str,
    options: AiWorkspaceInlineDiffOptions,
) -> AiWorkspaceInlineDiffProjection {
    if diff_text.trim().is_empty() || options.max_files == 0 {
        return AiWorkspaceInlineDiffProjection::default();
    }

    let file_sections = split_ai_workspace_inline_diff_sections(diff_text);
    let truncated_file_count = file_sections.len().saturating_sub(options.max_files);
    let mut projection = AiWorkspaceInlineDiffProjection {
        truncated_file_count,
        ..AiWorkspaceInlineDiffProjection::default()
    };

    for section in file_sections.into_iter().take(options.max_files) {
        let file = ai_workspace_project_inline_diff_file(section, options);
        projection.total_added = projection.total_added.saturating_add(file.added);
        projection.total_removed = projection.total_removed.saturating_add(file.removed);
        projection.files.push(file);
    }

    projection
}

pub(crate) fn ai_workspace_inline_diff_presentation_policy(
    projection: &AiWorkspaceInlineDiffProjection,
    options: AiWorkspaceInlineDiffOptions,
) -> AiWorkspaceInlineDiffPresentationPolicy {
    let truncated = ai_workspace_inline_diff_is_truncated(projection);
    let total_changed_lines = projection
        .total_added
        .saturating_add(projection.total_removed);

    AiWorkspaceInlineDiffPresentationPolicy {
        collapsed_by_default: !projection.files.is_empty(),
        recommend_open_in_review: truncated
            || total_changed_lines > AI_WORKSPACE_INLINE_DIFF_REVIEW_CHANGED_LINE_THRESHOLD,
        truncation_notice: truncated.then(|| {
            let total_files = ai_workspace_inline_diff_total_file_count(projection);
            if projection.truncated_file_count > 0 {
                format!(
                    "Preview truncated in the AI thread. Showing {} of {total_files} files. Open in Review for the full diff.",
                    options.max_files.min(total_files)
                )
            } else {
                "Preview truncated in the AI thread. Some hunks or lines are omitted. Open in Review for the full diff.".to_string()
            }
        }),
    }
}

fn ai_workspace_project_inline_diff_file(
    section: AiWorkspaceInlineDiffFileSection,
    options: AiWorkspaceInlineDiffOptions,
) -> AiWorkspaceInlineDiffFile {
    let document = parse_patch_document(section.patch.as_str());
    let display_path = ai_workspace_inline_diff_display_path(
        section.old_path.as_deref(),
        section.new_path.as_deref(),
        &document.prelude,
    );
    let added = document
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| line.kind == DiffLineKind::Added)
        .count();
    let removed = document
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| line.kind == DiffLineKind::Removed)
        .count();
    let truncated_hunk_count = document
        .hunks
        .len()
        .saturating_sub(options.max_hunks_per_file);
    let hunks = document
        .hunks
        .iter()
        .take(options.max_hunks_per_file)
        .map(|hunk| {
            let truncated_line_count = hunk.lines.len().saturating_sub(options.max_lines_per_hunk);
            let lines = hunk
                .lines
                .iter()
                .take(options.max_lines_per_hunk)
                .map(|line| AiWorkspaceInlineDiffLine {
                    kind: match line.kind {
                        DiffLineKind::Context => AiWorkspaceInlineDiffLineKind::Context,
                        DiffLineKind::Added => AiWorkspaceInlineDiffLineKind::Added,
                        DiffLineKind::Removed => AiWorkspaceInlineDiffLineKind::Removed,
                    },
                    old_line: line.old_line,
                    new_line: line.new_line,
                    text: line.text.clone(),
                })
                .collect::<Vec<_>>();

            AiWorkspaceInlineDiffHunk {
                header: hunk.header.clone(),
                lines,
                trailing_meta: hunk.trailing_meta.clone(),
                truncated_line_count,
            }
        })
        .collect::<Vec<_>>();

    AiWorkspaceInlineDiffFile {
        old_path: section.old_path,
        new_path: section.new_path,
        display_path,
        prelude_meta: filter_ai_workspace_inline_diff_meta_lines(document.prelude),
        hunks,
        epilogue_meta: document.epilogue,
        added,
        removed,
        truncated_hunk_count,
    }
}

fn split_ai_workspace_inline_diff_sections(
    diff_text: &str,
) -> Vec<AiWorkspaceInlineDiffFileSection> {
    let mut sections = Vec::<AiWorkspaceInlineDiffFileSection>::new();
    let mut current_lines = Vec::<String>::new();
    let mut current_paths = None::<(String, String)>;

    for line in diff_text.lines() {
        if let Some((old_path, new_path)) = ai_workspace_inline_diff_file_header_paths(line) {
            if !current_lines.is_empty() {
                sections.push(AiWorkspaceInlineDiffFileSection {
                    old_path: current_paths.as_ref().map(|(old, _)| old.clone()),
                    new_path: current_paths.as_ref().map(|(_, new)| new.clone()),
                    patch: current_lines.join("\n"),
                });
                current_lines.clear();
            }
            current_paths = Some((old_path, new_path));
        }
        current_lines.push(line.to_string());
    }

    if !current_lines.is_empty() {
        sections.push(AiWorkspaceInlineDiffFileSection {
            old_path: current_paths.as_ref().map(|(old, _)| old.clone()),
            new_path: current_paths.as_ref().map(|(_, new)| new.clone()),
            patch: current_lines.join("\n"),
        });
    }

    sections
}

fn ai_workspace_inline_diff_file_header_paths(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("diff"), Some("--git"), Some(old_path), Some(new_path)) => {
            Some((old_path.to_string(), new_path.to_string()))
        }
        _ => None,
    }
}

fn ai_workspace_inline_diff_is_truncated(projection: &AiWorkspaceInlineDiffProjection) -> bool {
    projection.truncated_file_count > 0
        || projection.files.iter().any(|file| {
            file.truncated_hunk_count > 0
                || file.hunks.iter().any(|hunk| hunk.truncated_line_count > 0)
        })
}

fn ai_workspace_inline_diff_total_file_count(
    projection: &AiWorkspaceInlineDiffProjection,
) -> usize {
    projection
        .files
        .len()
        .saturating_add(projection.truncated_file_count)
}

fn ai_workspace_inline_diff_display_path(
    old_path: Option<&str>,
    new_path: Option<&str>,
    prelude: &[String],
) -> String {
    if let Some(path) = new_path
        .map(normalize_ai_workspace_inline_diff_new_path)
        .filter(|path| *path != "/dev/null")
    {
        return path.to_string();
    }

    if let Some(path) = old_path
        .map(normalize_ai_workspace_inline_diff_old_path)
        .filter(|path| *path != "/dev/null")
    {
        return path.to_string();
    }

    for line in prelude {
        if let Some(path) = line
            .strip_prefix("+++ ")
            .map(normalize_ai_workspace_inline_diff_new_path)
            .filter(|path| *path != "/dev/null")
        {
            return path.to_string();
        }
        if let Some(path) = line
            .strip_prefix("--- ")
            .map(normalize_ai_workspace_inline_diff_old_path)
            .filter(|path| *path != "/dev/null")
        {
            return path.to_string();
        }
    }

    DEFAULT_DISPLAY_PATH.to_string()
}

fn normalize_ai_workspace_inline_diff_old_path(path: &str) -> &str {
    path.strip_prefix("a/").unwrap_or(path)
}

fn normalize_ai_workspace_inline_diff_new_path(path: &str) -> &str {
    path.strip_prefix("b/").unwrap_or(path)
}

fn filter_ai_workspace_inline_diff_meta_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .filter(|line| {
            !line.starts_with("diff --git ")
                && !line.starts_with("--- ")
                && !line.starts_with("+++ ")
        })
        .collect()
}
