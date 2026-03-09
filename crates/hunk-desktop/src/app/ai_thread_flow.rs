use hunk_codex::state::AiState;
use hunk_git::branch::sanitize_branch_name;

const MAX_BRANCH_SLUG_TOKENS: usize = 6;
const MAX_BRANCH_SLUG_LEN: usize = 48;
const MAX_COMMIT_SUBJECT_LEN: usize = 72;
const BRANCH_STOP_WORDS: &[&str] = &[
    "a", "an", "and", "as", "at", "be", "for", "from", "in", "into", "is", "it", "of", "on", "or",
    "that", "the", "this", "to", "with",
];

pub(super) fn ai_branch_name_for_prompt(prompt: &str, worktree_mode: bool) -> String {
    let prefix = if worktree_mode {
        "ai/worktree"
    } else {
        "ai/local"
    };
    let slug = ai_branch_slug_for_prompt(prompt);
    sanitize_branch_name(format!("{prefix}/{slug}").as_str())
}

pub(super) fn ai_commit_subject_for_thread(
    state: &AiState,
    thread_id: &str,
    fallback_branch_name: &str,
) -> String {
    let latest_message = state
        .items
        .values()
        .filter(|item| item.thread_id == thread_id && item.kind == "agentMessage")
        .max_by_key(|item| item.last_sequence)
        .map(|item| item.content.as_str());
    if let Some(message) = latest_message
        && let Some(subject) = normalized_commit_subject_line(message)
    {
        return subject;
    }
    ai_fallback_commit_subject(fallback_branch_name)
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
        if subject.len() > MAX_COMMIT_SUBJECT_LEN {
            subject.truncate(MAX_COMMIT_SUBJECT_LEN);
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
