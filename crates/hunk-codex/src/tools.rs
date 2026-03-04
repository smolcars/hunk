use std::collections::BTreeMap;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use codex_app_server_protocol::DynamicToolCallOutputContentItem;
use codex_app_server_protocol::DynamicToolCallParams;
use codex_app_server_protocol::DynamicToolCallResponse;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltInTool {
    WorkspaceSummary,
    ListDirectory,
    ReadFile,
}

#[derive(Debug, Clone)]
pub struct DynamicToolRegistry {
    handlers: BTreeMap<String, BuiltInTool>,
}

impl Default for DynamicToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicToolRegistry {
    pub fn new() -> Self {
        let handlers = [
            (
                "hunk.workspace_summary".to_string(),
                BuiltInTool::WorkspaceSummary,
            ),
            (
                "hunk.list_directory".to_string(),
                BuiltInTool::ListDirectory,
            ),
            ("hunk.read_file".to_string(), BuiltInTool::ReadFile),
        ]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        Self { handlers }
    }

    pub fn execute(&self, cwd: &Path, params: &DynamicToolCallParams) -> DynamicToolCallResponse {
        let Some(tool) = self.handlers.get(params.tool.as_str()) else {
            return error_response(format!("unsupported dynamic tool '{}'", params.tool));
        };

        match tool {
            BuiltInTool::WorkspaceSummary => self.workspace_summary(cwd),
            BuiltInTool::ListDirectory => self.list_directory(cwd, &params.arguments),
            BuiltInTool::ReadFile => self.read_file(cwd, &params.arguments),
        }
    }

    fn workspace_summary(&self, cwd: &Path) -> DynamicToolCallResponse {
        let entries = match fs::read_dir(cwd) {
            Ok(entries) => entries,
            Err(error) => {
                return error_response(format!(
                    "failed to read workspace '{}': {error}",
                    cwd.display()
                ));
            }
        };

        let mut file_count = 0usize;
        let mut dir_count = 0usize;
        for entry in entries.filter_map(Result::ok) {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                dir_count = dir_count.saturating_add(1);
            } else if file_type.is_file() {
                file_count = file_count.saturating_add(1);
            }
        }

        success_json(serde_json::json!({
            "cwd": cwd,
            "fileCount": file_count,
            "directoryCount": dir_count,
        }))
    }

    fn list_directory(&self, cwd: &Path, arguments: &Value) -> DynamicToolCallResponse {
        let args = match parse_tool_args::<ListDirectoryArgs>(arguments) {
            Ok(args) => args,
            Err(error) => return error_response(error),
        };

        let relative_path = args.path.unwrap_or_else(|| ".".to_string());
        let target_path = match safe_join(cwd, relative_path.as_str()) {
            Ok(path) => path,
            Err(error) => return error_response(error),
        };

        let max_entries = args.max_entries.unwrap_or(200).clamp(1, 2_000);
        let entries = match fs::read_dir(&target_path) {
            Ok(entries) => entries,
            Err(error) => {
                return error_response(format!(
                    "failed to list directory '{}': {error}",
                    target_path.display()
                ));
            }
        };

        let mut listed = Vec::new();
        for entry in entries.filter_map(Result::ok).take(max_entries) {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !args.include_hidden.unwrap_or(false) && file_name.starts_with('.') {
                continue;
            }

            let (entry_type, size_bytes) = match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => ("directory", None),
                Ok(file_type) if file_type.is_file() => {
                    let size = entry.metadata().ok().map(|metadata| metadata.len());
                    ("file", size)
                }
                Ok(file_type) if file_type.is_symlink() => ("symlink", None),
                Ok(_) => ("other", None),
                Err(_) => ("unknown", None),
            };
            listed.push(serde_json::json!({
                "name": file_name,
                "entryType": entry_type,
                "sizeBytes": size_bytes,
            }));
        }

        success_json(serde_json::json!({
            "path": target_path,
            "entries": listed,
        }))
    }

    fn read_file(&self, cwd: &Path, arguments: &Value) -> DynamicToolCallResponse {
        let args = match parse_tool_args::<ReadFileArgs>(arguments) {
            Ok(args) => args,
            Err(error) => return error_response(error),
        };

        let target_path = match safe_join(cwd, args.path.as_str()) {
            Ok(path) => path,
            Err(error) => return error_response(error),
        };
        let max_bytes = args.max_bytes.unwrap_or(32_000).clamp(1, 512_000);

        let bytes = match fs::read(&target_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return error_response(format!(
                    "failed to read file '{}': {error}",
                    target_path.display()
                ));
            }
        };

        let truncated = bytes.len() > max_bytes;
        let visible = bytes.into_iter().take(max_bytes).collect::<Vec<_>>();
        let text = String::from_utf8_lossy(&visible).to_string();
        success_json(serde_json::json!({
            "path": target_path,
            "truncated": truncated,
            "content": text,
        }))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListDirectoryArgs {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    include_hidden: Option<bool>,
    #[serde(default)]
    max_entries: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadFileArgs {
    path: String,
    #[serde(default)]
    max_bytes: Option<usize>,
}

fn parse_tool_args<T>(arguments: &Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|error| format!("invalid dynamic tool arguments: {error}"))
}

fn safe_join(cwd: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("parent path traversal is not allowed".to_string());
    }

    let workspace_root = fs::canonicalize(cwd)
        .map_err(|error| format!("failed to resolve workspace root '{}': {error}", cwd.display()))?;
    let target = cwd.join(path);
    let resolved = fs::canonicalize(&target)
        .map_err(|error| format!("failed to resolve target path '{}': {error}", target.display()))?;
    if !resolved.starts_with(&workspace_root) {
        return Err(format!(
            "path '{}' escapes workspace root '{}'",
            target.display(),
            workspace_root.display()
        ));
    }
    Ok(resolved)
}

fn success_json(value: Value) -> DynamicToolCallResponse {
    match serde_json::to_string_pretty(&value) {
        Ok(text) => DynamicToolCallResponse {
            content_items: vec![DynamicToolCallOutputContentItem::InputText { text }],
            success: true,
        },
        Err(error) => error_response(format!(
            "failed to serialize dynamic tool response: {error}"
        )),
    }
}

fn error_response(message: String) -> DynamicToolCallResponse {
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText { text: message }],
        success: false,
    }
}
