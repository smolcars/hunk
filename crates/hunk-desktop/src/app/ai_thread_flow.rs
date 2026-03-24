use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hunk_codex::state::AiState;
use hunk_git::branch::sanitize_branch_name;

const MAX_BRANCH_SLUG_TOKENS: usize = 6;
const MAX_BRANCH_SLUG_LEN: usize = 48;
const MAX_COMMIT_SUBJECT_LEN: usize = 72;
const MAX_COMMIT_BODY_LEN: usize = 2_000;
const MAX_BRANCH_REQUEST_CONTEXT_LEN: usize = 4_000;
const MAX_CHANGED_FILES_CONTEXT_LEN: usize = 6_000;
const MAX_PATCH_CONTEXT_LEN: usize = 40_000;
const DEFAULT_AI_GIT_TEXT_MODEL: &str = "gpt-5.4-mini";
const DEFAULT_AI_GIT_REASONING_EFFORT: &str = "low";
const AI_GIT_TEXT_TIMEOUT: Duration = Duration::from_secs(20);
const AI_GIT_TEXT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const BRANCH_STOP_WORDS: &[&str] = &[
    "a", "an", "and", "as", "at", "be", "for", "from", "in", "into", "is", "it", "of", "on", "or",
    "that", "the", "this", "to", "with",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AiCommitMessage {
    pub subject: String,
    pub body: Option<String>,
}

impl AiCommitMessage {
    pub(super) fn as_git_message(&self) -> String {
        if let Some(body) = self.body.as_deref() {
            let body = body.trim();
            if !body.is_empty() {
                return format!("{}\n\n{}", self.subject, body);
            }
        }
        self.subject.clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AiCodexGenerationConfig<'a> {
    pub codex_executable: &'a Path,
    pub repo_root: &'a Path,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AiCommitGenerationContext<'a> {
    pub branch_name: &'a str,
    pub changed_files_summary: &'a str,
    pub diff_patch: &'a str,
}

pub(super) fn ai_branch_name_for_prompt(prompt: &str, worktree_mode: bool) -> String {
    let prefix = if worktree_mode {
        "ai/worktree"
    } else {
        "ai/local"
    };
    let slug = ai_branch_slug_for_prompt(prompt);
    sanitize_branch_name(format!("{prefix}/{slug}").as_str())
}

pub(super) fn ai_branch_name_for_thread(
    state: &AiState,
    thread_id: &str,
    fallback_branch_name: &str,
    worktree_mode: bool,
) -> String {
    let prompt = ai_first_prompt_for_thread(state, thread_id, fallback_branch_name);

    ai_branch_name_for_prompt(prompt.as_str(), worktree_mode)
}

pub(super) fn ai_first_prompt_seed_for_thread(state: &AiState, thread_id: &str) -> Option<String> {
    state
        .items
        .values()
        .filter(|item| item.thread_id == thread_id && item.kind == "userMessage")
        .min_by_key(|item| item.last_sequence)
        .map(|item| item.content.trim().to_string())
        .filter(|prompt| !prompt.is_empty())
}

pub(super) fn ai_latest_agent_message_for_thread(
    state: &AiState,
    thread_id: &str,
) -> Option<String> {
    state
        .items
        .values()
        .filter(|item| item.thread_id == thread_id && item.kind == "agentMessage")
        .max_by_key(|item| item.last_sequence)
        .map(|item| item.content.trim().to_string())
        .filter(|message| !message.is_empty())
}

pub(super) fn ai_commit_subject_for_thread(
    state: &AiState,
    thread_id: &str,
    fallback_branch_name: &str,
) -> String {
    let latest_message = ai_latest_agent_message_for_thread(state, thread_id);
    if let Some(message) = latest_message
        && let Some(subject) = normalized_commit_subject_line(message.as_str())
    {
        return subject;
    }
    ai_fallback_commit_subject(fallback_branch_name)
}

pub(super) fn ai_commit_message_for_thread(
    state: &AiState,
    thread_id: &str,
    fallback_branch_name: &str,
) -> AiCommitMessage {
    AiCommitMessage {
        subject: ai_commit_subject_for_thread(state, thread_id, fallback_branch_name),
        body: None,
    }
}

pub(super) fn ai_branch_generation_seed_for_thread(
    state: &AiState,
    thread_id: &str,
    fallback_branch_name: &str,
) -> String {
    ai_first_prompt_seed_for_thread(state, thread_id)
        .or_else(|| ai_latest_agent_message_for_thread(state, thread_id))
        .or_else(|| {
            state
                .threads
                .get(thread_id)
                .and_then(|thread| thread.title.clone())
        })
        .unwrap_or_else(|| fallback_branch_name.to_string())
}

pub(super) fn try_ai_branch_name_for_prompt(
    codex_executable: &Path,
    repo_root: &Path,
    prompt: &str,
    image_paths: &[PathBuf],
    worktree_mode: bool,
) -> Option<String> {
    let prompt = prompt.trim();
    if prompt.is_empty() && image_paths.is_empty() {
        return None;
    }
    let request_context = if prompt.is_empty() {
        "(none)".to_string()
    } else {
        limit_text(prompt, MAX_BRANCH_REQUEST_CONTEXT_LEN)
    };

    let branch_prefix = if worktree_mode {
        "ai/worktree"
    } else {
        "ai/local"
    };
    let mut generation_prompt = format!(
        "Generate a concise git branch fragment for this request.\n\
Return strict JSON with one key: branch.\n\
Rules:\n\
- 2 to 6 words.\n\
- lowercase words only.\n\
- no prefix like feature/ or ai/.\n\
- no trailing punctuation.\n\
- use attached images as primary context for visual/UI requests.\n\
\n\
User request:\n{}\n",
        request_context
    );
    if !image_paths.is_empty() {
        generation_prompt.push_str(format!("\nAttached images: {}\n", image_paths.len()).as_str());
    }
    let output_schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["branch"],
        "properties": {
            "branch": { "type": "string" }
        }
    });
    let generated = run_codex_json_generation(
        codex_executable,
        repo_root,
        generation_prompt.as_str(),
        &output_schema,
        image_paths,
    )?;
    let branch = generated
        .get("branch")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let fragment = ai_branch_slug_for_prompt(branch);
    let candidate = sanitize_branch_name(format!("{branch_prefix}/{fragment}").as_str());
    if candidate.trim().is_empty() {
        return None;
    }
    Some(candidate)
}

pub(super) fn try_ai_commit_message(
    config: AiCodexGenerationConfig<'_>,
    context: AiCommitGenerationContext<'_>,
) -> Option<AiCommitMessage> {
    if context.changed_files_summary.trim().is_empty() && context.diff_patch.trim().is_empty() {
        return None;
    }

    let prompt = format!(
        "Generate a git commit message for these working copy changes.\n\
Return strict JSON with keys: subject, body.\n\
Rules:\n\
- subject must be imperative and under 72 characters.\n\
- subject must not end with a period.\n\
- body can be an empty string.\n\
- body should only include critical context not obvious from the diff.\n\
- capture the primary user-visible or developer-visible change.\n\
\n\
Current branch:\n{}\n\
\n\
Changed files:\n{}\n\
\n\
Diff patch:\n{}\n",
        context.branch_name,
        limit_text(context.changed_files_summary, MAX_CHANGED_FILES_CONTEXT_LEN),
        limit_text(context.diff_patch, MAX_PATCH_CONTEXT_LEN),
    );
    let output_schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["subject", "body"],
        "properties": {
            "subject": { "type": "string" },
            "body": { "type": "string" }
        }
    });
    let generated = run_codex_json_generation(
        config.codex_executable,
        config.repo_root,
        prompt.as_str(),
        &output_schema,
        &[],
    )?;
    let subject = generated
        .get("subject")
        .and_then(|value| value.as_str())
        .and_then(normalize_commit_subject)?;
    let body = generated
        .get("body")
        .and_then(|value| value.as_str())
        .map(normalize_commit_body)
        .unwrap_or_default();
    Some(AiCommitMessage {
        subject,
        body: (!body.is_empty()).then_some(body),
    })
}

fn ai_branch_slug_for_prompt(prompt: &str) -> String {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in prompt.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
            continue;
        }

        if !current.is_empty() {
            push_branch_slug_token(current.as_str(), &mut tokens);
            current.clear();
        }
    }

    if !current.is_empty() {
        push_branch_slug_token(current.as_str(), &mut tokens);
    }

    if tokens.is_empty() {
        return "change".to_string();
    }

    let mut slug = tokens
        .into_iter()
        .take(MAX_BRANCH_SLUG_TOKENS)
        .collect::<Vec<_>>()
        .join("-");
    if slug.len() > MAX_BRANCH_SLUG_LEN {
        slug.truncate(MAX_BRANCH_SLUG_LEN);
        slug = slug.trim_matches('-').to_string();
    }
    if slug.is_empty() {
        "change".to_string()
    } else {
        slug
    }
}

fn push_branch_slug_token(candidate: &str, tokens: &mut Vec<String>) {
    let token = candidate.trim_matches('-');
    if token.is_empty() || BRANCH_STOP_WORDS.contains(&token) {
        return;
    }
    tokens.push(token.to_string());
}

fn normalized_commit_subject_line(message: &str) -> Option<String> {
    for line in message.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = trimmed
            .trim_start_matches('#')
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim_start_matches('>')
            .trim();
        if cleaned.is_empty() {
            continue;
        }

        let mut subject = cleaned
            .replace("`", "")
            .replace("**", "")
            .replace("__", "")
            .replace('\t', " ");
        while subject.contains("  ") {
            subject = subject.replace("  ", " ");
        }
        subject = subject.trim().trim_end_matches('.').trim().to_string();
        if subject.is_empty() {
            continue;
        }
        if subject.chars().count() > MAX_COMMIT_SUBJECT_LEN {
            subject = truncate_text_chars(subject.as_str(), MAX_COMMIT_SUBJECT_LEN);
            subject = subject.trim().to_string();
        }
        if subject.is_empty() {
            continue;
        }
        return Some(subject);
    }
    None
}

fn ai_fallback_commit_subject(branch_name: &str) -> String {
    let branch_segment = branch_name
        .split('/')
        .next_back()
        .unwrap_or(branch_name)
        .trim();
    if branch_segment.is_empty() || branch_segment == "unknown" || branch_segment == "detached" {
        return "Update project files".to_string();
    }

    let mut title = branch_segment.replace(['-', '_'], " ");
    while title.contains("  ") {
        title = title.replace("  ", " ");
    }
    let title = title.trim();
    if title.is_empty() {
        "Update project files".to_string()
    } else {
        format!("Update {title}")
    }
}

fn ai_first_prompt_for_thread(
    state: &AiState,
    thread_id: &str,
    fallback_branch_name: &str,
) -> String {
    ai_first_prompt_seed_for_thread(state, thread_id)
        .or_else(|| {
            state
                .threads
                .get(thread_id)
                .and_then(|thread| thread.title.clone())
        })
        .unwrap_or_else(|| fallback_branch_name.to_string())
}

fn run_codex_json_generation(
    codex_executable: &Path,
    repo_root: &Path,
    prompt: &str,
    schema: &serde_json::Value,
    image_paths: &[PathBuf],
) -> Option<serde_json::Value> {
    let unique_suffix = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos()
    );
    let schema_path = std::env::temp_dir().join(format!("hunk-codex-schema-{unique_suffix}.json"));
    let output_path = std::env::temp_dir().join(format!("hunk-codex-output-{unique_suffix}.json"));

    fs::write(&schema_path, serde_json::to_vec(schema).ok()?).ok()?;
    fs::write(&output_path, b"").ok()?;

    let mut command = std::process::Command::new(codex_executable);
    command
        .current_dir(repo_root)
        .arg("exec")
        .arg("--ephemeral")
        .arg("-s")
        .arg("read-only")
        .arg("--skip-git-repo-check")
        .arg("--output-schema")
        .arg(schema_path.as_path())
        .arg("--output-last-message")
        .arg(output_path.as_path())
        .arg("--model")
        .arg(DEFAULT_AI_GIT_TEXT_MODEL);
    for image_path in image_paths {
        command.arg("--image").arg(image_path.as_path());
    }
    command
        .arg("--config")
        .arg(format!(
            "model_reasoning_effort=\"{}\"",
            DEFAULT_AI_GIT_REASONING_EFFORT
        ))
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let output = (|| {
        let mut child = command.spawn().ok()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).ok()?;
        }
        let status = wait_for_command_completion(&mut child, AI_GIT_TEXT_TIMEOUT)?;
        if !status.success() {
            return None;
        }
        let payload = fs::read_to_string(output_path.as_path()).ok()?;
        serde_json::from_str::<serde_json::Value>(payload.as_str()).ok()
    })();

    let _ = fs::remove_file(schema_path.as_path());
    let _ = fs::remove_file(output_path.as_path());
    output
}

fn normalize_commit_subject(subject: &str) -> Option<String> {
    normalized_commit_subject_line(subject)
}

fn normalize_commit_body(body: &str) -> String {
    let mut normalized = body
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if normalized.chars().count() > MAX_COMMIT_BODY_LEN {
        normalized = truncate_text_chars(normalized.as_str(), MAX_COMMIT_BODY_LEN);
        normalized = normalized.trim().to_string();
    }
    normalized
}

fn limit_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let mut truncated = truncate_text_chars(text, max_len);
    truncated.push_str("\n[truncated]");
    truncated
}

fn truncate_text_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

pub(super) fn wait_for_command_completion(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Option<std::process::ExitStatus> {
    let started_at = Instant::now();
    loop {
        if let Some(status) = child.try_wait().ok()? {
            return Some(status);
        }
        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(AI_GIT_TEXT_POLL_INTERVAL);
    }
}
