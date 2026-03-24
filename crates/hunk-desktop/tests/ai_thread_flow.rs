use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
#[path = "../src/app/ai_thread_flow.rs"]
mod ai_thread_flow;

use ai_thread_flow::{
    AiCodexGenerationConfig, AiCommitGenerationContext, ai_branch_name_for_prompt,
    ai_commit_subject_for_thread, try_ai_branch_name_for_prompt, try_ai_commit_message,
    wait_for_command_completion,
};
use hunk_codex::state::{AiState, ItemStatus, ItemSummary};

fn item(
    item_id: &str,
    thread_id: &str,
    kind: &str,
    content: &str,
    last_sequence: u64,
) -> ItemSummary {
    ItemSummary {
        id: item_id.to_string(),
        thread_id: thread_id.to_string(),
        turn_id: "turn-1".to_string(),
        kind: kind.to_string(),
        status: ItemStatus::Completed,
        content: content.to_string(),
        display_metadata: None,
        last_sequence,
    }
}

#[test]
fn branch_name_for_prompt_uses_mode_prefix() {
    let local = ai_branch_name_for_prompt("Add OAuth login flow", false);
    let worktree = ai_branch_name_for_prompt("Add OAuth login flow", true);

    assert!(local.starts_with("ai/local/"));
    assert!(worktree.starts_with("ai/worktree/"));
}

#[test]
fn branch_name_for_prompt_filters_noise_words() {
    let branch = ai_branch_name_for_prompt(
        "Implement the ability to add and remove reviewers in the PR panel",
        false,
    );
    assert!(branch.starts_with("ai/local/implement-ability-add-remove-reviewers-pr"));
}

#[test]
fn commit_subject_for_thread_prefers_latest_agent_message_line() {
    let mut state = AiState::default();
    state.items.insert(
        "item-1".to_string(),
        item(
            "item-1",
            "thread-1",
            "agentMessage",
            "Added branch/worktree selection to New.\nAlso removed dropdown.",
            12,
        ),
    );
    state.items.insert(
        "item-2".to_string(),
        item(
            "item-2",
            "thread-1",
            "agentMessage",
            "Refined timeline header.",
            22,
        ),
    );

    let subject = ai_commit_subject_for_thread(&state, "thread-1", "feature/old");
    assert_eq!(subject, "Refined timeline header");
}

#[test]
fn commit_subject_for_thread_falls_back_to_branch_name() {
    let state = AiState::default();
    let subject = ai_commit_subject_for_thread(&state, "thread-1", "feature/open-pr-flow");
    assert_eq!(subject, "Update open pr flow");
}

#[test]
fn branch_name_generation_uses_dedicated_model_and_image_inputs() {
    let temp_dir = TestTempDir::new();
    let args_path = temp_dir.path().join("args.txt");
    let codex_path = write_fake_codex_script(
        temp_dir.path(),
        "record-branch-codex.sh",
        Some(args_path.as_path()),
        r#"{"branch":"fix screenshot overflow"}"#,
    );
    let image_path = temp_dir.path().join("screenshot.png");
    fs::write(image_path.as_path(), b"png").expect("image fixture");

    let branch = try_ai_branch_name_for_prompt(
        codex_path.as_path(),
        temp_dir.path(),
        "",
        std::slice::from_ref(&image_path),
        true,
    )
    .expect("branch name");

    assert_eq!(branch, "ai/worktree/fix-screenshot-overflow");

    let args = fs::read_to_string(args_path).expect("args");
    assert!(args.contains("--model\ngpt-5.4-mini\n"));
    assert!(args.contains("model_reasoning_effort=\"low\""));
    assert!(args.contains("--image\n"));
    assert!(args.contains(image_path.to_string_lossy().as_ref()));
}

#[test]
fn commit_message_generation_truncates_non_ascii_output_safely() {
    let temp_dir = TestTempDir::new();
    let subject = "🙂".repeat(80);
    let body = "é".repeat(2_050);
    let output_json = format!(r#"{{"subject":"{subject}","body":"{body}"}}"#);
    let codex_path = write_fake_codex_script(
        temp_dir.path(),
        "commit-codex.sh",
        None,
        output_json.as_str(),
    );

    let message = try_ai_commit_message(
        AiCodexGenerationConfig {
            codex_executable: codex_path.as_path(),
            repo_root: temp_dir.path(),
        },
        AiCommitGenerationContext {
            branch_name: "ai/local/fix-screenshot-overflow",
            changed_files_summary: "M src/app.rs",
            diff_patch: "diff --git a/src/app.rs b/src/app.rs\n",
        },
    )
    .expect("commit message");

    assert_eq!(message.subject.chars().count(), 72);
    assert_eq!(
        message.body.as_deref().expect("body").chars().count(),
        2_000
    );
}

#[test]
fn codex_wait_timeout_kills_hung_process() {
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg("sleep 5")
        .spawn()
        .expect("spawn");
    let started_at = Instant::now();

    let status = wait_for_command_completion(&mut child, Duration::from_millis(20));

    assert!(status.is_none());
    assert!(started_at.elapsed() < Duration::from_secs(1));
}

fn write_fake_codex_script(
    temp_dir: &Path,
    file_name: &str,
    args_path: Option<&Path>,
    output_json: &str,
) -> PathBuf {
    let script_path = temp_dir.join(file_name);
    let args_recording = args_path
        .map(|path| {
            format!(
                "args_path={}\n: > \"$args_path\"\nfor arg in \"$@\"; do\n  printf '%s\\n' \"$arg\" >> \"$args_path\"\ndone\n",
                shell_quote(path.to_string_lossy().as_ref())
            )
        })
        .unwrap_or_default();
    let script = format!(
        "#!/bin/sh\nset -eu\n{args_recording}output_path=\"\"\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--output-last-message\" ]; then\n    shift\n    output_path=\"$1\"\n    break\n  fi\n  shift\ndone\nif [ -z \"$output_path\" ]; then\n  exit 1\nfi\nprintf '%s' {} > \"$output_path\"\ncat >/dev/null\n",
        shell_quote(output_json)
    );
    fs::write(script_path.as_path(), script).expect("script");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(script_path.as_path())
            .expect("metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(script_path.as_path(), permissions).expect("permissions");
    }
    script_path
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new() -> Self {
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(format!("hunk-ai-thread-flow-tests-{unique_suffix}"));
        fs::create_dir_all(path.as_path()).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.path.as_path());
    }
}
